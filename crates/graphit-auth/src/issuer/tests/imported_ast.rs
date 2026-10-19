use super::service_credentials::{Server, USER_SUB, broker_config, context, now_seconds};
use super::*;
use crate::broker::{BrokerRequestContext, oauth::web::WebOAuth};

#[test]
fn native_imported_ast_read_requires_its_own_live_read_authority() {
    use super::service_credentials::{local_config, local_profile, put};
    use graphit_core::module_settings::Module;

    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    put(
        &store,
        local_config(),
        local_profile("reader", "fixture-imported-reader", None, false),
        true,
        &ctx,
    );
    let reads = [Permission::Read(Module::Ast)];
    let allowed = RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        Arc::clone(&store),
        authority(&reads, &reads, limits(), 7200),
        ttl(),
        &ctx,
    )
    .unwrap();
    let runtime = allowed.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let local = allowed
        .authenticate_local("fixture-imported-reader", &ctx)
        .unwrap();
    for caller in [&runtime, &local] {
        let read = allowed
            .admit_authenticated_imported_ast_read(caller, f.realm())
            .unwrap();
        assert!(std::ptr::eq(read.request(), &ctx));
        assert!(std::ptr::eq(read.installation().live, &f.installation));
        read.recheck().unwrap();
        let other = Fixture::new(&ctx);
        for wrong in [
            other.realm(),
            ScopeCapture {
                live: &f.project,
                record: &f.project_record,
            },
        ] {
            assert!(matches!(
                allowed.admit_authenticated_imported_ast_read(caller, wrong),
                Err(AuthError::Denied)
            ));
        }
    }
    let denied = issuer(&f, store, &ctx, false);
    let denied_caller = denied.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(matches!(
        denied.admit_authenticated_imported_ast_read(&denied_caller, f.realm()),
        Err(AuthError::Denied)
    ));
    assert!(matches!(
        denied.admit_authenticated_imported_ast_read(&runtime, f.realm()),
        Err(AuthError::Denied)
    ));
    let read = allowed
        .admit_authenticated_imported_ast_read(&runtime, f.realm())
        .unwrap();
    cancel.cancel();
    assert!(matches!(read.recheck(), Err(AuthError::Request)));
    assert!(matches!(
        allowed.admit_authenticated_imported_ast_read(&runtime, f.realm()),
        Err(AuthError::Request)
    ));
}

#[tokio::test]
async fn imported_ast_embedding_uses_its_own_web_admission_and_local_profile() {
    use super::service_credentials::{cpu, local_config, local_profile, policy, put};
    use crate::{
        ServiceAdmissionLimits, ServiceBindingView, ServiceMode, ServiceOrigin, ServicePurpose,
        ServiceRuleInput, ServiceUseError, profile::ProfileError,
    };

    let server = Server::new().await;
    server.userinfo_status.store(503, Ordering::SeqCst);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    put(
        &store,
        local_config(),
        local_profile("models", "fixture-mcp-key", None, false),
        true,
        &ctx,
    );
    let added = store
        .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
            doc.add_provider(
                "broker".into(),
                broker_config(&server.origin, true, false),
                doc.revision(),
                1,
                ctx,
            )
        })
        .unwrap();
    assert!(added.failure().is_none());
    let revision = store
        .load(&ctx)
        .unwrap()
        .document()
        .provider("broker")
        .unwrap()
        .revision();
    let rules = [ServiceRuleInput::Local {
        origin: ServiceOrigin::Web,
        provider: "service",
        purpose: ServicePurpose::Embedding,
        execution: cpu(),
    }];
    let configured_policy = policy(
        &rules,
        &[],
        limits(),
        ServiceAdmissionLimits {
            max_rules: 16,
            max_binding_bytes: 8192,
        },
        Duration::ZERO,
    )
    .unwrap()
    .with_web_imported_ast_read("broker")
    .unwrap();
    let issuer = RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        Arc::clone(&store),
        configured_policy,
        ttl(),
        &ctx,
    )
    .unwrap();
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
    let caller = flow
        .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let read = issuer
        .admit_web_imported_ast_read(&caller, &verifier, &ctx, "broker", f.realm())
        .unwrap();
    let foreign = self::issuer(&f, store, &ctx, true);
    assert!(matches!(
        foreign.capture_imported_ast_embedding_service(&read),
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));
    let keys = server.count("/keys");
    let mut selected = issuer
        .capture_imported_ast_embedding_service(&read)
        .unwrap();
    assert_eq!(selected.mode().unwrap(), ServiceMode::LocalAuthorized);
    assert!(matches!(
        selected.credential(),
        Err(ServiceUseError::WrongMode)
    ));
    assert!(matches!(
        selected.binding().unwrap(),
        ServiceBindingView::Local {
            provider: "service",
            purpose: ServicePurpose::Embedding,
            ..
        }
    ));
    let lease = selected.local().unwrap();
    assert_eq!(lease.execution().unwrap(), cpu());
    assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
    lease.check().unwrap();
    assert_eq!(server.count("/keys"), keys);
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
    cancel.cancel();
    assert!(matches!(
        lease.check(),
        Err(ServiceUseError::Profile(ProfileError::Cancelled))
    ));
    assert!(matches!(
        issuer.capture_imported_ast_embedding_service(&read),
        Err(ServiceUseError::Profile(ProfileError::Cancelled))
    ));
}

fn store(f: &Fixture, ctx: &RequestContext<'_>) -> Arc<AuthStore> {
    f.auth_store_policy(
        ctx,
        crate::profile::ProfileLimits {
            bytes: 4096,
            nodes: 4096,
            scalar_bytes: 2048,
            collection_entries: 64,
            providers: 16,
            profiles: 16,
        },
    )
}
fn provider(store: &AuthStore, server: &Server, ctx: &RequestContext<'_>) -> u64 {
    let result = store
        .mutate(store.load(ctx).unwrap(), ctx, |doc, ctx| {
            doc.add_provider(
                "service".into(),
                broker_config(&server.origin, true, false),
                doc.revision(),
                1,
                ctx,
            )
        })
        .unwrap();
    assert!(result.failure().is_none());
    store
        .load(ctx)
        .unwrap()
        .document()
        .provider("service")
        .unwrap()
        .revision()
}
fn issuer(
    f: &Fixture,
    store: Arc<AuthStore>,
    ctx: &RequestContext<'_>,
    enabled: bool,
) -> RoleIssuer {
    let policy = authority(&[], &[], limits(), 7200);
    let policy = if enabled {
        policy.with_web_imported_ast_read("service").unwrap()
    } else {
        policy
    };
    RoleIssuer::new(f.realm(), Arc::clone(&f.runtime), store, policy, ttl(), ctx).unwrap()
}
fn web_limits() -> crate::broker::oauth::OAuthLimits {
    crate::broker::oauth::OAuthLimits {
        authorization_url_bytes: 8192,
        callback_bytes: 8192,
        callback_query_entries: 8,
        form_body_bytes: 8192,
        token_bytes: 8192,
        response_nodes: 32,
        response_scalar_bytes: 8192,
    }
}
fn access(server: &Server, resource: &str, expiry: u64) -> String {
    server.sign(&serde_json::json!({"iss":server.origin,"sub":USER_SUB,"aud":["broker-api",resource],
        "client_id":"web-client","jti":"import-read","token_use":"access","iat":now_seconds().saturating_sub(1),"exp":expiry}))
}

#[tokio::test]
async fn web_imported_ast_read_offline_and_exact_borrowed_installation() {
    let server = Server::new().await;
    server.userinfo_status.store(503, Ordering::SeqCst);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let revision = provider(&store, &server, &ctx);
    let issuer = issuer(&f, Arc::clone(&store), &ctx, true);
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
    let knowledge = RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        Arc::clone(&store),
        authority(&[], &[], limits(), 7200)
            .with_web_imported_knowledge_read("service")
            .unwrap(),
        ttl(),
        &ctx,
    )
    .unwrap();
    assert!(matches!(
        knowledge.admit_web_imported_ast_read(&proof, &verifier, &ctx, "service", f.realm()),
        Err(AuthError::Denied)
    ));
    assert!(matches!(
        issuer.admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "service", f.realm()),
        Err(AuthError::Denied)
    ));
    let keys = server.count("/keys");
    let read = issuer
        .admit_web_imported_ast_read(&proof, &verifier, &ctx, "service", f.realm())
        .unwrap();
    assert!(std::ptr::eq(read.request(), &ctx));
    assert!(std::ptr::eq(read.installation().live, &f.installation));
    assert!(std::ptr::eq(
        read.installation().record,
        &f.installation_record
    ));
    for _ in 0..3 {
        read.recheck().unwrap();
    }
    assert_eq!(server.count("/keys"), keys);
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
    assert!(store.load(&ctx).unwrap().document().profiles().is_empty());
}

#[tokio::test]
async fn web_imported_ast_default_deny_wrong_binding_original_and_scope() {
    let server = Server::new().await;
    let foreign = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let revision = provider(&store, &server, &ctx);
    let allowed = issuer(&f, Arc::clone(&store), &ctx, true);
    let denied = issuer(&f, store, &ctx, false);
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
    assert!(matches!(
        denied.admit_web_imported_ast_read(&proof, &verifier, &ctx, "service", f.realm()),
        Err(AuthError::Denied)
    ));
    assert!(matches!(
        allowed.admit_web_imported_ast_read(&proof, &verifier, &ctx, "unknown", f.realm()),
        Err(AuthError::Denied)
    ));
    let other_verifier = server.verifier(false, revision);
    let foreign_verifier = foreign.verifier(false, revision);
    for wrong in [&other_verifier, &foreign_verifier] {
        assert!(
            allowed
                .admit_web_imported_ast_read(&proof, wrong, &ctx, "service", f.realm())
                .is_err()
        );
    }
    let other_original = context(&cancel);
    assert!(
        allowed
            .admit_web_imported_ast_read(&proof, &verifier, &other_original, "service", f.realm())
            .is_err()
    );
    let other = Fixture::new(&ctx);
    for scope in [
        other.realm(),
        ScopeCapture {
            live: &f.project,
            record: &f.project_record,
        },
    ] {
        assert!(matches!(
            allowed.admit_web_imported_ast_read(&proof, &verifier, &ctx, "service", scope),
            Err(AuthError::Denied)
        ));
    }
    let paths = InstallationPaths::resolve(
        Brand::default(),
        &f.source_root,
        None,
        Some(f.installation.owner_root()),
    )
    .unwrap();
    let StoreOwner::Installation { installation_id } = f.installation.owner() else {
        panic!("installation")
    };
    let user = CapturedPublicationScope::user(
        &paths,
        installation_id.clone(),
        graphit_core::artifact_store::UserIdentity::new(server.origin.clone(), USER_SUB.into())
            .unwrap(),
        ScopeLimits::new(1, 4096, 128).unwrap(),
        ctx.snapshot(),
    )
    .unwrap();
    let user_record = user.binding().unwrap();
    assert!(matches!(
        allowed.admit_web_imported_ast_read(
            &proof,
            &verifier,
            &ctx,
            "service",
            ScopeCapture {
                live: &user,
                record: &user_record
            }
        ),
        Err(AuthError::Denied)
    ));
    let wrong_resource = access(
        &server,
        "https://other.example/resource",
        now_seconds() + 600,
    );
    assert!(
        flow.verify_access(
            &registered,
            &wrong_resource,
            BrokerRequestContext::new(ctx.snapshot())
        )
        .await
        .is_err()
    );
    let wrong_issuer=foreign.sign(&serde_json::json!({"iss":foreign.origin,"sub":USER_SUB,"aud":["broker-api",registered.resource()],
        "client_id":"web-client","token_use":"access","iat":now_seconds()-1,"exp":now_seconds()+600}));
    assert!(
        flow.verify_access(
            &registered,
            &wrong_issuer,
            BrokerRequestContext::new(ctx.snapshot())
        )
        .await
        .is_err()
    );
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(foreign.count("/userinfo"), 0);
}

#[test]
fn native_imported_ast_embedding_keeps_runtime_and_local_origins() {
    use super::service_credentials::{cpu, local_config, local_profile, policy, put};
    use crate::{
        ServiceAdmissionLimits, ServiceMode, ServiceOrigin, ServicePurpose, ServiceRuleInput,
        ServiceUseError,
    };
    use graphit_core::module_settings::Module;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    put(
        &store,
        local_config(),
        local_profile("models", "fixture-native-ast", None, false),
        true,
        &ctx,
    );
    let rules = [
        ServiceRuleInput::Local {
            origin: ServiceOrigin::Runtime,
            provider: "service",
            purpose: ServicePurpose::Embedding,
            execution: cpu(),
        },
        ServiceRuleInput::Local {
            origin: ServiceOrigin::Local,
            provider: "service",
            purpose: ServicePurpose::Embedding,
            execution: cpu(),
        },
    ];
    let configured = policy(
        &rules,
        &[Permission::Read(Module::Ast)],
        limits(),
        ServiceAdmissionLimits {
            max_rules: 16,
            max_binding_bytes: 8192,
        },
        Duration::ZERO,
    )
    .unwrap();
    let issuer = RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        Arc::clone(&store),
        configured,
        ttl(),
        &ctx,
    )
    .unwrap();
    let foreign = self::issuer(&f, store, &ctx, true);
    let runtime = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let local = issuer
        .authenticate_local("fixture-native-ast", &ctx)
        .unwrap();
    for (caller, origin) in [
        (&runtime, ServiceOrigin::Runtime),
        (&local, ServiceOrigin::Local),
    ] {
        let read = issuer
            .admit_authenticated_imported_ast_read(caller, f.realm())
            .unwrap();
        assert_eq!(read.origin(), origin);
        assert!(matches!(
            foreign.capture_imported_ast_embedding_service(&read),
            Err(ServiceUseError::Auth(AuthError::Denied))
        ));
        let mut selected = issuer
            .capture_imported_ast_embedding_service(&read)
            .unwrap();
        assert_eq!(selected.mode().unwrap(), ServiceMode::LocalAuthorized);
        assert!(matches!(
            selected.credential(),
            Err(ServiceUseError::WrongMode)
        ));
        let lease = selected.local().unwrap();
        assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
        assert_eq!(lease.execution().unwrap(), cpu());
        lease.check().unwrap();
    }
    let read = issuer
        .admit_authenticated_imported_ast_read(&runtime, f.realm())
        .unwrap();
    cancel.cancel();
    assert!(
        issuer
            .capture_imported_ast_embedding_service(&read)
            .is_err()
    );
}

#[tokio::test]
async fn web_imported_ast_services_keep_its_web_identity_and_ignore_unrelated_active_profile() {
    use super::service_credentials::{local_config, local_profile, policy};
    use crate::{
        CredentialSource, RemoteServiceBindingInput, ServiceAdmissionLimits, ServiceBindingView,
        ServiceMode, ServiceOrigin, ServicePurpose, ServiceRuleInput, ServiceUseError,
    };
    for other_profile in [false, true] {
        for binding_variant in 0..3 {
            let server = Server::new().await;
            server.userinfo_status.store(503, Ordering::SeqCst);
            let cancel = Cancellation::default();
            let ctx = context(&cancel);
            let f = Fixture::new(&ctx);
            let store = store(&f, &ctx);
            let revision = provider(&store, &server, &ctx);
            if other_profile {
                let mut data =
                    local_profile("other-account", "other-account-private-mcp", None, false);
                data.provider = "other".into();
                let result = store
                    .mutate(store.load(&ctx).unwrap(), &ctx, |document, ctx| {
                        document.add_provider(
                            "other".into(),
                            local_config(),
                            document.revision(),
                            1,
                            ctx,
                        )?;
                        let expected = crate::profile::ProfileExpectation {
                            document_revision: document.revision(),
                            provider_revision: document.provider("other").unwrap().revision(),
                            profile_revision: None,
                        };
                        document.replace_profile_data(data, expected, 1, ctx)?;
                        document.activate("other-account", document.revision(), ctx)?;
                        Ok(())
                    })
                    .unwrap();
                assert!(result.failure().is_none());
            }
            let rules: Vec<_> = [
                ServicePurpose::HubRead,
                ServicePurpose::Embedding,
                ServicePurpose::Rerank,
            ]
            .into_iter()
            .map(|purpose| ServiceRuleInput::Remote {
                origin: ServiceOrigin::Web,
                binding: RemoteServiceBindingInput::Broker {
                    provider: if binding_variant == 1 {
                        "other"
                    } else {
                        "service"
                    },
                    purpose,
                    endpoint: &server.origin,
                    resource: (binding_variant == 2).then_some("https://other.example/resource"),
                },
                credential: CredentialSource::IncomingBearer,
                incoming: None,
            })
            .collect();
            let configured = policy(
                &rules,
                &[],
                limits(),
                ServiceAdmissionLimits {
                    max_rules: 16,
                    max_binding_bytes: 8192,
                },
                Duration::ZERO,
            )
            .unwrap()
            .with_web_imported_ast_read("service")
            .unwrap();
            let issuer = RoleIssuer::new(
                f.realm(),
                Arc::clone(&f.runtime),
                Arc::clone(&store),
                configured,
                ttl(),
                &ctx,
            )
            .unwrap();
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
            let read = issuer
                .admit_web_imported_ast_read(&proof, &verifier, &ctx, "service", f.realm())
                .unwrap();
            if binding_variant != 0 {
                assert!(
                    issuer
                        .capture_imported_ast_embedding_service(&read)
                        .is_err()
                );
                assert!(issuer.capture_imported_ast_rerank_service(&read).is_err());
                continue;
            }
            assert!(matches!(
                issuer.capture_imported_ast_hub_service(&read, "other"),
                Err(ServiceUseError::Auth(AuthError::Denied))
            ));
            let keys = server.count("/keys");
            for purpose in [
                ServicePurpose::HubRead,
                ServicePurpose::Embedding,
                ServicePurpose::Rerank,
            ] {
                let mut selected = match purpose {
                    ServicePurpose::HubRead => {
                        issuer.capture_imported_ast_hub_service(&read, "service")
                    }
                    ServicePurpose::Embedding => {
                        issuer.capture_imported_ast_embedding_service(&read)
                    }
                    _ => issuer.capture_imported_ast_rerank_service(&read),
                }
                .unwrap();
                assert_eq!(selected.mode().unwrap(), ServiceMode::BrokerIncoming);
                assert!(
                    matches!(selected.binding().unwrap(), ServiceBindingView::Broker { provider: "service", purpose: actual, provider_revision, resource: None, .. } if actual == purpose && provider_revision == revision)
                );
                let lease = selected.credential().unwrap();
                assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
                assert!(Arc::ptr_eq(
                    lease.original().unwrap().snapshot(),
                    read.request().snapshot()
                ));
                assert!(lease.credential().unwrap() == Some(raw.as_str()));
                let subject =
                    crate::issuer::user_subject::ServiceSubjectProof::subject_identity(&lease)
                        .unwrap()
                        .unwrap();
                assert_eq!(subject.subject, USER_SUB);
                assert!(matches!(
                    subject.origin,
                    crate::issuer::user_subject::VerifiedUserOrigin::Web
                ));
                for _ in 0..3 {
                    lease.check().unwrap();
                }
                assert_eq!(server.count("/keys"), keys);
                assert_eq!(server.count("/userinfo"), 0);
                assert_eq!(server.count("/token"), 0);
                if purpose == ServicePurpose::Rerank {
                    cancel.cancel();
                    assert!(matches!(
                        lease.check(),
                        Err(ServiceUseError::Profile(
                            crate::profile::ProfileError::Cancelled
                        ))
                    ));
                }
            }
        }
    }
}

#[test]
fn native_imported_ast_hub_read_requires_real_remote_request_permission() {
    use super::service_credentials::policy;
    use crate::{
        CredentialSource, RemoteServiceBindingInput, ServiceAdmissionLimits, ServiceMode,
        ServiceOrigin, ServicePurpose, ServiceRuleInput, ServiceUseError,
    };
    for allow_remote in [false, true] {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = store(&f, &ctx);
        let result = store
            .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                doc.add_provider(
                    "service".into(),
                    broker_config("https://broker.example", false, true),
                    doc.revision(),
                    1,
                    ctx,
                )
            })
            .unwrap();
        assert!(result.failure().is_none());
        let rules = [ServiceRuleInput::Remote {
            origin: ServiceOrigin::Runtime,
            binding: RemoteServiceBindingInput::Broker {
                provider: "service",
                purpose: ServicePurpose::HubRead,
                endpoint: "https://broker.example",
                resource: None,
            },
            credential: CredentialSource::Anonymous,
            incoming: None,
        }];
        let mut permissions = vec![Permission::Read(graphit_core::module_settings::Module::Ast)];
        if allow_remote {
            permissions.push(Permission::External(crate::ExternalEffect::RemoteRequest));
        }
        let configured = policy(
            &rules,
            &permissions,
            limits(),
            ServiceAdmissionLimits {
                max_rules: 16,
                max_binding_bytes: 8192,
            },
            Duration::ZERO,
        )
        .unwrap();
        let issuer = RoleIssuer::new(
            f.realm(),
            Arc::clone(&f.runtime),
            store,
            configured,
            ttl(),
            &ctx,
        )
        .unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let read = issuer
            .admit_authenticated_imported_ast_read(&caller, f.realm())
            .unwrap();
        let selected = issuer.capture_imported_ast_hub_service(&read, "service");
        if allow_remote {
            let mut selected = selected.unwrap();
            assert_eq!(selected.mode().unwrap(), ServiceMode::BrokerAnonymous);
            let lease = selected.credential().unwrap();
            assert!(lease.credential().unwrap().is_none());
            assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
        } else {
            assert!(matches!(
                selected,
                Err(ServiceUseError::Auth(AuthError::Denied))
            ));
        }
    }
}
