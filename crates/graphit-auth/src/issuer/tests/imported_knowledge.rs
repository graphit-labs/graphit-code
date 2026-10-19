use super::service_credentials::{Server, USER_SUB, broker_config, context, now_seconds};
use super::*;
use crate::broker::{BrokerRequestContext, oauth::web::WebOAuth};

#[test]
fn native_imported_knowledge_read_requires_its_own_live_read_authority() {
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
    let reads = [Permission::Read(Module::Knowledge)];
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
            .admit_authenticated_imported_knowledge_read(caller, f.realm())
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
                allowed.admit_authenticated_imported_knowledge_read(caller, wrong),
                Err(AuthError::Denied)
            ));
        }
    }
    let denied = issuer(&f, store, &ctx, false);
    let denied_caller = denied.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(matches!(
        denied.admit_authenticated_imported_knowledge_read(&denied_caller, f.realm()),
        Err(AuthError::Denied)
    ));
    assert!(matches!(
        denied.admit_authenticated_imported_knowledge_read(&runtime, f.realm()),
        Err(AuthError::Denied)
    ));
    let read = allowed
        .admit_authenticated_imported_knowledge_read(&runtime, f.realm())
        .unwrap();
    cancel.cancel();
    assert!(matches!(read.recheck(), Err(AuthError::Request)));
    assert!(matches!(
        allowed.admit_authenticated_imported_knowledge_read(&runtime, f.realm()),
        Err(AuthError::Request)
    ));
}

pub(super) fn store(f: &Fixture, ctx: &RequestContext<'_>) -> Arc<AuthStore> {
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
pub(super) fn provider(store: &AuthStore, server: &Server, ctx: &RequestContext<'_>) -> u64 {
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
        policy.with_web_imported_knowledge_read("service").unwrap()
    } else {
        policy
    };
    RoleIssuer::new(f.realm(), Arc::clone(&f.runtime), store, policy, ttl(), ctx).unwrap()
}
pub(super) fn web_limits() -> crate::broker::oauth::OAuthLimits {
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
pub(super) fn access(server: &Server, resource: &str, expiry: u64) -> String {
    server.sign(&serde_json::json!({"iss":server.origin,"sub":USER_SUB,"aud":["broker-api",resource],
        "client_id":"web-client","jti":"import-read","token_use":"access","iat":now_seconds().saturating_sub(1),"exp":expiry}))
}

#[tokio::test]
async fn web_imported_knowledge_read_offline_and_exact_borrowed_installation() {
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
    let keys = server.count("/keys");
    let read = issuer
        .admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "service", f.realm())
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
async fn web_imported_knowledge_default_deny_wrong_binding_original_and_scope() {
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
        denied.admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "service", f.realm()),
        Err(AuthError::Denied)
    ));
    assert!(matches!(
        allowed.admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "unknown", f.realm()),
        Err(AuthError::Denied)
    ));
    let other_verifier = server.verifier(false, revision);
    let foreign_verifier = foreign.verifier(false, revision);
    for wrong in [&other_verifier, &foreign_verifier] {
        assert!(
            allowed
                .admit_web_imported_knowledge_read(&proof, wrong, &ctx, "service", f.realm())
                .is_err()
        );
    }
    let other_original = context(&cancel);
    assert!(
        allowed
            .admit_web_imported_knowledge_read(
                &proof,
                &verifier,
                &other_original,
                "service",
                f.realm()
            )
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
            allowed.admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "service", scope),
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
        allowed.admit_web_imported_knowledge_read(
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

#[tokio::test]
async fn web_imported_knowledge_live_configuration_and_installation_replacement() {
    for physical in [false, true] {
        let server = Server::new().await;
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
        let read = issuer
            .admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "service", f.realm())
            .unwrap();
        if physical {
            let root = f.installation.owner_root();
            std::fs::rename(root, root.with_extension("replaced")).unwrap();
            std::fs::create_dir(root).unwrap();
        } else {
            let result = store
                .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                    let p = doc.provider("service").unwrap();
                    let revision = p.revision();
                    let now = p.updated_at() + 1;
                    let config = p.config().clone();
                    doc.update_provider("service", config, doc.revision(), revision, now, ctx)
                })
                .unwrap();
            assert!(result.failure().is_none());
        }
        assert!(matches!(read.recheck(), Err(AuthError::Denied)));
        assert!(
            issuer
                .admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "service", f.realm())
                .is_err()
        );
        assert!(std::ptr::eq(read.request(), &ctx)); // Getter stays borrowed data, not authority validation.
        assert_eq!(server.count("/userinfo"), 0);
    }
}

#[tokio::test]
async fn web_imported_knowledge_expiry_cancel_and_deadline_without_network() {
    for ending in [0, 1, 2] {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let deadline = Instant::now() + Duration::from_millis(500);
        let ctx = if ending == 1 {
            request_before(&cancel, deadline)
        } else {
            context(&cancel)
        };
        let f = Fixture::new(&ctx);
        let store = store(&f, &ctx);
        let revision = provider(&store, &server, &ctx);
        let issuer = issuer(&f, store, &ctx, true);
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
        let expiry = now_seconds() + if ending == 2 { 2 } else { 600 };
        let raw = access(&server, registered.resource(), expiry);
        let proof = flow
            .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let read = issuer
            .admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "service", f.realm())
            .unwrap();
        let keys = server.count("/keys");
        match ending {
            0 => cancel.cancel(),
            1 => tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await,
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
        assert_eq!(
            read.recheck(),
            Err(if ending == 2 {
                AuthError::Expired
            } else {
                AuthError::Request
            })
        );
        assert!(matches!(
            issuer.admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "service", f.realm()),
            Err(AuthError::Expired | AuthError::Request)
        ));
        assert_eq!(server.count("/keys"), keys);
        assert_eq!(server.count("/userinfo"), 0);
        assert_eq!(server.count("/token"), 0);
    }
}

#[test]
fn web_imported_knowledge_policy_is_independent_literal_and_explicit() {
    let base = || authority(&[], &[], limits(), 7200);
    assert!(matches!(
        base().with_web_imported_knowledge_read(""),
        Err(AuthError::Invalid)
    ));
    assert!(matches!(
        base()
            .with_web_imported_knowledge_read("service")
            .unwrap()
            .with_web_imported_knowledge_read("service"),
        Err(AuthError::Invalid)
    ));
    let enabled = base().with_web_imported_knowledge_read("service").unwrap();
    assert!(enabled.incoming_user.is_empty());
    assert!(enabled.native_installation.is_empty());
    assert!(matches!(
        base().with_incoming_user_permissions(
            "service",
            &[Permission::Read(
                graphit_core::module_settings::Module::Task
            )]
        ),
        Err(AuthError::Invalid)
    ));
}

#[tokio::test]
async fn imported_knowledge_embedding_uses_its_own_web_admission_and_local_profile() {
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
    .with_web_imported_knowledge_read("broker")
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
        .admit_web_imported_knowledge_read(&caller, &verifier, &ctx, "broker", f.realm())
        .unwrap();
    let foreign = self::issuer(&f, Arc::clone(&store), &ctx, true);
    assert!(matches!(
        foreign.capture_imported_knowledge_embedding_service(&read),
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));
    let keys = server.count("/keys");
    let mut selected = issuer
        .capture_imported_knowledge_embedding_service(&read)
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
    let change = store
        .mutate(store.load(&ctx).unwrap(), &ctx, |document, ctx| {
            let provider = document.provider("broker").unwrap();
            let revision = provider.revision();
            let updated = provider.updated_at() + 1;
            let config = provider.config().clone();
            document.update_provider(
                "broker",
                config,
                document.revision(),
                revision,
                updated,
                ctx,
            )
        })
        .unwrap();
    assert!(change.failure().is_none());
    assert!(
        lease.check().is_err(),
        "changed imported Web provider invalidates the lease"
    );
    assert!(
        issuer
            .capture_imported_knowledge_embedding_service(&read)
            .is_err()
    );
    cancel.cancel();
    assert!(matches!(
        lease.check(),
        Err(ServiceUseError::Profile(ProfileError::Cancelled))
    ));
    assert!(matches!(
        issuer.capture_imported_knowledge_embedding_service(&read),
        Err(ServiceUseError::Profile(ProfileError::Cancelled))
    ));
}

#[test]
fn native_imported_knowledge_embedding_keeps_runtime_and_local_origins() {
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
        local_profile("models", "fixture-native-knowledge", None, false),
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
        &[Permission::Read(Module::Knowledge)],
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
        .authenticate_local("fixture-native-knowledge", &ctx)
        .unwrap();
    for (caller, origin) in [
        (&runtime, ServiceOrigin::Runtime),
        (&local, ServiceOrigin::Local),
    ] {
        let read = issuer
            .admit_authenticated_imported_knowledge_read(caller, f.realm())
            .unwrap();
        assert_eq!(read.origin(), origin);
        assert!(matches!(
            foreign.capture_imported_knowledge_embedding_service(&read),
            Err(ServiceUseError::Auth(AuthError::Denied))
        ));
        let mut selected = issuer
            .capture_imported_knowledge_embedding_service(&read)
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
        .admit_authenticated_imported_knowledge_read(&runtime, f.realm())
        .unwrap();
    cancel.cancel();
    assert!(
        issuer
            .capture_imported_knowledge_embedding_service(&read)
            .is_err()
    );
}

#[tokio::test]
async fn native_imported_ai_remote_services_keep_read_authority_and_require_remote_permission() {
    use super::service_credentials::{local_config, local_profile, native_profile, policy, put};
    use crate::profile::{AiService, DirectProtocol};
    use crate::{
        CredentialSource, ExternalEffect, RemoteServiceBindingInput, ServiceAdmissionLimits,
        ServiceMode, ServiceOrigin, ServicePurpose, ServiceRuleInput, ServiceUseError,
    };
    use graphit_core::module_settings::Module;
    let server = Server::new().await;
    for direct in [false, true] {
        for allow_remote in [false, true] {
            let cancel = Cancellation::default();
            let ctx = context(&cancel);
            let f = Fixture::new(&ctx);
            let store = store(&f, &ctx);
            let expiry = now_seconds() + 600;
            let token = server.token(true, expiry);
            let (config, profile) = if direct {
                let mut config = local_config();
                config.embedding = AiService::Direct {
                    protocol: DirectProtocol::OpenAiCompatible,
                    endpoint: server.origin.clone(),
                    model: "fixture-embedding".into(),
                    model_revision: "fixture-r1".into(),
                    recipe: "retrieval-v1".into(),
                    dimensions: 2,
                    request_dimensions: None,
                    batch_endpoint: None,
                };
                config.rerank = AiService::Direct {
                    protocol: DirectProtocol::Cohere,
                    endpoint: server.origin.clone(),
                    model: "fixture-rerank".into(),
                    model_revision: "fixture-r1".into(),
                    recipe: "native-score-v1".into(),
                    dimensions: 0,
                    request_dimensions: None,
                    batch_endpoint: None,
                };
                (
                    config,
                    local_profile("remote", "fixture-remote", None, false),
                )
            } else {
                (
                    broker_config(&server.origin, true, false),
                    native_profile(&server.origin, &token, expiry),
                )
            };
            put(&store, config, profile, true, &ctx);
            let revision = store
                .load(&ctx)
                .unwrap()
                .document()
                .provider("service")
                .unwrap()
                .revision();
            let mut rules = Vec::new();
            for origin in [ServiceOrigin::Runtime, ServiceOrigin::Local] {
                for purpose in [ServicePurpose::Embedding, ServicePurpose::Rerank] {
                    let binding = if direct {
                        RemoteServiceBindingInput::Direct {
                            provider: "service",
                            purpose,
                            endpoint: &server.origin,
                            protocol: if purpose == ServicePurpose::Embedding {
                                DirectProtocol::OpenAiCompatible
                            } else {
                                DirectProtocol::Cohere
                            },
                            model: if purpose == ServicePurpose::Embedding {
                                "fixture-embedding"
                            } else {
                                "fixture-rerank"
                            },
                            model_revision: "fixture-r1",
                            recipe: if purpose == ServicePurpose::Embedding {
                                "retrieval-v1"
                            } else {
                                "native-score-v1"
                            },
                            dimensions: if purpose == ServicePurpose::Embedding {
                                2
                            } else {
                                0
                            },
                            request_dimensions: None,
                            batch_endpoint: None,
                        }
                    } else {
                        RemoteServiceBindingInput::Broker {
                            provider: "service",
                            purpose,
                            endpoint: &server.origin,
                            resource: None,
                        }
                    };
                    rules.push(ServiceRuleInput::Remote {
                        origin,
                        binding,
                        credential: if direct {
                            CredentialSource::DirectKey
                        } else {
                            CredentialSource::NativeAccess
                        },
                        incoming: None,
                    });
                }
            }
            let mut permissions = vec![
                Permission::Read(Module::Knowledge),
                Permission::Read(Module::Ast),
            ];
            if allow_remote {
                permissions.push(Permission::External(ExternalEffect::RemoteRequest));
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
                Arc::clone(&store),
                configured,
                ttl(),
                &ctx,
            )
            .unwrap();
            let runtime = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
            let local = direct.then(|| issuer.authenticate_local("fixture-remote", &ctx).unwrap());
            let verifier = server.verifier(false, revision);
            for caller in std::iter::once(&runtime).chain(local.as_ref()) {
                let knowledge = issuer
                    .admit_authenticated_imported_knowledge_read(caller, f.realm())
                    .unwrap();
                let ast = issuer
                    .admit_authenticated_imported_ast_read(caller, f.realm())
                    .unwrap();
                for purpose in [ServicePurpose::Embedding, ServicePurpose::Rerank] {
                    let knowledge_selection = match purpose {
                        ServicePurpose::Embedding => {
                            issuer.capture_imported_knowledge_embedding_service(&knowledge)
                        }
                        _ => issuer.capture_imported_knowledge_rerank_service(&knowledge),
                    };
                    let ast_selection = match purpose {
                        ServicePurpose::Embedding => {
                            issuer.capture_imported_ast_embedding_service(&ast)
                        }
                        _ => issuer.capture_imported_ast_rerank_service(&ast),
                    };
                    for selection in [knowledge_selection, ast_selection] {
                        if !allow_remote {
                            assert!(matches!(
                                selection,
                                Err(ServiceUseError::Auth(AuthError::Denied))
                            ));
                            continue;
                        }
                        let mut selected = selection.unwrap();
                        assert_eq!(
                            selected.mode().unwrap(),
                            if direct {
                                ServiceMode::Direct
                            } else {
                                ServiceMode::BrokerNative
                            }
                        );
                        let lease = if direct {
                            selected.credential().unwrap()
                        } else {
                            selected.native_credential(&verifier).await.unwrap()
                        };
                        assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
                        assert!(lease.credential().unwrap().is_some());
                        lease.check().unwrap();
                    }
                }
            }
            if allow_remote {
                let read = issuer
                    .admit_authenticated_imported_knowledge_read(&runtime, f.realm())
                    .unwrap();
                let mut selected = issuer
                    .capture_imported_knowledge_rerank_service(&read)
                    .unwrap();
                let lease = if direct {
                    selected.credential().unwrap()
                } else {
                    selected.native_credential(&verifier).await.unwrap()
                };
                cancel.cancel();
                assert!(lease.check().is_err());
            }
        }
    }
}

#[tokio::test]
async fn web_imported_knowledge_services_keep_its_web_identity_and_ignore_unrelated_active_profile()
{
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
            .with_web_imported_knowledge_read("service")
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
                .admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "service", f.realm())
                .unwrap();
            if binding_variant != 0 {
                assert!(
                    issuer
                        .capture_imported_knowledge_embedding_service(&read)
                        .is_err()
                );
                assert!(
                    issuer
                        .capture_imported_knowledge_rerank_service(&read)
                        .is_err()
                );
                continue;
            }
            assert!(matches!(
                issuer.capture_imported_knowledge_hub_service(&read, "other"),
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
                        issuer.capture_imported_knowledge_hub_service(&read, "service")
                    }
                    ServicePurpose::Embedding => {
                        issuer.capture_imported_knowledge_embedding_service(&read)
                    }
                    _ => issuer.capture_imported_knowledge_rerank_service(&read),
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
fn native_imported_knowledge_hub_read_requires_real_remote_request_permission() {
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
        let mut permissions = vec![Permission::Read(
            graphit_core::module_settings::Module::Knowledge,
        )];
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
            .admit_authenticated_imported_knowledge_read(&caller, f.realm())
            .unwrap();
        let selected = issuer.capture_imported_knowledge_hub_service(&read, "service");
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
