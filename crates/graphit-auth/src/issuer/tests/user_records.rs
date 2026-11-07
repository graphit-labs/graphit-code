use super::service_credentials::{
    INCOMING_RESOURCE, Server, USER_SUB, broker_config, context, native_profile, now_seconds, put,
};
use super::*;
use crate::broker::BrokerRequestContext;
use crate::profile::{ProfileExpectation, ProviderKind};
use graphit_core::{artifact_store::UserIdentity, module_settings::Module};

mod native_admin;

const READ: [Permission; 2] = [Permission::ReadUser, Permission::Read(Module::Memory)];
const ALL: [Permission; 5] = [
    Permission::ReadUser,
    Permission::Read(Module::Memory),
    Permission::MutateUser,
    Permission::WriteMemory,
    Permission::WriteFencedMemory,
];

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
fn web_access(server: &Server, resource: &str, expiry: u64) -> String {
    server.sign(
        &serde_json::json!({"iss":server.origin,"sub":USER_SUB,"aud":["broker-api",resource],
        "client_id":"web-client","jti":"actual-web-user","token_use":"access",
        "iat":now_seconds().saturating_sub(1),"exp":expiry,"preferred_username":"display-not-sub"}),
    )
}

#[tokio::test]
async fn web_user_write_requires_own_policy_and_mints_stable_actor_with_fence() {
    use crate::broker::oauth::web::WebOAuth;
    let server = Server::new().await;
    server.userinfo_status.store(503, Ordering::SeqCst);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = profile_store(&f, &ctx);
    let revision = incoming_provider(&store, &server, &ctx);
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
    let raw = web_access(&server, registered.resource(), now_seconds() + 600);
    let proof = flow
        .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
    for permissions in [
        vec![],
        READ.to_vec(),
        vec![Permission::MutateUser],
        vec![Permission::WriteMemory],
        vec![Permission::MutateUser, Permission::WriteMemory],
        vec![Permission::MutateUser, Permission::WriteFencedMemory],
        ALL.to_vec(),
    ] {
        let issuer = make_issuer(&f, Arc::clone(&store), &ctx, &permissions, true);
        for (operation, permission, obligation) in [
            (
                UserMemoryWriteOperation::Memory,
                Permission::WriteMemory,
                Obligation::Unconditional,
            ),
            (
                UserMemoryWriteOperation::FencedMemory,
                Permission::WriteFencedMemory,
                Obligation::MemoryFenceRequired,
            ),
        ] {
            let write = issuer.admit_web_user_memory_write(
                &proof,
                &verifier,
                &ctx,
                "service",
                ScopeCapture {
                    live: &scope,
                    record: &record,
                },
                operation,
            );
            if permissions.contains(&Permission::MutateUser) && permissions.contains(&permission) {
                let write = write.unwrap();
                assert_eq!(write.recheck(), Ok(obligation));
                assert_eq!(write.operation().unwrap(), operation);
                assert_eq!(write.identity().unwrap().issuer(), server.origin);
                assert_eq!(write.identity().unwrap().subject(), USER_SUB);
                assert!(std::ptr::eq(write.request().unwrap(), &ctx));
                assert_eq!(write.user().unwrap().live.owner(), scope.owner());
                assert!(write.actor().unwrap() == write.actor().unwrap());
            } else {
                assert!(write.is_err());
            }
        }
    }
    let issuer = make_issuer(&f, Arc::clone(&store), &ctx, &ALL, true);
    let write = issuer
        .admit_web_user_memory_write(
            &proof,
            &verifier,
            &ctx,
            "service",
            ScopeCapture {
                live: &scope,
                record: &record,
            },
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    let changed = server.sign(&serde_json::json!({"iss":server.origin,"sub":USER_SUB,"aud":["broker-api",registered.resource()],"client_id":"web-client","jti":"renamed","token_use":"access","iat":now_seconds().saturating_sub(1),"exp":now_seconds()+600,"preferred_username":"another-display"}));
    let changed = flow
        .verify_access(
            &registered,
            &changed,
            BrokerRequestContext::new(ctx.snapshot()),
        )
        .await
        .unwrap();
    let renamed = issuer
        .admit_web_user_memory_write(
            &changed,
            &verifier,
            &ctx,
            "service",
            ScopeCapture {
                live: &scope,
                record: &record,
            },
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    assert!(write.actor().unwrap() == renamed.actor().unwrap());
    let denied = make_issuer(&f, store, &ctx, &ALL, false);
    assert!(
        denied
            .admit_web_user_memory_write(
                &proof,
                &verifier,
                &ctx,
                "service",
                ScopeCapture {
                    live: &scope,
                    record: &record
                },
                UserMemoryWriteOperation::Memory
            )
            .is_err()
    );
    let (foreign, foreign_record) = user(&f, &ctx, &server.origin, "other-subject");
    let other_raw = server.sign(&serde_json::json!({"iss":server.origin,"sub":"other-subject","aud":["broker-api",registered.resource()],"client_id":"web-client","jti":"other-principal","token_use":"access","iat":now_seconds().saturating_sub(1),"exp":now_seconds()+600,"preferred_username":"display-not-sub"}));
    let other_proof = flow
        .verify_access(
            &registered,
            &other_raw,
            BrokerRequestContext::new(ctx.snapshot()),
        )
        .await
        .unwrap();
    let other_write = issuer
        .admit_web_user_memory_write(
            &other_proof,
            &verifier,
            &ctx,
            "service",
            ScopeCapture {
                live: &foreign,
                record: &foreign_record,
            },
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    assert!(write.actor().unwrap() != other_write.actor().unwrap());
    let fresh = context(&cancel);
    for (request, provider, capture) in [
        (
            &fresh,
            "service",
            ScopeCapture {
                live: &scope,
                record: &record,
            },
        ),
        (
            &ctx,
            "unknown",
            ScopeCapture {
                live: &scope,
                record: &record,
            },
        ),
        (
            &ctx,
            "service",
            ScopeCapture {
                live: &foreign,
                record: &foreign_record,
            },
        ),
        (
            &ctx,
            "service",
            ScopeCapture {
                live: &f.project,
                record: &f.project_record,
            },
        ),
    ] {
        assert!(
            issuer
                .admit_web_user_memory_write(
                    &proof,
                    &verifier,
                    request,
                    provider,
                    capture,
                    UserMemoryWriteOperation::Memory
                )
                .is_err()
        );
    }
    let other = server.verifier(false, revision);
    assert!(
        issuer
            .admit_web_user_memory_write(
                &proof,
                &other,
                &ctx,
                "service",
                ScopeCapture {
                    live: &scope,
                    record: &record
                },
                UserMemoryWriteOperation::Memory
            )
            .is_err()
    );
    assert!(!scope.runtime_root().exists());
    assert!(!f.source_root.join("records").exists());
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
    cancel.cancel();
    assert_eq!(write.recheck(), Err(AuthError::Request));
    assert!(write.actor().is_err());
}

#[tokio::test]
async fn web_user_read_uses_registered_proof_and_explicit_policy_without_profile() {
    use crate::broker::oauth::web::WebOAuth;
    let server = Server::new().await;
    server.userinfo_status.store(503, Ordering::SeqCst);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = profile_store(&f, &ctx);
    let revision = incoming_provider(&store, &server, &ctx);
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
    let raw = web_access(&server, registered.resource(), now_seconds() + 600);
    let proof = flow
        .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
    for permissions in [vec![], vec![READ[0]], vec![READ[1]], READ.to_vec()] {
        let issuer = make_issuer(&f, Arc::clone(&store), &ctx, &permissions, true);
        let read = issuer.admit_web_user_memory_read(
            &proof,
            &verifier,
            &ctx,
            "service",
            ScopeCapture {
                live: &scope,
                record: &record,
            },
        );
        if permissions == READ {
            let read = read.unwrap();
            assert_eq!(read.identity().unwrap().issuer(), server.origin);
            assert_eq!(read.identity().unwrap().subject(), USER_SUB);
            assert!(Arc::ptr_eq(
                read.request().unwrap().snapshot(),
                ctx.snapshot()
            ));
            for _ in 0..3 {
                read.recheck().unwrap();
            }
        } else {
            assert!(read.is_err());
        }
    }
    let denied = make_issuer(&f, store, &ctx, &READ, false);
    assert!(
        denied
            .admit_web_user_memory_read(
                &proof,
                &verifier,
                &ctx,
                "service",
                ScopeCapture {
                    live: &scope,
                    record: &record
                }
            )
            .is_err()
    );
    assert!(!scope.runtime_root().exists());
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
    assert_eq!(server.count("/register"), 1);
}

#[tokio::test]
async fn web_user_read_rejects_foreign_identity_provider_verifier_and_original() {
    use crate::broker::oauth::web::WebOAuth;
    let a = Server::new().await;
    let b = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = profile_store(&f, &ctx);
    let revision = incoming_provider(&store, &a, &ctx);
    let issuer = make_issuer(&f, store, &ctx, &READ, true);
    let verifier = a.verifier(false, revision);
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
    let raw = web_access(&a, registered.resource(), now_seconds() + 600);
    let proof = flow
        .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let (scope, record) = user(&f, &ctx, &a.origin, USER_SUB);
    let (foreign, foreign_record) = user(&f, &ctx, &b.origin, USER_SUB);
    assert_ne!(scope.runtime_root(), foreign.runtime_root());
    assert!(
        issuer
            .admit_web_user_memory_read(
                &proof,
                &verifier,
                &ctx,
                "service",
                ScopeCapture {
                    live: &foreign,
                    record: &foreign_record
                }
            )
            .is_err()
    );
    assert!(
        issuer
            .admit_web_user_memory_read(
                &proof,
                &verifier,
                &ctx,
                "unknown",
                ScopeCapture {
                    live: &scope,
                    record: &record
                }
            )
            .is_err()
    );
    let fresh = context(&cancel);
    assert!(
        issuer
            .admit_web_user_memory_read(
                &proof,
                &verifier,
                &fresh,
                "service",
                ScopeCapture {
                    live: &scope,
                    record: &record
                }
            )
            .is_err()
    );
    let other = a.verifier(false, revision);
    assert!(
        issuer
            .admit_web_user_memory_read(
                &proof,
                &other,
                &ctx,
                "service",
                ScopeCapture {
                    live: &scope,
                    record: &record
                }
            )
            .is_err()
    );
    assert!(
        issuer
            .admit_web_user_memory_read(
                &proof,
                &verifier,
                &ctx,
                "service",
                ScopeCapture {
                    live: &f.project,
                    record: &f.project_record
                }
            )
            .is_err()
    );
    let other_fixture = Fixture::new(&ctx);
    let (other_scope, other_record) = user(&other_fixture, &ctx, &a.origin, USER_SUB);
    assert!(
        issuer
            .admit_web_user_memory_read(
                &proof,
                &verifier,
                &ctx,
                "service",
                ScopeCapture {
                    live: &other_scope,
                    record: &other_record
                }
            )
            .is_err()
    );
    assert!(!scope.runtime_root().exists());
}

#[tokio::test]
async fn web_user_read_live_config_and_physical_capture_invalidation_deny_output() {
    use crate::broker::oauth::web::WebOAuth;
    for physical in [false, true] {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = profile_store(&f, &ctx);
        let revision = incoming_provider(&store, &server, &ctx);
        let issuer = make_issuer(&f, Arc::clone(&store), &ctx, &READ, true);
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
        let raw = web_access(&server, registered.resource(), now_seconds() + 600);
        let proof = flow
            .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let (first, _) = user(&f, &ctx, &server.origin, USER_SUB);
        std::fs::create_dir_all(first.runtime_root()).unwrap();
        let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
        let read = issuer
            .admit_web_user_memory_read(
                &proof,
                &verifier,
                &ctx,
                "service",
                ScopeCapture {
                    live: &scope,
                    record: &record,
                },
            )
            .unwrap();
        if physical {
            let root = scope.user_root().unwrap();
            std::fs::rename(root, root.with_extension("replaced")).unwrap();
            std::fs::create_dir(root).unwrap();
        } else {
            let result = store
                .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                    let provider = doc.provider("service").unwrap();
                    let revision = provider.revision();
                    let now = provider.updated_at() + 1;
                    let config = provider.config().clone();
                    doc.update_provider("service", config, doc.revision(), revision, now, ctx)
                })
                .unwrap();
            assert!(result.failure().is_none());
        }
        assert!(read.recheck().is_err());
        assert!(read.identity().is_err());
        assert!(read.user().is_err());
        assert!(
            issuer
                .admit_web_user_memory_read(
                    &proof,
                    &verifier,
                    &ctx,
                    "service",
                    ScopeCapture {
                        live: &scope,
                        record: &record
                    }
                )
                .is_err()
        );
        assert_eq!(server.count("/userinfo"), 0);
    }
}

#[tokio::test]
async fn web_user_read_cancel_deadline_and_signed_expiry_end_original_without_network() {
    use crate::broker::oauth::web::WebOAuth;
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
        let store = profile_store(&f, &ctx);
        let revision = incoming_provider(&store, &server, &ctx);
        let issuer = make_issuer(&f, store, &ctx, &READ, true);
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
        let raw = web_access(&server, registered.resource(), expiry);
        let proof = flow
            .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
        let read = issuer
            .admit_web_user_memory_read(
                &proof,
                &verifier,
                &ctx,
                "service",
                ScopeCapture {
                    live: &scope,
                    record: &record,
                },
            )
            .unwrap();
        let keys = server.count("/keys");
        match ending {
            0 => cancel.cancel(),
            1 => tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await,
            _ => {
                let delay = std::time::UNIX_EPOCH
                    .checked_add(Duration::from_secs(expiry))
                    .unwrap()
                    .duration_since(SystemTime::now())
                    .unwrap()
                    + Duration::from_millis(10);
                tokio::time::sleep(delay).await;
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
        assert!(read.request().is_err());
        assert_eq!(server.count("/keys"), keys);
        assert_eq!(server.count("/userinfo"), 0);
        assert_eq!(server.count("/token"), 0);
    }
}

fn profile_store(f: &Fixture, ctx: &RequestContext<'_>) -> Arc<AuthStore> {
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
fn make_issuer(
    f: &Fixture,
    store: Arc<AuthStore>,
    ctx: &RequestContext<'_>,
    permissions: &[Permission],
    incoming: bool,
) -> RoleIssuer {
    let policy = authority(permissions, &all_permissions(), limits(), 7200);
    let policy = if incoming {
        policy
            .with_incoming_user_permissions("service", permissions)
            .unwrap()
    } else {
        policy
    };
    RoleIssuer::new(f.realm(), Arc::clone(&f.runtime), store, policy, ttl(), ctx).unwrap()
}
fn user(
    f: &Fixture,
    ctx: &RequestContext<'_>,
    issuer: &str,
    subject: &str,
) -> (CapturedPublicationScope, ScopeBindingRecord) {
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
    let scope = CapturedPublicationScope::user(
        &paths,
        installation_id.clone(),
        UserIdentity::new(issuer.into(), subject.into()).unwrap(),
        ScopeLimits::new(1, 4096, 128).unwrap(),
        ctx.snapshot(),
    )
    .unwrap();
    let record = scope.binding().unwrap();
    (scope, record)
}
fn incoming_provider(store: &AuthStore, server: &Server, ctx: &RequestContext<'_>) -> u64 {
    let mut config = broker_config(&server.origin, true, false);
    let ProviderKind::Broker(binding) = &mut config.kind else {
        unreachable!()
    };
    binding.mcp_resource = Some(INCOMING_RESOURCE.into());
    let result = store
        .mutate(store.load(ctx).unwrap(), ctx, |doc, ctx| {
            doc.add_provider("service".into(), config, doc.revision(), 1, ctx)
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

#[test]
fn incoming_user_policy_is_finite_explicit_default_deny() {
    assert!(
        authority(&[], &[], limits(), 7200)
            .with_incoming_user_permissions("", &READ)
            .is_err()
    );
    assert!(
        authority(&[], &[], limits(), 7200)
            .with_incoming_user_permissions("service", &[Permission::Configure])
            .is_err()
    );
    assert!(
        authority(&[], &[], limits(), 7200)
            .with_incoming_user_permissions("service", &[READ[0], READ[0]])
            .is_err()
    );
    assert!(
        authority(&[], &[], limits(), 7200)
            .with_incoming_user_permissions("service", &READ)
            .unwrap()
            .with_incoming_user_permissions("service", &READ)
            .is_err()
    );
}

#[tokio::test]
async fn native_admission_keeps_actual_subject_permissions_original_and_fence() {
    let server = Server::new().await;
    server.userinfo_status.store(503, Ordering::SeqCst);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = profile_store(&f, &ctx);
    let expiry = now_seconds() + 600;
    put(
        &store,
        broker_config(&server.origin, true, false),
        native_profile(&server.origin, &server.token(true, expiry), expiry),
        true,
        &ctx,
    );
    let issuer = make_issuer(&f, store, &ctx, &ALL, false);
    let revision = issuer
        .local
        .load(&ctx)
        .unwrap()
        .document()
        .provider("service")
        .unwrap()
        .revision();
    let verifier = server.verifier(false, revision);
    let caller = issuer
        .authenticate_runtime(&f.secret(), &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .unwrap();
    let identity = issuer.user_from_authenticated(&caller).unwrap();
    assert_eq!(identity.issuer().unwrap(), server.origin);
    assert_eq!(identity.subject().unwrap(), USER_SUB);
    let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
    let read = issuer
        .admit_authenticated_user_memory_read(
            &caller,
            &ctx,
            ScopeCapture {
                live: &scope,
                record: &record,
            },
        )
        .unwrap();
    assert!(Arc::ptr_eq(
        read.request().unwrap().snapshot(),
        ctx.snapshot()
    ));
    assert_eq!(read.identity().unwrap().subject(), USER_SUB);
    assert_eq!(read.user().unwrap().live.owner(), scope.owner());
    for operation in [
        UserMemoryWriteOperation::Memory,
        UserMemoryWriteOperation::FencedMemory,
    ] {
        let write = issuer
            .admit_authenticated_user_memory_write(
                &caller,
                &ctx,
                ScopeCapture {
                    live: &scope,
                    record: &record,
                },
                operation,
            )
            .unwrap();
        assert_eq!(write.operation().unwrap(), operation);
        assert_eq!(
            write.recheck().unwrap(),
            if operation == UserMemoryWriteOperation::Memory {
                Obligation::Unconditional
            } else {
                Obligation::MemoryFenceRequired
            }
        );
        let a = actor_bytes(&write.actor().unwrap());
        let b = actor_bytes(&caller.actor().unwrap());
        assert_eq!(a, b);
    }
    for _ in 0..3 {
        read.recheck().unwrap();
    }
    assert!(!scope.runtime_root().exists());
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
    let fresh = context(&cancel);
    assert!(
        issuer
            .admit_authenticated_user_memory_read(
                &caller,
                &fresh,
                ScopeCapture {
                    live: &scope,
                    record: &record
                }
            )
            .is_err()
    );
    let (other, other_record) = user(&f, &ctx, &server.origin, "another-sub");
    assert!(
        issuer
            .admit_authenticated_user_memory_read(
                &caller,
                &ctx,
                ScopeCapture {
                    live: &other,
                    record: &other_record
                }
            )
            .is_err()
    );
    assert!(
        issuer
            .admit_authenticated_user_memory_read(
                &caller,
                &ctx,
                ScopeCapture {
                    live: &f.project,
                    record: &f.project_record
                }
            )
            .is_err()
    );
    cancel.cancel();
    assert_eq!(read.recheck(), Err(AuthError::Request));
}

#[tokio::test]
async fn native_missing_subject_or_each_permission_denies_before_storage() {
    let server = Server::new().await;
    for permissions in [
        vec![],
        vec![READ[0]],
        vec![READ[1]],
        vec![Permission::MutateUser],
        vec![Permission::WriteMemory],
    ] {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = profile_store(&f, &ctx);
        let expiry = now_seconds() + 600;
        put(
            &store,
            broker_config(&server.origin, true, false),
            native_profile(&server.origin, &server.token(true, expiry), expiry),
            true,
            &ctx,
        );
        let issuer = make_issuer(&f, store, &ctx, &permissions, false);
        let revision = issuer
            .local
            .load(&ctx)
            .unwrap()
            .document()
            .provider("service")
            .unwrap()
            .revision();
        let verifier = server.verifier(false, revision);
        let caller = issuer
            .authenticate_runtime(&f.secret(), &ctx)
            .unwrap()
            .authenticate_native(&verifier)
            .await
            .unwrap();
        let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
        assert!(
            issuer
                .admit_authenticated_user_memory_read(
                    &caller,
                    &ctx,
                    ScopeCapture {
                        live: &scope,
                        record: &record
                    }
                )
                .is_err()
        );
        assert!(
            issuer
                .admit_authenticated_user_memory_write(
                    &caller,
                    &ctx,
                    ScopeCapture {
                        live: &scope,
                        record: &record
                    },
                    UserMemoryWriteOperation::Memory
                )
                .is_err()
        );
        assert!(!scope.runtime_root().exists());
    }
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = make_issuer(&f, profile_store(&f, &ctx), &ctx, &ALL, false);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
    assert!(issuer.user_from_authenticated(&caller).is_err());
    assert!(
        issuer
            .admit_authenticated_user_memory_read(
                &caller,
                &ctx,
                ScopeCapture {
                    live: &scope,
                    record: &record
                }
            )
            .is_err()
    );
    assert!(!scope.runtime_root().exists());
}

#[tokio::test]
async fn incoming_without_active_profile_uses_own_principal_and_explicit_policy() {
    let server = Server::new().await;
    server.userinfo_status.store(503, Ordering::SeqCst);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = profile_store(&f, &ctx);
    let revision = incoming_provider(&store, &server, &ctx);
    let verifier = server.verifier(true, revision);
    let raw = server.token(false, now_seconds() + 600);
    let proof = verifier
        .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let denied = make_issuer(&f, Arc::clone(&store), &ctx, &ALL, false);
    let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
    assert!(
        denied
            .admit_incoming_user_memory_read(
                &proof,
                &verifier,
                &ctx,
                "service",
                ScopeCapture {
                    live: &scope,
                    record: &record
                }
            )
            .is_err()
    );
    let issuer = make_issuer(&f, store, &ctx, &ALL, true);
    let read = issuer
        .admit_incoming_user_memory_read(
            &proof,
            &verifier,
            &ctx,
            "service",
            ScopeCapture {
                live: &scope,
                record: &record,
            },
        )
        .unwrap();
    assert_eq!(read.identity().unwrap().issuer(), server.origin);
    assert_eq!(read.identity().unwrap().subject(), USER_SUB);
    for operation in [
        UserMemoryWriteOperation::Memory,
        UserMemoryWriteOperation::FencedMemory,
    ] {
        let write = issuer
            .admit_incoming_user_memory_write(
                &proof,
                &verifier,
                &ctx,
                "service",
                ScopeCapture {
                    live: &scope,
                    record: &record,
                },
                operation,
            )
            .unwrap();
        assert_eq!(write.operation().unwrap(), operation);
        write.actor().unwrap();
        assert_eq!(
            write.recheck().unwrap(),
            if operation == UserMemoryWriteOperation::Memory {
                Obligation::Unconditional
            } else {
                Obligation::MemoryFenceRequired
            }
        );
    }
    for _ in 0..3 {
        read.recheck().unwrap();
    }
    assert!(!scope.runtime_root().exists());
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
    let wrong = server.verifier(true, revision);
    assert!(
        issuer
            .admit_incoming_user_memory_read(
                &proof,
                &wrong,
                &ctx,
                "service",
                ScopeCapture {
                    live: &scope,
                    record: &record
                }
            )
            .is_err()
    );
    assert!(
        issuer
            .admit_incoming_user_memory_read(
                &proof,
                &verifier,
                &ctx,
                "unknown",
                ScopeCapture {
                    live: &scope,
                    record: &record
                }
            )
            .is_err()
    );
    let foreign = context(&cancel);
    assert!(
        issuer
            .admit_incoming_user_memory_read(
                &proof,
                &verifier,
                &foreign,
                "service",
                ScopeCapture {
                    live: &scope,
                    record: &record
                }
            )
            .is_err()
    );
    cancel.cancel();
    assert_eq!(read.recheck(), Err(AuthError::Request));
}

#[tokio::test]
async fn incoming_a_never_borrows_active_b_profile_or_foreign_namespace() {
    let a = Server::new().await;
    let b = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = profile_store(&f, &ctx);
    let revision = incoming_provider(&store, &a, &ctx);
    let expiry = now_seconds() + 600;
    let result = store
        .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
            let mut config_b = broker_config(&b.origin, true, false);
            let ProviderKind::Broker(binding_b) = &mut config_b.kind else {
                unreachable!()
            };
            binding_b.mcp_resource = Some(INCOMING_RESOURCE.into());
            doc.add_provider("other".into(), config_b, doc.revision(), 2, ctx)?;
            let mut data = native_profile(&b.origin, &b.token(true, expiry), expiry);
            data.name = "profile-b".into();
            data.provider = "other".into();
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("other").unwrap().revision(),
                profile_revision: None,
            };
            doc.replace_profile_data(data, expected, 3, ctx)?;
            doc.activate("profile-b", doc.revision(), ctx)
        })
        .unwrap();
    assert!(result.failure().is_none());
    let revision_b = store
        .load(&ctx)
        .unwrap()
        .document()
        .provider("other")
        .unwrap()
        .revision();
    let policy = authority(&ALL, &all_permissions(), limits(), 7200)
        .with_incoming_user_permissions("service", &ALL)
        .unwrap()
        .with_incoming_user_permissions("other", &ALL)
        .unwrap();
    let issuer = RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        store,
        policy,
        ttl(),
        &ctx,
    )
    .unwrap();
    let verifier = a.verifier(true, revision);
    let raw = a.token(false, expiry);
    let proof = verifier
        .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let (scope_a, record_a) = user(&f, &ctx, &a.origin, USER_SUB);
    let (scope_b, record_b) = user(&f, &ctx, &b.origin, USER_SUB);
    let read = issuer
        .admit_incoming_user_memory_read(
            &proof,
            &verifier,
            &ctx,
            "service",
            ScopeCapture {
                live: &scope_a,
                record: &record_a,
            },
        )
        .unwrap();
    assert_eq!(read.identity().unwrap().issuer(), a.origin);
    assert!(
        issuer
            .admit_incoming_user_memory_read(
                &proof,
                &verifier,
                &ctx,
                "service",
                ScopeCapture {
                    live: &scope_b,
                    record: &record_b
                }
            )
            .is_err()
    );
    let verifier_b = b.verifier(true, revision_b);
    let raw_b = b.token(false, expiry);
    let proof_b = verifier_b
        .verify(&raw_b, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let write_a = issuer
        .admit_incoming_user_memory_write(
            &proof,
            &verifier,
            &ctx,
            "service",
            ScopeCapture {
                live: &scope_a,
                record: &record_a,
            },
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    let write_b = issuer
        .admit_incoming_user_memory_write(
            &proof_b,
            &verifier_b,
            &ctx,
            "other",
            ScopeCapture {
                live: &scope_b,
                record: &record_b,
            },
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    let actor_a = actor_bytes(&write_a.actor().unwrap());
    let actor_b = actor_bytes(&write_b.actor().unwrap());
    assert_ne!(actor_a, actor_b);
    assert_ne!(scope_a.runtime_root(), scope_b.runtime_root());
    assert_eq!(a.count("/userinfo") + b.count("/userinfo"), 0);
}

#[tokio::test]
async fn native_logout_and_incoming_config_republication_invalidate_admissions() {
    for incoming in [false, true] {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = profile_store(&f, &ctx);
        if incoming {
            let revision = incoming_provider(&store, &server, &ctx);
            let issuer = make_issuer(&f, Arc::clone(&store), &ctx, &ALL, true);
            let verifier = server.verifier(true, revision);
            let raw = server.token(false, now_seconds() + 600);
            let proof = verifier
                .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
                .await
                .unwrap();
            let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
            let read = issuer
                .admit_incoming_user_memory_read(
                    &proof,
                    &verifier,
                    &ctx,
                    "service",
                    ScopeCapture {
                        live: &scope,
                        record: &record,
                    },
                )
                .unwrap();
            let result = store
                .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                    let provider = doc.provider("service").unwrap();
                    let version = provider.revision();
                    let now = provider.updated_at() + 1;
                    let config = provider.config().clone();
                    doc.update_provider("service", config, doc.revision(), version, now, ctx)
                })
                .unwrap();
            assert!(result.failure().is_none());
            assert!(read.recheck().is_err());
            assert!(
                issuer
                    .admit_incoming_user_memory_read(
                        &proof,
                        &verifier,
                        &ctx,
                        "service",
                        ScopeCapture {
                            live: &scope,
                            record: &record
                        }
                    )
                    .is_err()
            );
        } else {
            let expiry = now_seconds() + 600;
            put(
                &store,
                broker_config(&server.origin, true, false),
                native_profile(&server.origin, &server.token(true, expiry), expiry),
                true,
                &ctx,
            );
            let issuer = make_issuer(&f, Arc::clone(&store), &ctx, &ALL, false);
            let revision = issuer
                .local
                .load(&ctx)
                .unwrap()
                .document()
                .provider("service")
                .unwrap()
                .revision();
            let verifier = server.verifier(false, revision);
            let caller = issuer
                .authenticate_runtime(&f.secret(), &ctx)
                .unwrap()
                .authenticate_native(&verifier)
                .await
                .unwrap();
            let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
            let read = issuer
                .admit_authenticated_user_memory_read(
                    &caller,
                    &ctx,
                    ScopeCapture {
                        live: &scope,
                        record: &record,
                    },
                )
                .unwrap();
            let result = store
                .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                    doc.logout(None, doc.revision(), ctx)
                })
                .unwrap();
            assert!(result.failure().is_none());
            assert!(read.recheck().is_err());
        }
    }
}

#[tokio::test]
async fn physical_user_namespace_and_installation_replacement_deny_live_read() {
    for replace_installation in [false, true] {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = profile_store(&f, &ctx);
        let revision = incoming_provider(&store, &server, &ctx);
        let issuer = make_issuer(&f, store, &ctx, &READ, true);
        let verifier = server.verifier(true, revision);
        let raw = server.token(false, now_seconds() + 600);
        let proof = verifier
            .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let (first, _) = user(&f, &ctx, &server.origin, USER_SUB);
        std::fs::create_dir_all(first.runtime_root()).unwrap();
        let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
        let read = issuer
            .admit_incoming_user_memory_read(
                &proof,
                &verifier,
                &ctx,
                "service",
                ScopeCapture {
                    live: &scope,
                    record: &record,
                },
            )
            .unwrap();
        let root = if replace_installation {
            scope.owner_root()
        } else {
            scope.user_root().unwrap()
        };
        std::fs::rename(root, root.with_extension("replaced")).unwrap();
        std::fs::create_dir(root).unwrap();
        assert!(read.recheck().is_err());
        assert!(read.identity().is_err());
    }
}

#[tokio::test]
async fn signed_expiry_and_original_deadline_end_user_admission_without_network() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = profile_store(&f, &ctx);
    let revision = incoming_provider(&store, &server, &ctx);
    let issuer = make_issuer(&f, store, &ctx, &READ, true);
    let verifier = server.verifier(true, revision);
    let expiry = now_seconds() + 2;
    let raw = server.token(false, expiry);
    let proof = verifier
        .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
    let read = issuer
        .admit_incoming_user_memory_read(
            &proof,
            &verifier,
            &ctx,
            "service",
            ScopeCapture {
                live: &scope,
                record: &record,
            },
        )
        .unwrap();
    let keys = server.count("/keys");
    let delay = std::time::UNIX_EPOCH
        .checked_add(Duration::from_secs(expiry))
        .unwrap()
        .duration_since(SystemTime::now())
        .unwrap()
        + Duration::from_millis(10);
    tokio::time::sleep(delay).await;
    assert_eq!(read.recheck(), Err(AuthError::Expired));
    assert_eq!(server.count("/keys"), keys);
    assert_eq!(server.count("/userinfo"), 0);
    let deadline = Instant::now() + Duration::from_millis(500);
    let timed = request_before(&cancel, deadline);
    let timed_fixture = Fixture::new(&timed);
    let timed_store = profile_store(&timed_fixture, &timed);
    let revision = incoming_provider(&timed_store, &server, &timed);
    let timed_issuer = make_issuer(&timed_fixture, timed_store, &timed, &READ, true);
    let timed_verifier = server.verifier(true, revision);
    let raw = server.token(false, now_seconds() + 600);
    let proof = timed_verifier
        .verify(&raw, BrokerRequestContext::new(timed.snapshot()))
        .await
        .unwrap();
    let (scope, record) = user(&timed_fixture, &timed, &server.origin, USER_SUB);
    let read = timed_issuer
        .admit_incoming_user_memory_read(
            &proof,
            &timed_verifier,
            &timed,
            "service",
            ScopeCapture {
                live: &scope,
                record: &record,
            },
        )
        .unwrap();
    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
    assert_eq!(read.recheck(), Err(AuthError::Request));
}

#[test]
fn user_proofs_support_borrowed_async_consumers() {
    fn sync<T: Sync>() {}
    sync::<crate::ServiceCredentialLease<'static>>();
    sync::<crate::VerifiedUserSubject<'static>>();
    sync::<UserReadAdmission<'static>>();
    sync::<UserWriteAdmission<'static>>();
    sync::<UserKnowledgeReadAdmission<'static>>();
    sync::<UserKnowledgeWriteAdmission<'static>>();
}

const KNOWLEDGE_READ: [Permission; 2] = [Permission::ReadUser, Permission::Read(Module::Knowledge)];
const KNOWLEDGE_WRITE: [Permission; 2] = [
    Permission::MutateUser,
    Permission::MutateModule(Module::Knowledge),
];
const KNOWLEDGE_ALL: [Permission; 4] = [
    KNOWLEDGE_READ[0],
    KNOWLEDGE_READ[1],
    KNOWLEDGE_WRITE[0],
    KNOWLEDGE_WRITE[1],
];

#[tokio::test]
async fn knowledge_incoming_requires_both_domain_permissions_and_own_principal() {
    let a = Server::new().await;
    let b = Server::new().await;
    a.userinfo_status.store(503, Ordering::SeqCst);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = profile_store(&f, &ctx);
    let revision = incoming_provider(&store, &a, &ctx);
    let expiry = now_seconds() + 600;
    let result = store
        .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
            let mut config = broker_config(&b.origin, true, false);
            let ProviderKind::Broker(binding) = &mut config.kind else {
                unreachable!()
            };
            binding.mcp_resource = Some(INCOMING_RESOURCE.into());
            doc.add_provider("other".into(), config, doc.revision(), 2, ctx)?;
            let mut data = native_profile(&b.origin, &b.token(true, expiry), expiry);
            data.name = "profile-b".into();
            data.provider = "other".into();
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("other").unwrap().revision(),
                profile_revision: None,
            };
            doc.replace_profile_data(data, expected, 3, ctx)?;
            doc.activate("profile-b", doc.revision(), ctx)
        })
        .unwrap();
    assert!(result.failure().is_none());
    let verifier = a.verifier(true, revision);
    let raw = a.token(false, expiry);
    let proof = verifier
        .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let (scope, record) = user(&f, &ctx, &a.origin, USER_SUB);
    let capture = ScopeCapture {
        live: &scope,
        record: &record,
    };
    for permissions in [
        vec![],
        vec![KNOWLEDGE_READ[0]],
        vec![KNOWLEDGE_READ[1]],
        vec![KNOWLEDGE_WRITE[0]],
        vec![KNOWLEDGE_WRITE[1]],
        READ.to_vec(),
        ALL.to_vec(),
        KNOWLEDGE_READ.to_vec(),
        KNOWLEDGE_WRITE.to_vec(),
        KNOWLEDGE_ALL.to_vec(),
    ] {
        let issuer = make_issuer(&f, Arc::clone(&store), &ctx, &permissions, true);
        let read =
            issuer.admit_incoming_user_knowledge_read(&proof, &verifier, &ctx, "service", capture);
        let write =
            issuer.admit_incoming_user_knowledge_write(&proof, &verifier, &ctx, "service", capture);
        if permissions.contains(&KNOWLEDGE_READ[0]) && permissions.contains(&KNOWLEDGE_READ[1]) {
            let read = read.unwrap();
            assert_eq!(read.identity().unwrap().issuer(), a.origin);
            assert_eq!(read.identity().unwrap().subject(), USER_SUB);
            assert!(std::ptr::eq(read.request().unwrap(), &ctx));
            assert!(std::ptr::eq(read.user().unwrap().live, &scope));
            read.recheck().unwrap();
        } else {
            assert!(matches!(read, Err(AuthError::Denied)));
        }
        if permissions.contains(&KNOWLEDGE_WRITE[0]) && permissions.contains(&KNOWLEDGE_WRITE[1]) {
            let write = write.unwrap();
            assert_eq!(write.recheck().unwrap(), Obligation::Unconditional);
            assert_eq!(write.identity().unwrap().issuer(), a.origin);
            assert!(std::ptr::eq(write.request().unwrap(), &ctx));
            assert!(std::ptr::eq(write.user().unwrap().live, &scope));
        } else {
            assert!(matches!(write, Err(AuthError::Denied)));
        }
    }
    let denied = make_issuer(&f, Arc::clone(&store), &ctx, &KNOWLEDGE_ALL, false);
    assert!(matches!(
        denied.admit_incoming_user_knowledge_read(&proof, &verifier, &ctx, "service", capture),
        Err(AuthError::Denied)
    ));
    let allowed = make_issuer(&f, store, &ctx, &KNOWLEDGE_ALL, true);
    let (other_user, other_record) = user(&f, &ctx, &b.origin, USER_SUB);
    let other = Fixture::new(&ctx);
    let (foreign_user, foreign_record) = user(&other, &ctx, &a.origin, USER_SUB);
    for wrong in [
        ScopeCapture {
            live: &other_user,
            record: &other_record,
        },
        ScopeCapture {
            live: &foreign_user,
            record: &foreign_record,
        },
        f.realm(),
        ScopeCapture {
            live: &f.project,
            record: &f.project_record,
        },
    ] {
        assert!(matches!(
            allowed.admit_incoming_user_knowledge_read(&proof, &verifier, &ctx, "service", wrong),
            Err(AuthError::Denied)
        ));
        assert!(matches!(
            allowed.admit_incoming_user_knowledge_write(&proof, &verifier, &ctx, "service", wrong),
            Err(AuthError::Denied)
        ));
    }
    let wrong_original = context(&cancel);
    let wrong_verifier = a.verifier(true, revision);
    assert!(
        allowed
            .admit_incoming_user_knowledge_read(
                &proof,
                &verifier,
                &wrong_original,
                "service",
                capture
            )
            .is_err()
    );
    assert!(
        allowed
            .admit_incoming_user_knowledge_write(&proof, &wrong_verifier, &ctx, "service", capture)
            .is_err()
    );
    assert!(
        allowed
            .admit_incoming_user_knowledge_read(&proof, &verifier, &ctx, "other", capture)
            .is_err()
    );
    let native_raw = a.token(true, expiry);
    assert!(
        verifier
            .verify(&native_raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .is_err()
    );
    assert!(!scope.runtime_root().exists());
    assert_eq!(a.count("/userinfo") + b.count("/userinfo"), 0);
}

#[tokio::test]
async fn knowledge_incoming_current_provider_and_physical_namespace_are_required() {
    for replacement in [0, 1, 2] {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = profile_store(&f, &ctx);
        let revision = incoming_provider(&store, &server, &ctx);
        let issuer = make_issuer(&f, Arc::clone(&store), &ctx, &KNOWLEDGE_ALL, true);
        let verifier = server.verifier(true, revision);
        let raw = server.token(false, now_seconds() + 600);
        let proof = verifier
            .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let (first, _) = user(&f, &ctx, &server.origin, USER_SUB);
        std::fs::create_dir_all(first.runtime_root()).unwrap();
        let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
        let capture = ScopeCapture {
            live: &scope,
            record: &record,
        };
        let read = issuer
            .admit_incoming_user_knowledge_read(&proof, &verifier, &ctx, "service", capture)
            .unwrap();
        let write = issuer
            .admit_incoming_user_knowledge_write(&proof, &verifier, &ctx, "service", capture)
            .unwrap();
        if replacement == 0 {
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
        } else {
            let root = if replacement == 1 {
                scope.user_root().unwrap()
            } else {
                scope.owner_root()
            };
            std::fs::rename(root, root.with_extension("replaced")).unwrap();
            std::fs::create_dir(root).unwrap();
        }
        assert_eq!(read.recheck(), Err(AuthError::Denied));
        assert_eq!(write.recheck(), Err(AuthError::Denied));
        assert!(read.identity().is_err());
        assert!(read.user().is_err());
        assert!(write.request().is_err());
        assert!(
            issuer
                .admit_incoming_user_knowledge_read(&proof, &verifier, &ctx, "service", capture)
                .is_err()
        );
        assert_eq!(server.count("/userinfo"), 0);
    }
}

#[tokio::test]
async fn knowledge_incoming_expiry_cancel_and_deadline_keep_causal_errors() {
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
        let store = profile_store(&f, &ctx);
        let revision = incoming_provider(&store, &server, &ctx);
        let issuer = make_issuer(&f, store, &ctx, &KNOWLEDGE_ALL, true);
        let verifier = server.verifier(true, revision);
        let expiry = now_seconds() + if ending == 2 { 2 } else { 600 };
        let raw = server.token(false, expiry);
        let proof = verifier
            .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
        let capture = ScopeCapture {
            live: &scope,
            record: &record,
        };
        let read = issuer
            .admit_incoming_user_knowledge_read(&proof, &verifier, &ctx, "service", capture)
            .unwrap();
        let write = issuer
            .admit_incoming_user_knowledge_write(&proof, &verifier, &ctx, "service", capture)
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
        let expected = if ending == 2 {
            AuthError::Expired
        } else {
            AuthError::Request
        };
        assert_eq!(read.recheck(), Err(expected));
        assert_eq!(write.recheck(), Err(expected));
        assert!(read.request().is_err());
        assert!(write.identity().is_err());
        assert_eq!(server.count("/keys"), keys);
        assert_eq!(server.count("/userinfo"), 0);
        assert_eq!(server.count("/token"), 0);
    }
}

#[tokio::test]
async fn knowledge_same_identity_survives_a_new_issuer_epoch_with_fresh_proof() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = profile_store(&f, &ctx);
    let revision = incoming_provider(&store, &server, &ctx);
    let verifier = server.verifier(true, revision);
    let raw = server.token(false, now_seconds() + 600);
    let first = make_issuer(&f, Arc::clone(&store), &ctx, &KNOWLEDGE_ALL, true);
    let proof = verifier
        .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
    let capture = ScopeCapture {
        live: &scope,
        record: &record,
    };
    let read = first
        .admit_incoming_user_knowledge_read(&proof, &verifier, &ctx, "service", capture)
        .unwrap();
    let next_ctx = context(&cancel);
    let next = make_issuer(&f, store, &next_ctx, &KNOWLEDGE_ALL, true);
    assert_ne!(first.epoch, next.epoch);
    let next_proof = verifier
        .verify(&raw, BrokerRequestContext::new(next_ctx.snapshot()))
        .await
        .unwrap();
    let (next_scope, next_record) = user(&f, &next_ctx, &server.origin, USER_SUB);
    let next_read = next
        .admit_incoming_user_knowledge_read(
            &next_proof,
            &verifier,
            &next_ctx,
            "service",
            ScopeCapture {
                live: &next_scope,
                record: &next_record,
            },
        )
        .unwrap();
    assert_eq!(read.identity().unwrap(), next_read.identity().unwrap());
    assert_eq!(scope.runtime_root(), next_scope.runtime_root());
    assert_eq!(server.count("/userinfo"), 0);
}

#[tokio::test]
async fn knowledge_web_requires_both_domain_permissions_and_own_registered_principal() {
    use crate::broker::oauth::web::WebOAuth;
    let server = Server::new().await;
    server.userinfo_status.store(503, Ordering::SeqCst);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = profile_store(&f, &ctx);
    let revision = incoming_provider(&store, &server, &ctx);
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
    let raw = web_access(&server, registered.resource(), now_seconds() + 600);
    let proof = flow
        .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
    let capture = ScopeCapture {
        live: &scope,
        record: &record,
    };
    let keys = server.count("/keys");
    for permissions in [
        vec![],
        vec![KNOWLEDGE_READ[0]],
        vec![KNOWLEDGE_READ[1]],
        vec![KNOWLEDGE_WRITE[0]],
        vec![KNOWLEDGE_WRITE[1]],
        ALL.to_vec(),
        KNOWLEDGE_READ.to_vec(),
        KNOWLEDGE_WRITE.to_vec(),
        KNOWLEDGE_ALL.to_vec(),
    ] {
        let issuer = make_issuer(&f, Arc::clone(&store), &ctx, &permissions, true);
        let read =
            issuer.admit_web_user_knowledge_read(&proof, &verifier, &ctx, "service", capture);
        let write =
            issuer.admit_web_user_knowledge_write(&proof, &verifier, &ctx, "service", capture);
        if KNOWLEDGE_READ.iter().all(|p| permissions.contains(p)) {
            let read = read.unwrap();
            assert_eq!(read.identity().unwrap().issuer(), server.origin);
            assert_eq!(read.identity().unwrap().subject(), USER_SUB);
            assert!(std::ptr::eq(read.request().unwrap(), &ctx));
            assert!(std::ptr::eq(read.user().unwrap().live, &scope));
            read.recheck().unwrap();
        } else {
            assert!(matches!(read, Err(AuthError::Denied)));
        }
        if KNOWLEDGE_WRITE.iter().all(|p| permissions.contains(p)) {
            let write = write.unwrap();
            assert_eq!(write.recheck(), Ok(Obligation::Unconditional));
            assert_eq!(write.identity().unwrap().subject(), USER_SUB);
            assert!(std::ptr::eq(write.request().unwrap(), &ctx));
            assert!(std::ptr::eq(write.user().unwrap().live, &scope));
        } else {
            assert!(matches!(write, Err(AuthError::Denied)));
        }
    }
    let denied = make_issuer(&f, Arc::clone(&store), &ctx, &KNOWLEDGE_ALL, false);
    assert!(matches!(
        denied.admit_web_user_knowledge_read(&proof, &verifier, &ctx, "service", capture),
        Err(AuthError::Denied)
    ));
    assert!(matches!(
        denied.admit_web_user_knowledge_write(&proof, &verifier, &ctx, "service", capture),
        Err(AuthError::Denied)
    ));
    let issuer = make_issuer(&f, store, &ctx, &KNOWLEDGE_ALL, true);
    let (other_user, other_record) = user(&f, &ctx, &server.origin, "another-subject");
    let other = Fixture::new(&ctx);
    let (foreign, foreign_record) = user(&other, &ctx, &server.origin, USER_SUB);
    for wrong in [
        ScopeCapture {
            live: &other_user,
            record: &other_record,
        },
        ScopeCapture {
            live: &foreign,
            record: &foreign_record,
        },
        f.realm(),
        ScopeCapture {
            live: &f.project,
            record: &f.project_record,
        },
    ] {
        assert!(matches!(
            issuer.admit_web_user_knowledge_read(&proof, &verifier, &ctx, "service", wrong),
            Err(AuthError::Denied)
        ));
        assert!(matches!(
            issuer.admit_web_user_knowledge_write(&proof, &verifier, &ctx, "service", wrong),
            Err(AuthError::Denied)
        ));
    }
    let fresh = context(&cancel);
    let other_verifier = server.verifier(false, revision);
    for (request, verifier, provider) in [
        (&fresh, &verifier, "service"),
        (&ctx, &other_verifier, "service"),
        (&ctx, &verifier, "unknown"),
    ] {
        assert!(
            issuer
                .admit_web_user_knowledge_read(&proof, verifier, request, provider, capture)
                .is_err()
        );
        assert!(
            issuer
                .admit_web_user_knowledge_write(&proof, verifier, request, provider, capture)
                .is_err()
        );
    }
    assert!(!scope.runtime_root().exists());
    assert!(!f.source_root.join("records").exists());
    assert_eq!(server.count("/keys"), keys);
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
    assert_eq!(server.count("/register"), 1);
}

#[tokio::test]
async fn knowledge_web_rechecks_current_provider_and_physical_user_binding() {
    use crate::broker::oauth::web::WebOAuth;
    for replacement in [0, 1, 2] {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = profile_store(&f, &ctx);
        let revision = incoming_provider(&store, &server, &ctx);
        let issuer = make_issuer(&f, Arc::clone(&store), &ctx, &KNOWLEDGE_ALL, true);
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
        let raw = web_access(&server, registered.resource(), now_seconds() + 600);
        let proof = flow
            .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let (first, _) = user(&f, &ctx, &server.origin, USER_SUB);
        std::fs::create_dir_all(first.runtime_root()).unwrap();
        let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
        let capture = ScopeCapture {
            live: &scope,
            record: &record,
        };
        let read = issuer
            .admit_web_user_knowledge_read(&proof, &verifier, &ctx, "service", capture)
            .unwrap();
        let write = issuer
            .admit_web_user_knowledge_write(&proof, &verifier, &ctx, "service", capture)
            .unwrap();
        if replacement == 0 {
            let result = store
                .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                    let provider = doc.provider("service").unwrap();
                    let revision = provider.revision();
                    let now = provider.updated_at() + 1;
                    let config = provider.config().clone();
                    doc.update_provider("service", config, doc.revision(), revision, now, ctx)
                })
                .unwrap();
            assert!(result.failure().is_none());
        } else {
            let root = if replacement == 1 {
                scope.user_root().unwrap()
            } else {
                scope.owner_root()
            };
            std::fs::rename(root, root.with_extension("replaced")).unwrap();
            std::fs::create_dir(root).unwrap();
        }
        assert_eq!(read.recheck(), Err(AuthError::Denied));
        assert_eq!(write.recheck(), Err(AuthError::Denied));
        assert!(read.user().is_err());
        assert!(write.identity().is_err());
        assert!(
            issuer
                .admit_web_user_knowledge_write(&proof, &verifier, &ctx, "service", capture)
                .is_err()
        );
        assert_eq!(server.count("/userinfo"), 0);
    }
}

#[tokio::test]
async fn knowledge_web_expiry_cancel_and_deadline_keep_causal_errors_without_network() {
    use crate::broker::oauth::web::WebOAuth;
    for ending in [0, 1, 2] {
        let server = Server::new().await;
        server.userinfo_status.store(503, Ordering::SeqCst);
        let cancel = Cancellation::default();
        let deadline = Instant::now() + Duration::from_millis(500);
        let ctx = if ending == 1 {
            request_before(&cancel, deadline)
        } else {
            context(&cancel)
        };
        let f = Fixture::new(&ctx);
        let store = profile_store(&f, &ctx);
        let revision = incoming_provider(&store, &server, &ctx);
        let issuer = make_issuer(&f, store, &ctx, &KNOWLEDGE_ALL, true);
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
        let raw = web_access(&server, registered.resource(), expiry);
        let proof = flow
            .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
        let capture = ScopeCapture {
            live: &scope,
            record: &record,
        };
        let read = issuer
            .admit_web_user_knowledge_read(&proof, &verifier, &ctx, "service", capture)
            .unwrap();
        let write = issuer
            .admit_web_user_knowledge_write(&proof, &verifier, &ctx, "service", capture)
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
        let expected = if ending == 2 {
            AuthError::Expired
        } else {
            AuthError::Request
        };
        assert_eq!(read.recheck(), Err(expected));
        assert_eq!(write.recheck(), Err(expected));
        assert!(read.request().is_err());
        assert!(write.identity().is_err());
        assert_eq!(server.count("/keys"), keys);
        assert_eq!(server.count("/userinfo"), 0);
        assert_eq!(server.count("/token"), 0);
    }
}
