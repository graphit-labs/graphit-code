use super::service_credentials::{self as service, Server, USER_SUB, context, now_seconds};
use super::*;
use crate::{
    CredentialSource, ServiceOrigin, ServicePurpose, VerifiedUserOrigin,
    broker::BrokerRequestContext,
};

#[tokio::test]
async fn native_literal_subject_reuses_real_proof_without_userinfo_or_permission() {
    let server = Server::new().await;
    let raw = server.token(true, now_seconds() + 3600);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = service::issuer(&f, f.auth_store(&ctx), &ctx, &[]);
    let verifier = server.verifier(false, 1);
    let proof = verifier
        .verify_refresh(
            &raw,
            None,
            USER_SUB,
            BrokerRequestContext::new(ctx.snapshot()),
        )
        .await
        .unwrap();
    let subject = issuer.user_from_native(&proof, &verifier, &ctx).unwrap();
    assert_eq!(subject.issuer().unwrap(), server.origin);
    assert_eq!(subject.subject().unwrap(), USER_SUB);
    assert_eq!(subject.username().unwrap(), Some("display-not-sub"));
    assert!(subject.teams().unwrap().is_empty());
    assert_eq!(subject.origin().unwrap(), VerifiedUserOrigin::Native);
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/service"), 0);
    // An identity wrapper can exist under a deny-all service policy. It is not
    // a ReadUser/MutateUser, Role, storage or RemoteRequest proof.
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(
        issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .is_err()
    );
}

#[tokio::test]
async fn same_sub_under_two_literal_issuers_remains_two_distinct_identities() {
    let a = Server::new().await;
    let b = Server::new().await;
    let raw_a = a.token(true, now_seconds() + 3600);
    let raw_b = b.token(true, now_seconds() + 3600);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = service::issuer(&f, f.auth_store(&ctx), &ctx, &[]);
    let verifier_a = a.verifier(false, 1);
    let verifier_b = b.verifier(false, 1);
    let proof_a = verifier_a
        .verify_refresh(
            &raw_a,
            None,
            USER_SUB,
            BrokerRequestContext::new(ctx.snapshot()),
        )
        .await
        .unwrap();
    let proof_b = verifier_b
        .verify_refresh(
            &raw_b,
            None,
            USER_SUB,
            BrokerRequestContext::new(ctx.snapshot()),
        )
        .await
        .unwrap();
    let subject_a = issuer
        .user_from_native(&proof_a, &verifier_a, &ctx)
        .unwrap();
    let subject_b = issuer
        .user_from_native(&proof_b, &verifier_b, &ctx)
        .unwrap();
    assert_eq!(subject_a.subject().unwrap(), subject_b.subject().unwrap());
    assert_ne!(subject_a.issuer().unwrap(), subject_b.issuer().unwrap());
    assert!(
        issuer
            .user_from_native(&proof_a, &verifier_b, &ctx)
            .is_err()
    );
    assert_eq!(a.count("/userinfo") + b.count("/userinfo"), 0);
}

#[tokio::test]
async fn identity_requires_same_verifier_and_original_snapshot_after_admission() {
    let server = Server::new().await;
    let raw = server.token(false, now_seconds() + 3600);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = service::issuer(&f, f.auth_store(&ctx), &ctx, &[]);
    let verifier = server.verifier(true, 1);
    let proof = verifier
        .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let subject = issuer.user_from_incoming(&proof, &verifier, &ctx).unwrap();
    assert_eq!(subject.subject().unwrap(), USER_SUB);
    assert_eq!(subject.origin().unwrap(), VerifiedUserOrigin::Incoming);
    let fresh = context(&cancel);
    assert!(
        issuer
            .user_from_incoming(&proof, &verifier, &fresh)
            .is_err()
    );
    let other_verifier = server.verifier(true, 1);
    assert!(
        issuer
            .user_from_incoming(&proof, &other_verifier, &ctx)
            .is_err()
    );
    cancel.cancel();
    assert!(subject.check().is_err());
    assert!(subject.subject().is_err());
    assert!(subject.issuer().is_err());
    assert!(subject.origin().is_err());
    assert!(subject.username().is_err());
    assert!(subject.teams().is_err());
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/service"), 0);
}

#[tokio::test]
async fn service_subject_rechecks_policy_and_physical_observation_while_identity_only_does_not_grant()
 {
    for physical in [false, true] {
        let server = Server::new().await;
        let raw = server.token(false, now_seconds() + 3600);
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = service::native_store(&f, &ctx);
        service::put(
            &store,
            service::broker_config(&server.origin, true, false),
            service::native_profile(&server.origin, "daemon-b", now_seconds() + 3600),
            false,
            &ctx,
        );
        let revision = store
            .load(&ctx)
            .unwrap()
            .document()
            .provider("service")
            .unwrap()
            .revision();
        let verifier = server.verifier(true, revision);
        let proof = verifier
            .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let issuer = service::issuer(
            &f,
            Arc::clone(&store),
            &ctx,
            &[service::broker_rule(
                &server.origin,
                ServiceOrigin::BrokerIncoming,
                CredentialSource::IncomingBearer,
            )],
        );
        let identity = issuer.user_from_incoming(&proof, &verifier, &ctx).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let mut selected = issuer
            .capture_incoming_service(&proof, &verifier, &ctx, ServicePurpose::Embedding)
            .unwrap();
        let lease = selected.credential().unwrap();
        let subject = lease.user_subject().unwrap().unwrap();
        assert_eq!(subject.subject().unwrap(), USER_SUB);
        if physical {
            let mutation = context(&cancel);
            service::put(
                &store,
                service::broker_config(&server.origin, true, false),
                service::native_profile(&server.origin, "changed-daemon-b", now_seconds() + 3600),
                false,
                &mutation,
            );
        } else {
            issuer.revoke_service_policy(&parent).unwrap();
        }
        assert!(subject.check().is_err());
        assert!(subject.issuer().is_err());
        assert!(subject.subject().is_err());
        assert!(subject.origin().is_err());
        // The standalone identity still denotes the original authenticated peer,
        // but cannot bypass the revoked service lease or claim USER storage.
        assert_eq!(identity.subject().unwrap(), USER_SUB);
        assert!(lease.credential().is_err());
        assert_eq!(server.count("/service"), 0);
    }
}

#[tokio::test]
async fn expired_and_wrong_audience_signatures_never_create_subjects() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let verifier = server.verifier(true, 1);
    let expired = server.token(false, now_seconds().saturating_sub(60));
    assert!(
        verifier
            .verify(&expired, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .is_err()
    );
    let native = server.token(true, now_seconds() + 3600);
    assert!(
        verifier
            .verify(&native, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .is_err()
    );
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/service"), 0);
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
fn web_token(server: &Server, expiry: u64, audience: &str) -> String {
    server.sign(
        &serde_json::json!({"iss":server.origin,"sub":USER_SUB,"aud":["broker-api",audience],
        "client_id":"web-client","jti":"actual-web-token","token_use":"access",
        "iat":now_seconds().saturating_sub(1),"exp":expiry,"preferred_username":"display-not-sub"}),
    )
}
#[tokio::test]
async fn web_subject_retains_verified_identity_without_permission_or_repeated_userinfo() {
    use crate::broker::oauth::web::WebOAuth;
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = service::issuer(&f, f.auth_store(&ctx), &ctx, &[]);
    let verifier = server.verifier(false, 1);
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
    let raw = web_token(&server, now_seconds() + 3600, registered.resource());
    let proof = flow
        .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let subject = issuer.user_from_web(&proof, &verifier, &ctx).unwrap();
    assert_eq!(subject.issuer().unwrap(), server.origin);
    assert_eq!(subject.subject().unwrap(), USER_SUB);
    assert_eq!(subject.origin().unwrap(), VerifiedUserOrigin::Web);
    subject.check().unwrap();
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/register"), 1);
    let fresh = context(&cancel);
    assert!(issuer.user_from_web(&proof, &verifier, &fresh).is_err());
    for other in [server.verifier(false, 1), server.verifier(false, 2)] {
        assert!(issuer.user_from_web(&proof, &other, &ctx).is_err());
    }
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(
        issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .is_err()
    );
    assert_eq!(server.count("/service"), 0);
    server.userinfo_status.store(401, Ordering::SeqCst);
    assert!(
        flow.verify_access(
            &registered,
            &raw,
            BrokerRequestContext::new(fresh.snapshot())
        )
        .await
        .is_ok()
    );
    // Remote profile status does not revoke a locally valid JWT.
    assert_eq!(subject.subject().unwrap(), USER_SUB);
    assert_eq!(server.count("/userinfo"), 0);
}
#[tokio::test]
async fn web_subject_preserves_literal_issuers_and_rejects_invalid_access_proofs() {
    use crate::broker::oauth::web::WebOAuth;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = service::issuer(&f, f.auth_store(&ctx), &ctx, &[]);
    let mut identities = Vec::new();
    for server in [Server::new().await, Server::new().await] {
        let verifier = server.verifier(false, 1);
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
        for raw in [
            web_token(
                &server,
                now_seconds().saturating_sub(60),
                registered.resource(),
            ),
            web_token(
                &server,
                now_seconds() + 3600,
                "https://workspace.example/mcp",
            ),
        ] {
            assert!(
                flow.verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
                    .await
                    .is_err()
            );
        }
        assert_eq!(server.count("/userinfo"), 0);
        let raw = web_token(&server, now_seconds() + 3600, registered.resource());
        let proof = flow
            .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let subject = issuer.user_from_web(&proof, &verifier, &ctx).unwrap();
        identities.push((
            subject.issuer().unwrap().to_owned(),
            subject.subject().unwrap().to_owned(),
        ));
    }
    assert_ne!(identities[0].0, identities[1].0);
    assert_eq!(identities[0].1, identities[1].1);
}
#[tokio::test]
async fn web_subject_preserves_cancel_and_deadline_causes_on_all_getters() {
    use crate::{ServiceUseError, broker::oauth::web::WebOAuth, profile::ProfileError};
    for expire in [false, true] {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let setup = context(&cancel);
        let f = Fixture::new(&setup);
        let issuer = service::issuer(&f, f.auth_store(&setup), &setup, &[]);
        let deadline = Instant::now() + Duration::from_secs(2);
        let ctx = request_before(&cancel, deadline);
        let verifier = server.verifier(false, 1);
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
        let raw = web_token(&server, now_seconds() + 3600, registered.resource());
        let proof = flow
            .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let subject = issuer.user_from_web(&proof, &verifier, &ctx).unwrap();
        if expire {
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
        } else {
            cancel.cancel();
        }
        let expected = if expire {
            ProfileError::Deadline
        } else {
            ProfileError::Cancelled
        };
        for error in [
            subject.check().unwrap_err(),
            subject.issuer().unwrap_err(),
            subject.subject().unwrap_err(),
            subject.origin().unwrap_err(),
        ] {
            assert!(matches!(error, ServiceUseError::Profile(actual) if actual == expected));
        }
        assert_eq!(server.count("/userinfo"), 0);
    }
}
