use super::service_credentials::{
    Server, broker_config, context, native_profile, now_seconds, put,
};
use super::*;
use crate::{
    ExternalEffect, ServiceUseError,
    broker::BrokerError,
    profile::{
        LocalCredentials, ProfileCredentials, ProfileData, ProfileError, ProfileLimits, Secret,
    },
};
use std::time::UNIX_EPOCH;

const INSTALL: [Permission; 3] = [
    Permission::Configure,
    Permission::ManageCache,
    Permission::External(ExternalEffect::PublishedFilesystem),
];
const READ: Permission = Permission::Read(graphit_core::module_settings::Module::Ast);

fn store(f: &Fixture, ctx: &RequestContext<'_>) -> Arc<AuthStore> {
    f.auth_store_policy(
        ctx,
        ProfileLimits {
            bytes: 4096,
            nodes: 4096,
            scalar_bytes: 2048,
            collection_entries: 64,
            providers: 16,
            profiles: 16,
        },
    )
}
fn issuer(
    f: &Fixture,
    store: Arc<AuthStore>,
    ctx: &RequestContext<'_>,
    native: &[Permission],
) -> RoleIssuer {
    RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        store,
        authority(&[READ], &all_permissions(), limits(), 7200)
            .with_native_installation_permissions(native)
            .unwrap(),
        ttl(),
        ctx,
    )
    .unwrap()
}
fn verifier(
    server: &Server,
    issuer: &RoleIssuer,
    ctx: &RequestContext<'_>,
) -> crate::broker::BrokerVerifier {
    let revision = issuer
        .local
        .load(ctx)
        .unwrap()
        .document()
        .provider("service")
        .unwrap()
        .revision();
    server.verifier(false, revision)
}
fn populate(store: &AuthStore, server: &Server, ctx: &RequestContext<'_>, raw: &str, expiry: u64) {
    put(
        store,
        broker_config(&server.origin, true, false),
        native_profile(&server.origin, raw, expiry),
        true,
        ctx,
    );
}

#[cfg(windows)]
pub(super) fn assert_retained_scope_refuses_rename(
    scope: &CapturedPublicationScope,
    destination: &std::path::Path,
    request: &RequestContext<'_>,
) {
    let context = ScopeContext::from_snapshot(request.snapshot());
    scope.revalidate_bounded(&context).unwrap();
    let before = scope.binding().unwrap();
    let error = std::fs::rename(scope.owner_root(), destination).unwrap_err();
    assert_eq!(error.raw_os_error(), Some(32), "expected sharing violation");
    assert!(std::fs::metadata(scope.owner_root()).unwrap().is_dir());
    assert_eq!(
        std::fs::symlink_metadata(destination).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    scope.revalidate_bounded(&context).unwrap();
    assert!(scope.binding().unwrap() == before);
}

#[tokio::test]
async fn native_installation_keeps_same_runtime_and_uses_no_network_at_barriers() {
    let server = Server::new().await;
    server.userinfo_status.store(503, Ordering::SeqCst);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let expiry = now_seconds() + 600;
    populate(&store, &server, &ctx, &server.token(true, expiry), expiry);
    let issuer = issuer(&f, store, &ctx, &INSTALL);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(
        caller
            .authorize_native_installation(Permission::Configure)
            .is_err()
    );
    assert_eq!(caller.authorize(READ), Ok(Obligation::Unconditional));
    let verifier = verifier(&server, &issuer, &ctx);
    let mut yielded = false;
    let caller = {
        let future = caller.authenticate_native(&verifier);
        tokio::pin!(future);
        std::future::poll_fn(|cx| {
            let result = std::future::Future::poll(future.as_mut(), cx);
            assert!(
                !graphit_core::file_lock::FileLock::is_locked(
                    f.installation.owner_root().join(".auth.json.lock")
                )
                .unwrap()
            );
            yielded |= result.is_pending();
            result
        })
        .await
        .unwrap()
    };
    assert!(yielded);
    let discovery = server.count("/.well-known/graphit-broker");
    let oidc = server.count("/.well-known/openid-configuration");
    let keys = server.count("/keys");
    assert_eq!((discovery, oidc, keys), (1, 1, 1));
    for permission in INSTALL {
        assert!(matches!(
            caller.authorize_native_installation(permission),
            Ok(Obligation::Unconditional)
        ));
        assert_eq!(caller.authorize(permission), Err(AuthError::Denied));
    }
    assert!(caller.authorize_native_installation(READ).is_err());
    assert!(
        caller
            .authorize_native_installation(Permission::ManageAuthProfiles)
            .is_err()
    );
    assert_eq!(caller.authorize(READ), Ok(Obligation::Unconditional));
    assert!(std::ptr::eq(caller.issuer, &issuer));
    assert!(std::ptr::eq(caller.request, &ctx));
    assert_eq!(
        (
            server.count("/.well-known/graphit-broker"),
            server.count("/.well-known/openid-configuration"),
            server.count("/keys")
        ),
        (discovery, oidc, keys)
    );
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
    // Unix can remove the unrelated checkout; Windows retains its captured
    // directory handle. Neither outcome changes installation-only authority.
    #[cfg(unix)]
    std::fs::remove_dir(&f.source_root).unwrap();
    #[cfg(windows)]
    {
        let before = f.project.binding().unwrap();
        let error = std::fs::remove_dir(&f.source_root).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(32), "expected sharing violation");
        assert!(std::fs::metadata(&f.source_root).unwrap().is_dir());
        f.project
            .revalidate_bounded(&ScopeContext::from_snapshot(ctx.snapshot()))
            .unwrap();
        assert!(f.project.binding().unwrap() == before);
    }
    assert!(
        caller
            .authorize_native_installation(Permission::ManageCache)
            .is_ok()
    );
}

#[tokio::test]
async fn native_installation_requires_explicit_policy_and_rejects_extra_permissions() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let expiry = now_seconds() + 600;
    populate(&store, &server, &ctx, &server.token(true, expiry), expiry);
    let issuer = issuer(&f, Arc::clone(&store), &ctx, &[]);
    let verifier = verifier(&server, &issuer, &ctx);
    let caller = issuer
        .authenticate_runtime(&f.secret(), &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .unwrap();
    assert!(
        caller
            .authorize_native_installation(Permission::Configure)
            .is_err()
    );
    let subset = super::native_installation::issuer(&f, store, &ctx, &[Permission::Configure]);
    let caller = subset
        .authenticate_runtime(&f.secret(), &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .unwrap();
    assert!(
        caller
            .authorize_native_installation(Permission::Configure)
            .is_ok()
    );
    assert!(
        caller
            .authorize_native_installation(Permission::ManageCache)
            .is_err()
    );
    let base = || authority(&[READ], &[], limits(), 7200);
    assert!(matches!(
        base().with_native_installation_permissions(&[READ]),
        Err(AuthError::Invalid)
    ));
    assert!(matches!(
        base()
            .with_native_installation_permissions(&[Permission::Configure, Permission::Configure]),
        Err(AuthError::Invalid)
    ));
}

#[tokio::test]
async fn native_installation_refuses_local_anonymous_and_missing_active_profile() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let issuer = issuer(&f, Arc::clone(&store), &ctx, &INSTALL);
    let verifier = server.verifier(false, 1);
    let no_active = issuer
        .authenticate_runtime(&f.secret(), &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        no_active,
        ServiceUseError::Profile(ProfileError::NoActive)
    ));
    let local_data = |anonymous| ProfileData {
        name: "local".into(),
        provider: "service".into(),
        username: if anonymous {
            "anonymous".into()
        } else {
            "local-user".into()
        },
        organization: None,
        teams: vec![],
        credentials: ProfileCredentials::Local(LocalCredentials {
            anonymous,
            mcp_key: Some(Secret::new(SecretString::from("local-mcp-key"))),
            broker_key: None,
            embedding_api_key: None,
            rerank_api_key: None,
            s3: None,
        }),
    };
    put(
        &store,
        broker_config(&server.origin, false, true),
        local_data(false),
        true,
        &ctx,
    );
    let local = issuer.authenticate_local("local-mcp-key", &ctx).unwrap();
    assert!(matches!(
        local.authenticate_native(&verifier).await,
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));
    let runtime = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(matches!(
        runtime.authenticate_native(&verifier).await,
        Err(ServiceUseError::WrongMode)
    ));
    put(
        &store,
        broker_config(&server.origin, false, true),
        local_data(true),
        true,
        &ctx,
    );
    let anonymous = issuer.authenticate_local("local-mcp-key", &ctx).unwrap();
    assert!(matches!(
        anonymous.authenticate_native(&verifier).await,
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));
    assert_eq!(server.count("/keys"), 0);
}

#[tokio::test]
async fn native_installation_binds_provider_revision_resource_and_literal_issuer() {
    let server = Server::new().await;
    let other = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let expiry = now_seconds() + 600;
    populate(&store, &server, &ctx, &server.token(true, expiry), expiry);
    let issuer = issuer(&f, store, &ctx, &INSTALL);
    let revision = issuer
        .local
        .load(&ctx)
        .unwrap()
        .document()
        .provider("service")
        .unwrap()
        .revision();
    for verifier in [
        server.verifier(false, revision + 1),
        server.verifier(true, revision),
        other.verifier(false, revision),
    ] {
        let error = issuer
            .authenticate_runtime(&f.secret(), &ctx)
            .unwrap()
            .authenticate_native(&verifier)
            .await
            .err()
            .unwrap();
        assert!(matches!(error, ServiceUseError::Auth(AuthError::Denied)));
    }
    assert_eq!(server.count("/keys"), 0);
    assert_eq!(other.count("/keys"), 0);
}

#[tokio::test]
async fn native_installation_preserves_jwt_subject_purpose_and_signature_failures() {
    let server = Server::new().await;
    let other = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let issuer = issuer(&f, Arc::clone(&store), &ctx, &INSTALL);
    let now = now_seconds();
    let wrong_subject = server.sign(&serde_json::json!({
        "iss":server.origin,"sub":"different-real-sub","aud":["broker-api"],
        "client_id":"graphit-cli","jti":"actual","token_use":"access","iat":now-1,"exp":now+600
    }));
    for raw in [
        wrong_subject,
        server.token(false, now + 600),
        other.token(true, now + 600),
    ] {
        populate(&store, &server, &ctx, &raw, now + 600);
        let verifier = verifier(&server, &issuer, &ctx);
        let error = issuer
            .authenticate_runtime(&f.secret(), &ctx)
            .unwrap()
            .authenticate_native(&verifier)
            .await
            .err()
            .unwrap();
        assert!(matches!(
            error,
            ServiceUseError::Broker(BrokerError::InvalidToken)
        ));
    }
    assert_eq!(server.count("/userinfo"), 0);
}

#[tokio::test]
async fn native_installation_revokes_phase_after_logout_or_profile_publication() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let expiry = now_seconds() + 600;
    populate(&store, &server, &ctx, &server.token(true, expiry), expiry);
    let issuer = issuer(&f, Arc::clone(&store), &ctx, &INSTALL);
    let verifier = verifier(&server, &issuer, &ctx);
    let caller = issuer
        .authenticate_runtime(&f.secret(), &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .unwrap();
    let publication = store
        .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
            doc.logout(None, doc.revision(), ctx)
        })
        .unwrap();
    assert_eq!(
        publication.effect(),
        graphit_core::auth_document::publication::AuthPublicationEffect::Applied
    );
    assert!(matches!(
        caller.authorize_native_installation(Permission::Configure),
        Err(ServiceUseError::Store(_))
    ));
    assert_eq!(caller.authorize(READ), Ok(Obligation::Unconditional));
    // A replacement with equal identity remains a new physical observation.
    populate(&store, &server, &ctx, &server.token(true, expiry), expiry);
    assert!(
        caller
            .authorize_native_installation(Permission::Configure)
            .is_err()
    );
    let mut second = native_profile(&server.origin, &server.token(true, expiry), expiry);
    second.name = "second-native".into();
    put(
        &store,
        broker_config(&server.origin, true, false),
        second,
        false,
        &ctx,
    );
    let retained = issuer
        .authenticate_runtime(&f.secret(), &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .unwrap();
    let switched = store
        .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
            doc.activate("second-native", doc.revision(), ctx)
        })
        .unwrap();
    assert_eq!(
        switched.effect(),
        graphit_core::auth_document::publication::AuthPublicationEffect::Applied
    );
    assert_eq!(switched.failure(), None);
    assert_eq!(switched.cleanup_failure(), None);
    assert!(matches!(
        retained.authorize_native_installation(Permission::Configure),
        Err(ServiceUseError::Store(_))
    ));
}

#[tokio::test]
async fn native_installation_uses_original_and_retains_cancelled_deadline_causes() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let expiry = now_seconds() + 600;
    populate(&store, &server, &ctx, &server.token(true, expiry), expiry);
    let issuer = issuer(&f, store, &ctx, &INSTALL);
    let verifier = verifier(&server, &issuer, &ctx);
    let mut caller = issuer
        .authenticate_runtime(&f.secret(), &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .unwrap();
    let foreign = context(&cancel);
    caller.request = &foreign;
    assert!(
        caller
            .authorize_native_installation(Permission::Configure)
            .is_err()
    );
    caller.request = &ctx;
    cancel.cancel();
    assert!(matches!(
        caller.authorize_native_installation(Permission::Configure),
        Err(ServiceUseError::Profile(ProfileError::Cancelled))
    ));
    let deadline_cancel = Cancellation::default();
    let deadline = Instant::now() + Duration::from_millis(10);
    let expired = request_before(&deadline_cancel, deadline);
    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
    caller.request = &expired;
    assert!(matches!(
        caller.authorize_native_installation(Permission::Configure),
        Err(ServiceUseError::Profile(ProfileError::Deadline))
    ));
}

#[tokio::test]
async fn native_installation_rejects_signed_expiry_and_physical_key_replacement() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let expired = now_seconds().saturating_sub(1);
    populate(&store, &server, &ctx, &server.token(true, expired), expired);
    let issuer = issuer(&f, Arc::clone(&store), &ctx, &INSTALL);
    let verifier = verifier(&server, &issuer, &ctx);
    assert!(matches!(
        issuer
            .authenticate_runtime(&f.secret(), &ctx)
            .unwrap()
            .authenticate_native(&verifier)
            .await,
        Err(ServiceUseError::Broker(BrokerError::InvalidToken))
    ));
    let expiry = now_seconds() + 2;
    populate(&store, &server, &ctx, &server.token(true, expiry), expiry);
    let caller = issuer
        .authenticate_runtime(&f.secret(), &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .unwrap();
    tokio::time::sleep_until(tokio::time::Instant::from_std(
        Instant::now()
            + UNIX_EPOCH
                .checked_add(Duration::from_secs(expiry))
                .unwrap()
                .duration_since(SystemTime::now())
                .unwrap()
            + Duration::from_millis(10),
    ))
    .await;
    assert!(matches!(
        caller.authorize_native_installation(Permission::Configure),
        Err(ServiceUseError::Broker(BrokerError::InvalidToken))
    ));
    let expiry = now_seconds() + 600;
    populate(&store, &server, &ctx, &server.token(true, expiry), expiry);
    let caller = issuer
        .authenticate_runtime(&f.secret(), &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .unwrap();
    std::fs::write(
        f.installation.owner_root().join("daemon/mcp.key"),
        b"replacement-key",
    )
    .unwrap();
    assert!(matches!(
        caller.authorize_native_installation(Permission::Configure),
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));
    let replacement = Fixture::new(&ctx);
    let replacement_store = super::native_installation::store(&replacement, &ctx);
    populate(
        &replacement_store,
        &server,
        &ctx,
        &server.token(true, expiry),
        expiry,
    );
    let replacement_issuer =
        super::native_installation::issuer(&replacement, replacement_store, &ctx, &INSTALL);
    let replacement_verifier =
        super::native_installation::verifier(&server, &replacement_issuer, &ctx);
    let retained = replacement_issuer
        .authenticate_runtime(&replacement.secret(), &ctx)
        .unwrap()
        .authenticate_native(&replacement_verifier)
        .await
        .unwrap();
    let root = replacement.installation.owner_root();
    #[cfg(unix)]
    {
        std::fs::rename(root, root.with_extension("retired-installation")).unwrap();
        std::fs::create_dir(root).unwrap();
        assert!(
            retained
                .authorize_native_installation(Permission::Configure)
                .is_err()
        );
    }
    #[cfg(windows)]
    {
        assert!(matches!(
            retained.authorize_native_installation(Permission::Configure),
            Ok(Obligation::Unconditional)
        ));
        let document = std::fs::read(root.join("auth.json")).unwrap();
        assert_retained_scope_refuses_rename(
            &replacement.installation,
            &root.with_extension("retired-installation"),
            &ctx,
        );
        assert!(std::fs::read(root.join("auth.json")).unwrap() == document);
        assert!(matches!(
            retained.authorize_native_installation(Permission::Configure),
            Ok(Obligation::Unconditional)
        ));
    }
}

#[tokio::test]
async fn native_installation_preserves_transport_and_document_decode_causes() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let expiry = now_seconds() + 600;
    populate(&store, &server, &ctx, &server.token(true, expiry), expiry);
    let issuer = issuer(&f, Arc::clone(&store), &ctx, &INSTALL);
    let verifier = verifier(&server, &issuer, &ctx);
    drop(server);
    let error = issuer
        .authenticate_runtime(&f.secret(), &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        error,
        ServiceUseError::Broker(BrokerError::Connection)
    ));
    std::fs::write(
        f.installation.owner_root().join("auth.json"),
        b"{malformed-json-negative-fixture",
    )
    .unwrap();
    let error = issuer
        .authenticate_runtime(&f.secret(), &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        error,
        ServiceUseError::Store(crate::store::AuthStoreError::Profile(_))
    ));
}

#[tokio::test]
async fn native_provider_runtime_recipe_is_readonly_and_builds_actual_verifier() {
    use crate::broker::{BrokerPolicy, BrokerVerifier, JsonLimits, VerifierLimits};
    use graphit_http::client::{ClientPolicy, Limits as NetworkLimits};
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let expiry = now_seconds() + 600;
    populate(&store, &server, &ctx, &server.token(true, expiry), expiry);
    let issuer = RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        store,
        authority(&[READ], &[], limits(), 7200)
            .with_native_installation_permissions(&INSTALL)
            .unwrap(),
        ttl(),
        &ctx,
    )
    .unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert_eq!(
        caller.authorize(Permission::ManageAuthProfiles),
        Err(AuthError::Denied)
    );
    let (service, revision) = caller.native_provider().unwrap();
    assert_eq!(service.endpoint, server.origin);
    assert_eq!(service.mcp_resource, None);
    assert!(!service.allow_anonymous);
    assert_eq!(
        revision,
        issuer
            .local
            .load(&ctx)
            .unwrap()
            .document()
            .provider("service")
            .unwrap()
            .revision()
    );
    assert!(
        caller
            .authorize_native_installation(Permission::Configure)
            .is_err()
    );
    for path in [
        "/.well-known/graphit-broker",
        "/.well-known/openid-configuration",
        "/keys",
        "/userinfo",
        "/token",
    ] {
        assert_eq!(server.count(path), 0);
    }
    let verifier = BrokerVerifier::new(
        BrokerPolicy {
            endpoint: &service.endpoint,
            mcp_resource: service.mcp_resource.as_deref(),
            provider_revision: revision,
            network: ClientPolicy {
                origin: &service.endpoint,
                allow_loopback_http: true,
                proxy: None,
                additional_roots: &[],
            },
        },
        NetworkLimits {
            request_body_bytes: 8192,
            response_body_bytes: 8192,
            url_bytes: 2048,
            header_bytes: 16384,
            header_count: 32,
            max_in_flight: 8,
            max_idle_per_host: 2,
            idle_timeout: Duration::from_secs(30),
            max_certificates: 2,
            max_certificate_bytes: 8192,
        },
        VerifierLimits {
            token_bytes: 8192,
            json: JsonLimits {
                bytes: 8192,
                nodes: 256,
                depth: 16,
                scalar_bytes: 2048,
                collection_entries: 32,
            },
            max_keys: 8,
            metadata_ttl: Duration::from_secs(300),
            key_ttl: Duration::from_secs(300),
            unknown_key_cooldown: Duration::from_millis(100),
        },
    )
    .unwrap();
    let caller = caller.authenticate_native(&verifier).await.unwrap();
    assert!(
        caller
            .authorize_native_installation(Permission::Configure)
            .is_ok()
    );
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
}

#[tokio::test]
async fn native_provider_refuses_local_missing_and_mismatched_profiles() {
    let server = Server::new().await;
    let other = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let issuer = issuer(&f, Arc::clone(&store), &ctx, &[]);
    let runtime = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(matches!(
        runtime.native_provider(),
        Err(ServiceUseError::Profile(ProfileError::NoActive))
    ));
    let local_data = |anonymous| ProfileData {
        name: "local".into(),
        provider: "service".into(),
        username: if anonymous {
            "anonymous".into()
        } else {
            "local-user".into()
        },
        organization: None,
        teams: vec![],
        credentials: ProfileCredentials::Local(LocalCredentials {
            anonymous,
            mcp_key: Some(Secret::new(SecretString::from("local-mcp-key"))),
            broker_key: None,
            embedding_api_key: None,
            rerank_api_key: None,
            s3: None,
        }),
    };
    for anonymous in [false, true] {
        put(
            &store,
            broker_config(&server.origin, false, true),
            local_data(anonymous),
            true,
            &ctx,
        );
        let local = issuer.authenticate_local("local-mcp-key", &ctx).unwrap();
        assert!(matches!(
            local.native_provider(),
            Err(ServiceUseError::Auth(AuthError::Denied))
        ));
        assert!(matches!(
            runtime.native_provider(),
            Err(ServiceUseError::WrongMode)
        ));
    }
    let native = Fixture::new(&ctx);
    let native_store = super::native_installation::store(&native, &ctx);
    let expiry = now_seconds() + 600;
    put(
        &native_store,
        broker_config(&server.origin, true, false),
        native_profile(&other.origin, &server.token(true, expiry), expiry),
        true,
        &ctx,
    );
    let native_issuer = super::native_installation::issuer(&native, native_store, &ctx, &[]);
    let caller = native_issuer
        .authenticate_runtime(&native.secret(), &ctx)
        .unwrap();
    assert!(matches!(
        caller.native_provider(),
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));
    assert_eq!(server.count("/keys"), 0);
    assert_eq!(other.count("/keys"), 0);
}

#[tokio::test]
async fn native_provider_observes_revision_without_reusing_metadata_as_authority() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let expiry = now_seconds() + 600;
    populate(&store, &server, &ctx, &server.token(true, expiry), expiry);
    let issuer = issuer(&f, Arc::clone(&store), &ctx, &INSTALL);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let (first, revision) = caller.native_provider().unwrap();
    let old_verifier = server.verifier(false, revision);
    let changed = store
        .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
            let provider = doc.provider("service").unwrap();
            let version = provider.revision();
            let now = provider.updated_at() + 1;
            let config = provider.config().clone();
            doc.update_provider("service", config, doc.revision(), version, now, ctx)
        })
        .unwrap();
    assert_eq!(
        changed.effect(),
        graphit_core::auth_document::publication::AuthPublicationEffect::Applied
    );
    assert_eq!(changed.failure(), None);
    populate(&store, &server, &ctx, &server.token(true, expiry), expiry);
    let (current, newer_revision) = caller.native_provider().unwrap();
    assert_eq!(current, first);
    assert!(newer_revision > revision);
    assert!(matches!(
        caller.authenticate_native(&old_verifier).await,
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));
    assert_eq!(server.count("/keys"), 0);
}

#[tokio::test]
async fn native_provider_keeps_original_physical_cancel_deadline_and_decode_causes() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let expiry = now_seconds() + 600;
    populate(&store, &server, &ctx, &server.token(true, expiry), expiry);
    let issuer = issuer(&f, store, &ctx, &[]);
    let mut caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(caller.native_provider().is_ok());
    let foreign = context(&cancel);
    caller.request = &foreign;
    assert!(caller.native_provider().is_err());
    caller.request = &ctx;
    std::fs::write(
        f.installation.owner_root().join("auth.json"),
        b"{invalid-json-negative-fixture",
    )
    .unwrap();
    assert!(matches!(
        caller.native_provider(),
        Err(ServiceUseError::Store(
            crate::store::AuthStoreError::Profile(_)
        ))
    ));
    cancel.cancel();
    assert!(matches!(
        caller.native_provider(),
        Err(ServiceUseError::Profile(ProfileError::Cancelled))
    ));

    let deadline_cancel = Cancellation::default();
    let preparation = context(&deadline_cancel);
    let timed = Fixture::new(&preparation);
    let timed_store = super::native_installation::store(&timed, &preparation);
    populate(
        &timed_store,
        &server,
        &preparation,
        &server.token(true, expiry),
        expiry,
    );
    let timed_issuer = super::native_installation::issuer(&timed, timed_store, &preparation, &[]);
    let secret = timed.secret();
    // Bootstrap/publication precede the operation budget. Authenticate and
    // observe expiry with this same Original; its one-second value is unchanged.
    let deadline = Instant::now() + Duration::from_secs(1);
    let expired = request_before(&deadline_cancel, deadline);
    let timed_caller = timed_issuer
        .authenticate_runtime(&secret, &expired)
        .unwrap();
    timed_caller
        .native_provider()
        .expect("prepared native provider before operation deadline");
    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
    assert!(matches!(
        timed_caller.native_provider(),
        Err(ServiceUseError::Profile(ProfileError::Deadline))
    ));

    let physical_cancel = Cancellation::default();
    let current = context(&physical_cancel);
    let physical = Fixture::new(&current);
    let physical_store = super::native_installation::store(&physical, &current);
    populate(
        &physical_store,
        &server,
        &current,
        &server.token(true, expiry),
        expiry,
    );
    let physical_issuer =
        super::native_installation::issuer(&physical, physical_store, &current, &[]);
    let retained = physical_issuer
        .authenticate_runtime(&physical.secret(), &current)
        .unwrap();
    assert!(retained.native_provider().is_ok());
    let root = physical.installation.owner_root();
    #[cfg(unix)]
    {
        std::fs::rename(root, root.with_extension("replaced-installation")).unwrap();
        std::fs::create_dir(root).unwrap();
        assert!(matches!(
            retained.native_provider(),
            Err(ServiceUseError::Auth(_))
        ));
    }
    #[cfg(windows)]
    {
        let document = std::fs::read(root.join("auth.json")).unwrap();
        assert_retained_scope_refuses_rename(
            &physical.installation,
            &root.with_extension("replaced-installation"),
            &current,
        );
        assert!(std::fs::read(root.join("auth.json")).unwrap() == document);
        assert!(retained.native_provider().is_ok());
    }
    assert_eq!(server.count("/keys"), 0);
    assert_eq!(server.count("/userinfo"), 0);
}
