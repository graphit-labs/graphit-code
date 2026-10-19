mod services;

use super::imported_knowledge::{access, provider, store, web_limits};
use super::service_credentials::{Server, context, now_seconds};
use super::*;
use crate::IssuerAuthorityPolicy;
use crate::broker::{BrokerRequestContext, oauth::web::WebOAuth};

fn issuer(
    f: &Fixture,
    store: Arc<AuthStore>,
    ctx: &RequestContext<'_>,
    policy: IssuerAuthorityPolicy,
) -> RoleIssuer {
    RoleIssuer::new(f.realm(), Arc::clone(&f.runtime), store, policy, ttl(), ctx).unwrap()
}

fn enabled() -> IssuerAuthorityPolicy {
    authority(&[], &[], limits(), 7200)
        .with_web_installed_hub_read("service")
        .unwrap()
}

#[test]
fn web_installed_hub_policy_is_explicit_and_independent() {
    let base = || authority(&[], &[], limits(), 7200);
    assert!(matches!(
        base().with_web_installed_hub_read(""),
        Err(AuthError::Invalid)
    ));
    assert!(matches!(
        enabled().with_web_installed_hub_read("service"),
        Err(AuthError::Invalid)
    ));
    assert!(matches!(
        base().with_web_installed_hub_read(&"x".repeat(limits().max_scope_bytes + 1)),
        Err(AuthError::Limit)
    ));
    let policy = enabled();
    assert!(policy.incoming_user.is_empty());
    assert!(policy.native_installation.is_empty());
    assert!(policy.web_imported_ast.is_empty());
    assert!(policy.web_imported_knowledge.is_empty());
}

#[tokio::test]
async fn web_installed_hub_offline_read_rejects_other_authorities_and_bindings() {
    let server = Server::new().await;
    server.userinfo_status.store(503, Ordering::SeqCst);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let revision = provider(&store, &server, &ctx);
    let allowed = issuer(&f, Arc::clone(&store), &ctx, enabled());
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
    assert!(std::ptr::eq(read.request(), &ctx));
    assert!(std::ptr::eq(read.installation().live, &f.installation));
    assert!(std::ptr::eq(
        read.installation().record,
        &f.installation_record
    ));
    read.recheck().unwrap();
    let base = || authority(&[], &[], limits(), 7200);
    for policy in [
        base(),
        base().with_web_imported_knowledge_read("service").unwrap(),
        base().with_web_imported_ast_read("service").unwrap(),
    ] {
        let denied = issuer(&f, Arc::clone(&store), &ctx, policy);
        assert!(matches!(
            denied.admit_web_installed_hub_read(&proof, &verifier, &ctx, "service", f.realm()),
            Err(AuthError::Denied)
        ));
    }
    let other_ctx = context(&cancel);
    let other_verifier = server.verifier(false, revision);
    let other = Fixture::new(&ctx);
    assert!(
        allowed
            .admit_web_installed_hub_read(&proof, &verifier, &other_ctx, "service", f.realm())
            .is_err()
    );
    assert!(
        allowed
            .admit_web_installed_hub_read(&proof, &other_verifier, &ctx, "service", f.realm())
            .is_err()
    );
    assert!(matches!(
        allowed.admit_web_installed_hub_read(&proof, &verifier, &ctx, "unknown", f.realm()),
        Err(AuthError::Denied)
    ));
    for capture in [
        other.realm(),
        ScopeCapture {
            live: &f.project,
            record: &f.project_record,
        },
    ] {
        assert!(matches!(
            allowed.admit_web_installed_hub_read(&proof, &verifier, &ctx, "service", capture),
            Err(AuthError::Denied)
        ));
    }
    assert_eq!(server.count("/keys"), keys);
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
    assert!(store.load(&ctx).unwrap().document().profiles().is_empty());
}

#[tokio::test]
async fn web_installed_hub_rechecks_configuration_installation_and_request_end() {
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
        let allowed = issuer(&f, Arc::clone(&store), &ctx, enabled());
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
        let keys = server.count("/keys");
        match ending {
            0 => {
                let result = store
                    .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                        let p = doc.provider("service").unwrap();
                        let config = p.config().clone();
                        let revision = p.revision();
                        let now = p.updated_at() + 1;
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
        assert_eq!(
            read.recheck(),
            Err(match ending {
                0 | 1 => AuthError::Denied,
                4 => AuthError::Expired,
                _ => AuthError::Request,
            })
        );
        assert!(
            allowed
                .admit_web_installed_hub_read(&proof, &verifier, &ctx, "service", f.realm())
                .is_err()
        );
        assert_eq!(server.count("/keys"), keys);
        assert_eq!(server.count("/userinfo"), 0);
        assert_eq!(server.count("/token"), 0);
    }
}
