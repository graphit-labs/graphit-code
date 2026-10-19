#[path = "tests/broker_login.rs"]
pub(crate) mod broker_login;
#[path = "tests/broker_refresh.rs"]
mod broker_refresh;
use super::*;
use crate::profile::{
    AiService, LocalCredentials, ProfileExpectation, ProviderConfig, ProviderKind, S3Location,
    S3Source, Secret,
};
use crate::profile_manager::*;
use crate::store::AuthStoreError;
use graphit_core::auth_document::publication::AuthPublicationEffect;
use std::cell::RefCell;

thread_local! {
    static PREPARED: RefCell<Option<Box<dyn FnOnce()>>> = RefCell::new(None);
}
pub(crate) fn after_prepare() {
    if let Some(action) = PREPARED.with(|slot| slot.borrow_mut().take()) {
        action();
    }
}
fn after_encoding(action: impl FnOnce() + 'static) {
    PREPARED.with(|slot| assert!(slot.borrow_mut().replace(Box::new(action)).is_none()));
}
fn operation(cancel: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 4096,
            max_output_bytes: 4096,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 256,
            max_collection_entries: 64,
        },
        cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap()
}
fn manager(issuer: &RoleIssuer) -> NativeProfileManager<'_> {
    NativeProfileManager::new(issuer)
}
fn config() -> ProviderConfig {
    ProviderConfig {
        kind: ProviderKind::Local {
            allow_aws_credential_chain: false,
            broker: None,
        },
        embedding: AiService::Disabled,
        rerank: AiService::Disabled,
        s3: S3Location {
            bucket: None,
            region: None,
            endpoint: None,
            prefix: None,
            source: S3Source::LocalCredentials,
        },
    }
}
fn input(name: &str, key: Option<&str>) -> LocalProfileInput {
    LocalProfileInput {
        name: name.into(),
        provider: "local".into(),
        username: name.into(),
        organization: None,
        teams: vec![],
        credentials: LocalCredentials {
            anonymous: false,
            mcp_key: key.map(|k| Secret::new(SecretString::from(k))),
            broker_key: None,
            embedding_api_key: None,
            rerank_api_key: None,
            s3: None,
        },
    }
}
fn applied(value: graphit_core::auth_document::publication::AuthDocumentPublication) {
    assert_eq!(value.effect(), AuthPublicationEffect::Applied);
    assert_eq!(value.failure(), None);
    assert_eq!(value.cleanup_failure(), None);
    assert!(value.into_pending_cleanup().is_none());
}
fn begin<'a>(m: &'a NativeProfileManager<'a>, p: &'a AdminParent<'a>) -> ManagedAuthSnapshot<'a> {
    m.begin(ProfileManagerAuthority::Admin(p)).unwrap()
}

#[derive(Clone, Copy, Debug)]
enum CleanupDenial {
    WrongIssuer,
    ExpiredParent,
    RevokedRole,
}

fn assert_destination(path: &std::path::Path, prior: &Option<Vec<u8>>) {
    match prior {
        Some(bytes) => assert_eq!(std::fs::read(path).unwrap(), *bytes),
        None => assert!(!path.exists()),
    }
}

fn real_pending_cleanup(present: bool, denial: CleanupDenial) {
    use graphit_core::auth_document::{AuthDocumentError, recovery::AuthCleanupEffect};
    let cancellation = Cancellation::default();
    let mutation = operation(&cancellation);
    let f = Fixture::new(&mutation);
    let clock = clock();
    let issuer = fixture_issuer(
        &f,
        &mutation,
        clock.clone(),
        &all_permissions(),
        &all_permissions(),
        limits(),
        10,
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &mutation).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    if present {
        applied(m.ensure_default_local(begin(&m, &parent), 1, 1).unwrap());
    }
    let destination = f.installation.owner_root().join("auth.json");
    let prior = if present {
        Some(std::fs::read(&destination).unwrap())
    } else {
        assert!(!destination.exists());
        None
    };
    assert_eq!(prior.is_some(), present);
    let snapshot = begin(&m, &parent);
    let revision = snapshot.revision();
    let guard = issuer
        .local
        .cancel_publication_before_replace(&mutation)
        .unwrap();

    let result = m
        .add_provider(snapshot, "new-provider".into(), config(), revision, 2)
        .unwrap();
    assert_eq!(result.effect(), AuthPublicationEffect::NotApplied);
    assert_eq!(result.failure(), Some(AuthDocumentError::Cancelled));
    assert_eq!(result.cleanup_effect(), AuthCleanupEffect::NotApplied);
    assert_eq!(result.cleanup_failure(), Some(AuthDocumentError::Cancelled));
    let pending = result
        .into_pending_cleanup()
        .expect("actual Core creation proof");
    let stage = f.installation.owner_root().join(".auth.json.stage");
    let staged = std::fs::read(&stage).unwrap();
    assert!(!staged.is_empty());
    assert_destination(&destination, &prior);

    drop(guard);
    assert_eq!(std::fs::read(&stage).unwrap(), staged);

    assert!(mutation.snapshot().check().is_err());

    // Cleanup is a separate request. It never renews the cancelled mutation.
    let cleanup_cancel = Cancellation::default();
    let cleanup = operation(&cleanup_cancel);
    let current_caller = issuer.authenticate_runtime(&f.secret(), &cleanup).unwrap();
    let current_session = issuer.bootstrap_admin(&current_caller).unwrap();
    let current_parent = issuer
        .admin_parent(&current_caller, &current_session)
        .unwrap();
    let (denied, expected) = match denial {
        CleanupDenial::WrongIssuer => {
            let other = f.issuer(&cleanup, limits());
            let other_caller = other.authenticate_runtime(&f.secret(), &cleanup).unwrap();
            let other_session = other.bootstrap_admin(&other_caller).unwrap();
            let other_parent = other.admin_parent(&other_caller, &other_session).unwrap();

            (
                m.cleanup_pending(ProfileManagerAuthority::Admin(&other_parent), pending)
                    .err()
                    .unwrap(),
                AuthError::Denied,
            )
        }
        CleanupDenial::ExpiredParent => {
            clock.seconds.store(10, Ordering::SeqCst);

            (
                m.cleanup_pending(ProfileManagerAuthority::Admin(&current_parent), pending)
                    .err()
                    .unwrap(),
                AuthError::Expired,
            )
        }
        CleanupDenial::RevokedRole => {
            let definition = f.definition(
                Role::Coordinator,
                &[Permission::ManageAuthProfiles],
                1,
                limits(),
            );
            let reference = definition.reference().clone();
            issuer
                .register_definition(&current_parent, definition)
                .unwrap();
            let run = issuer
                .start_run(&current_parent, &f.scope(), Duration::from_secs(900))
                .unwrap();
            let bearer = issuer
                .issue(
                    &current_parent,
                    &run,
                    &reference,
                    &[Permission::ManageAuthProfiles],
                    &f.scope(),
                )
                .unwrap();
            let grant = issuer
                .verify(
                    bearer.expose_for_transport(),
                    &reference,
                    f.scope(),
                    &cleanup,
                )
                .unwrap();
            issuer.revoke(&current_parent, &bearer).unwrap();

            (
                m.cleanup_pending(ProfileManagerAuthority::Role(&grant), pending)
                    .err()
                    .unwrap(),
                AuthError::Denied,
            )
        }
    };
    assert_eq!(denied.error(), ProfileManagerError::Authority(expected));

    assert_eq!(std::fs::read(&stage).unwrap(), staged);
    assert_destination(&destination, &prior);
    let pending = denied.into_pending_cleanup();

    let retry_cancel = Cancellation::default();
    let retry = operation(&retry_cancel);
    let retry_caller = issuer.authenticate_runtime(&f.secret(), &retry).unwrap();
    let retry_session = issuer.bootstrap_admin(&retry_caller).unwrap();
    let retry_parent = issuer.admin_parent(&retry_caller, &retry_session).unwrap();
    let recovered = match m.cleanup_pending(ProfileManagerAuthority::Admin(&retry_parent), pending)
    {
        Ok(result) => result,
        Err(_) => panic!("fresh actual authority must admit original cleanup proof"),
    };
    assert_eq!(recovered.effect(), AuthCleanupEffect::Applied);
    assert_eq!(recovered.failure(), None);
    assert!(recovered.into_pending_cleanup().is_none());
    assert!(!stage.exists());
    assert_destination(&destination, &prior);
    assert!(mutation.snapshot().check().is_err());
}

#[test]
fn real_pending_present_wrong_issuer_then_fresh_cleanup() {
    real_pending_cleanup(true, CleanupDenial::WrongIssuer);
}
#[test]
fn real_pending_absent_wrong_issuer_then_fresh_cleanup() {
    real_pending_cleanup(false, CleanupDenial::WrongIssuer);
}
#[test]
fn real_pending_present_expired_parent_then_fresh_cleanup() {
    real_pending_cleanup(true, CleanupDenial::ExpiredParent);
}
#[test]
fn real_pending_absent_expired_parent_then_fresh_cleanup() {
    real_pending_cleanup(false, CleanupDenial::ExpiredParent);
}
#[test]
fn real_pending_present_revoked_role_then_fresh_cleanup() {
    real_pending_cleanup(true, CleanupDenial::RevokedRole);
}
#[test]
fn real_pending_absent_revoked_role_then_fresh_cleanup() {
    real_pending_cleanup(false, CleanupDenial::RevokedRole);
}

#[test]
fn absent_redacted_provider_local_lifecycle_and_omitted_keys() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let empty = begin(&m, &parent);
    assert!(!empty.was_present());
    assert_eq!(empty.revision(), 1);
    assert_eq!(empty.providers().len(), 0);
    assert_eq!(empty.profiles().len(), 0);
    assert_eq!(empty.active_name(), None);
    assert!(!f.installation.owner_root().join("auth.json").exists());
    applied(
        m.add_provider(empty, "local".into(), config(), 1, 1)
            .unwrap(),
    );
    let s = begin(&m, &parent);
    let provider = s.providers().next().unwrap().revision();
    let revision = s.revision();
    applied(
        m.install_local(
            s,
            input("a", Some("old-local-key")),
            ProfileExpectation {
                document_revision: revision,
                provider_revision: provider,
                profile_revision: None,
            },
            2,
        )
        .unwrap(),
    );
    let s = begin(&m, &parent);
    assert_eq!(s.active_name(), Some("a"));
    let v = s.profiles().next().unwrap();
    assert_eq!(v.username(), Some("a"));
    assert_eq!(v.teams().expect("Local identity has teams").len(), 0);
    let old_profile = v.revision();
    let old_token = v.token_revision();
    let birth = v.created_at();
    let revision = s.revision();
    applied(
        m.install_local(
            s,
            input("a", None),
            ProfileExpectation {
                document_revision: revision,
                provider_revision: provider,
                profile_revision: Some(old_profile),
            },
            3,
        )
        .unwrap(),
    );
    let s = begin(&m, &parent);
    let v = s.profiles().next().unwrap();
    assert_eq!(v.created_at(), birth);
    assert_eq!(v.revision(), old_profile + 1);
    assert_eq!(v.token_revision(), old_token + 1);
    let bytes = std::fs::read(f.installation.owner_root().join("auth.json")).unwrap();
    assert!(!String::from_utf8(bytes).unwrap().contains("old-local-key"));
    assert!(issuer.authenticate_local("old-local-key", &ctx).is_err());
    let revision = s.revision();
    applied(m.logout(s, None, revision).unwrap());
    let s = begin(&m, &parent);
    assert_eq!(s.profiles().len(), 0);
    assert_eq!(s.active_name(), None);
    let revision = s.revision();
    applied(m.remove_provider(s, "local", false, revision).unwrap());
    let s = begin(&m, &parent);
    assert_eq!(s.providers().len(), 0);
    let revision = s.revision();
    applied(m.ensure_default_local(s, revision, 4).unwrap());
    let s = begin(&m, &parent);
    assert_eq!(s.providers().len(), 1);
    assert_eq!(s.profiles().len(), 0);
}

#[test]
fn wrong_issuer_zero_io_and_missing_finite_permission_zero_effects() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let all = all_permissions();
    let admin: Vec<_> = all
        .iter()
        .copied()
        .filter(|p| *p != Permission::ManageAuthProfiles)
        .collect();
    let issuer = fixture_issuer(&f, &ctx, clock(), &all, &admin, limits(), 7200);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert_eq!(
        caller.authorize(Permission::ManageAuthProfiles),
        Ok(Obligation::Unconditional)
    );
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);

    assert!(matches!(
        m.begin(ProfileManagerAuthority::Admin(&parent)),
        Err(ProfileManagerError::Authority(AuthError::Denied))
    ));
    // Missing permission is checked after the mandatory fresh source check.

    assert!(!f.installation.owner_root().join("auth.json").exists());
    let other = f.issuer(&ctx, limits());
    let m = manager(&other);

    assert!(matches!(
        m.begin(ProfileManagerAuthority::Admin(&parent)),
        Err(ProfileManagerError::Authority(AuthError::Denied))
    ));
}

#[test]
fn owned_snapshots_are_independent_and_preserve_original_cancellation() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = NativeProfileManager::new(&issuer);
    let first = begin(&m, &parent);
    let second = begin(&m, &parent);
    let third = begin(&m, &parent);
    let revision = first.revision();
    assert_eq!(second.revision(), revision);
    assert_eq!(third.revision(), revision);
    drop(first);
    let fourth = begin(&m, &parent);
    assert_eq!(fourth.revision(), revision);
    cancel.cancel();
    assert!(matches!(
        m.add_provider(second, "local".into(), config(), revision, 1),
        Err(ProfileManagerError::Store(AuthStoreError::Core(
            graphit_core::auth_document::AuthDocumentError::Cancelled
        )))
    ));
    assert!(matches!(
        m.begin(ProfileManagerAuthority::Admin(&parent)),
        Err(ProfileManagerError::Authority(AuthError::Request))
    ));
    drop(third);
    drop(fourth);
    assert!(!f.installation.owner_root().join("auth.json").exists());
}

#[test]
fn expiry_after_encoding_preserves_previous_bytes() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let clock = clock();
    let issuer = fixture_issuer(
        &f,
        &ctx,
        clock.clone(),
        &all_permissions(),
        &all_permissions(),
        limits(),
        10,
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let s = begin(&m, &parent);

    after_encoding(move || clock.seconds.store(10, Ordering::SeqCst));
    assert!(matches!(
        m.add_provider(s, "local".into(), config(), 1, 1),
        Err(ProfileManagerError::Authority(AuthError::Expired))
    ));

    assert!(!f.installation.owner_root().join("auth.json").exists());
}

#[test]
fn real_role_permission_revocation_and_wrong_issuer() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let def = f.definition(
        Role::Coordinator,
        &[Permission::ManageAuthProfiles, Permission::InspectGit],
        1,
        limits(),
    );
    let reference = def.reference().clone();
    issuer.register_definition(&parent, def).unwrap();
    let run = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(900))
        .unwrap();
    let bearer = issuer
        .issue(
            &parent,
            &run,
            &reference,
            &[Permission::ManageAuthProfiles],
            &f.scope(),
        )
        .unwrap();
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let m = manager(&issuer);
    let s = m.begin(ProfileManagerAuthority::Role(&grant)).unwrap();
    applied(m.add_provider(s, "local".into(), config(), 1, 1).unwrap());
    let other = f.issuer(&ctx, limits());
    let other_manager = manager(&other);

    assert!(
        other_manager
            .begin(ProfileManagerAuthority::Role(&grant))
            .is_err()
    );

    let denied = issuer
        .issue(
            &parent,
            &run,
            &reference,
            &[Permission::InspectGit],
            &f.scope(),
        )
        .unwrap();
    let denied_grant = issuer
        .verify(denied.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    assert!(
        m.begin(ProfileManagerAuthority::Role(&denied_grant))
            .is_err()
    );
    let s = m.begin(ProfileManagerAuthority::Role(&grant)).unwrap();
    let before = std::fs::read(f.installation.owner_root().join("auth.json")).unwrap();
    issuer.revoke(&parent, &bearer).unwrap();
    let revision = s.revision();
    assert!(m.remove_provider(s, "local", false, revision).is_err());
    assert_eq!(
        std::fs::read(f.installation.owner_root().join("auth.json")).unwrap(),
        before
    );
}

#[test]
fn local_self_logout_returns_applied_then_admission_fails() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let runtime = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&runtime).unwrap();
    let parent = issuer.admin_parent(&runtime, &session).unwrap();
    let m = manager(&issuer);
    applied(
        m.add_provider(begin(&m, &parent), "local".into(), config(), 1, 1)
            .unwrap(),
    );
    let s = begin(&m, &parent);
    let revision = s.revision();
    let provider = s.providers().next().unwrap().revision();
    applied(
        m.install_local(
            s,
            input("a", Some("manager-local-key")),
            ProfileExpectation {
                document_revision: revision,
                provider_revision: provider,
                profile_revision: None,
            },
            2,
        )
        .unwrap(),
    );
    let local_request = operation(&cancel);
    let local = issuer
        .authenticate_local("manager-local-key", &local_request)
        .unwrap();
    let session = issuer.bootstrap_admin(&local).unwrap();
    let local_parent = issuer.admin_parent(&local, &session).unwrap();
    let s = begin(&m, &local_parent);
    let revision = s.revision();

    applied(m.logout(s, None, revision).unwrap());

    assert!(
        m.begin(ProfileManagerAuthority::Admin(&local_parent))
            .is_err()
    );
    let fresh = begin(&m, &parent);
    assert_eq!(fresh.profiles().len(), 0);
    assert_eq!(fresh.active_name(), None);
}

#[test]
fn provider_conflicts_staleness_explicit_cascade_and_wrong_manager() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    applied(
        m.add_provider(begin(&m, &parent), "local".into(), config(), 1, 1)
            .unwrap(),
    );
    let s = begin(&m, &parent);
    let revision = s.revision();
    let provider = s.providers().next().unwrap().revision();
    applied(
        m.install_local(
            s,
            input("a", None),
            ProfileExpectation {
                document_revision: revision,
                provider_revision: provider,
                profile_revision: None,
            },
            2,
        )
        .unwrap(),
    );
    let path = f.installation.owner_root().join("auth.json");
    let before = std::fs::read(&path).unwrap();
    let s = begin(&m, &parent);
    let revision = s.revision();
    assert!(m.remove_provider(s, "local", false, revision).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let other_manager = manager(&issuer);
    let s = begin(&m, &parent);
    let revision = s.revision();

    assert!(matches!(
        other_manager.logout(s, None, revision),
        Err(ProfileManagerError::WrongManager)
    ));

    let s = begin(&m, &parent);
    let revision = s.revision();
    assert!(
        m.update_provider(s, "local", config(), revision, provider + 1, 3)
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let s = begin(&m, &parent);
    let revision = s.revision();
    applied(
        m.update_provider(s, "local", config(), revision, provider, 3)
            .unwrap(),
    );
    let s = begin(&m, &parent);
    let revision = s.revision();
    let current = std::fs::read(&path).unwrap();
    assert!(m.activate(s, "a", revision).is_err()); // captured provider incarnation is stale
    assert_eq!(std::fs::read(&path).unwrap(), current);
    let s = begin(&m, &parent);
    let revision = s.revision();
    applied(m.remove_provider(s, "local", true, revision).unwrap());
    let s = begin(&m, &parent);
    assert_eq!(s.profiles().len(), 0);
    assert_eq!(s.providers().len(), 0);
    assert_eq!(s.active_name(), None);
}

#[test]
fn source_change_after_encoding_and_cancellation_deny_without_publication() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let s = begin(&m, &parent);
    let source = f.installation.owner_root().join("daemon/mcp.key");
    after_encoding(move || {
        let old = std::fs::read(&source).unwrap();
        let mut changed = old;
        changed[0] = if changed[0] == b'A' { b'B' } else { b'A' };
        std::fs::write(source, changed).unwrap();
    });

    assert!(m.add_provider(s, "local".into(), config(), 1, 1).is_err());

    assert!(!f.installation.owner_root().join("auth.json").exists());
    cancel.cancel();

    assert!(m.begin(ProfileManagerAuthority::Admin(&parent)).is_err());
}

#[test]
fn competing_original_observation_is_not_reloaded_or_rebased() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let a = begin(&m, &parent);
    let b = begin(&m, &parent);
    applied(m.add_provider(a, "local".into(), config(), 1, 1).unwrap());
    let path = f.installation.owner_root().join("auth.json");
    let before = std::fs::read(&path).unwrap();
    assert!(m.add_provider(b, "second".into(), config(), 1, 1).is_err());
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn real_stage_recovery_fresh_authority_and_expired_proof_denial() {
    use graphit_core::auth_document::recovery::AuthCleanupEffect;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let clock = clock();
    let issuer = fixture_issuer(
        &f,
        &ctx,
        clock.clone(),
        &all_permissions(),
        &all_permissions(),
        limits(),
        10,
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    assert_eq!(
        m.recover_stage(ProfileManagerAuthority::Admin(&parent))
            .unwrap()
            .effect(),
        AuthCleanupEffect::Absent
    );
    let path = f.installation.owner_root().join(".auth.json.stage");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    file.write_all(b"unparsed stage fixture").unwrap();
    drop(file);
    clock.seconds.store(10, Ordering::SeqCst);

    assert!(matches!(
        m.recover_stage(ProfileManagerAuthority::Admin(&parent)),
        Err(ProfileManagerError::Authority(AuthError::Expired))
    ));
    assert!(path.exists());

    // A new session authenticates independently; the old expired parent is never renewed.
    let new_caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let new_session = issuer.bootstrap_admin(&new_caller).unwrap();
    let fresh = issuer.admin_parent(&new_caller, &new_session).unwrap();
    let recovery = m
        .recover_stage(ProfileManagerAuthority::Admin(&fresh))
        .unwrap();
    assert_eq!(recovery.effect(), AuthCleanupEffect::Applied);
    assert_eq!(recovery.failure(), None);
    assert!(!path.exists());
    assert!(!f.installation.owner_root().join("auth.json").exists());
}

#[test]
fn missing_provider_invalid_local_data_and_activation_overflow_preserve_bytes() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    assert!(
        m.install_local(
            begin(&m, &parent),
            input("a", None),
            ProfileExpectation {
                document_revision: 1,
                provider_revision: 1,
                profile_revision: None
            },
            1
        )
        .is_err()
    );
    let path = f.installation.owner_root().join("auth.json");
    assert!(!path.exists());
    applied(
        m.add_provider(begin(&m, &parent), "local".into(), config(), 1, 1)
            .unwrap(),
    );
    let s = begin(&m, &parent);
    let revision = s.revision();
    let provider = s.providers().next().unwrap().revision();
    let mut invalid = input("a", None);
    invalid.credentials.anonymous = true;
    let before = std::fs::read(&path).unwrap();
    assert!(
        m.install_local(
            s,
            invalid,
            ProfileExpectation {
                document_revision: revision,
                provider_revision: provider,
                profile_revision: None
            },
            2
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let s = begin(&m, &parent);
    let revision = s.revision();
    applied(
        m.install_local(
            s,
            input("a", None),
            ProfileExpectation {
                document_revision: revision,
                provider_revision: provider,
                profile_revision: None,
            },
            2,
        )
        .unwrap(),
    );
    let bytes = String::from_utf8(std::fs::read(&path).unwrap()).unwrap();
    let old_revision = begin(&m, &parent).revision();
    let changed = bytes.replace(
        &format!("\"document_revision\":{old_revision}"),
        &format!("\"document_revision\":{}", u64::MAX - 1),
    );
    assert_ne!(bytes, changed);
    std::fs::write(&path, changed.as_bytes()).unwrap();
    let s = begin(&m, &parent);
    let record = s.profiles().next().unwrap();
    let profile = record.revision();
    let revision = s.revision();

    assert!(matches!(
        m.install_local(
            s,
            input("a", None),
            ProfileExpectation {
                document_revision: revision,
                provider_revision: provider,
                profile_revision: Some(profile)
            },
            3
        ),
        Err(ProfileManagerError::Store(AuthStoreError::Profile(
            crate::profile::ProfileError::Limit
        )))
    ));
    assert_eq!(std::fs::read(&path).unwrap(), changed.as_bytes());
}

#[test]
fn anonymous_supplied_keys_and_default_kind_conflict() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let mut anonymous = config();
    anonymous.kind = ProviderKind::Local {
        allow_aws_credential_chain: false,
        broker: Some(crate::profile::BrokerService {
            endpoint: "https://broker.example".into(),
            mcp_resource: None,
            allow_anonymous: true,
        }),
    };
    anonymous.embedding = AiService::Broker;
    anonymous.rerank = AiService::Broker;
    applied(
        m.add_provider(begin(&m, &parent), "local".into(), anonymous, 1, 1)
            .unwrap(),
    );
    let s = begin(&m, &parent);
    let revision = s.revision();
    let provider = s.providers().next().unwrap().revision();
    let mut value = input("anonymous", Some("supplied-anonymous-mcp"));
    value.credentials.anonymous = true;
    applied(
        m.install_local(
            s,
            value,
            ProfileExpectation {
                document_revision: revision,
                provider_revision: provider,
                profile_revision: None,
            },
            2,
        )
        .unwrap(),
    );
    issuer
        .authenticate_local("supplied-anonymous-mcp", &ctx)
        .unwrap();
    let s = begin(&m, &parent);
    assert_eq!(s.active_name(), Some("anonymous"));
    assert_eq!(s.profiles().next().unwrap().username(), Some("anonymous"));
    drop(s);
    let keyless = input("anonymous", None);
    let mut keyless = keyless;
    keyless.credentials.anonymous = true;
    let s = begin(&m, &parent);
    let revision = s.revision();
    let profile = s.profiles().next().unwrap().revision();
    applied(
        m.install_local(
            s,
            keyless,
            ProfileExpectation {
                document_revision: revision,
                provider_revision: provider,
                profile_revision: Some(profile),
            },
            3,
        )
        .unwrap(),
    );
    assert!(
        issuer
            .authenticate_local("supplied-anonymous-mcp", &ctx)
            .is_err()
    );
    let s = begin(&m, &parent);
    let revision = s.revision();
    applied(m.remove_provider(s, "local", true, revision).unwrap());
    let mut broker = config();
    broker.kind = ProviderKind::Broker(crate::profile::BrokerService {
        endpoint: "https://broker.example".into(),
        mcp_resource: None,
        allow_anonymous: false,
    });
    broker.embedding = AiService::Broker;
    broker.rerank = AiService::Broker;
    broker.s3.source = S3Source::BrokerScope;
    let s = begin(&m, &parent);
    let revision = s.revision();
    applied(
        m.add_provider(s, "local".into(), broker, revision, 4)
            .unwrap(),
    );
    let path = f.installation.owner_root().join("auth.json");
    let before = std::fs::read(&path).unwrap();
    let s = begin(&m, &parent);
    let revision = s.revision();
    assert!(m.ensure_default_local(s, revision, 5).is_err());
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn poisoned_state_denies_profile_read_and_publication() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let s = begin(&m, &parent);
    std::thread::scope(|scope| {
        let thread = scope.spawn(|| {
            let _guard = issuer.state.lock().unwrap();
            panic!("test issuer state failure");
        });
        assert!(thread.join().is_err());
    });
    assert!(matches!(
        m.begin(ProfileManagerAuthority::Admin(&parent)),
        Err(ProfileManagerError::Authority(AuthError::State))
    ));
    assert!(matches!(
        m.add_provider(s, "local".into(), config(), 1, 1),
        Err(ProfileManagerError::Authority(AuthError::State))
    ));
    assert!(!f.installation.owner_root().join("auth.json").exists());
}

#[test]
fn concurrent_snapshot_reads_preserve_authority_contention_and_held_observation() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = NativeProfileManager::new(&issuer);
    let snapshot = begin(&m, &parent);
    let revision = snapshot.revision();
    std::thread::scope(|scope| {
        let read = || match m.begin(ProfileManagerAuthority::Admin(&parent)) {
            Ok(observed) => assert_eq!(observed.revision(), revision),
            Err(ProfileManagerError::Authority(AuthError::State)) => {}
            Err(error) => panic!("unexpected observation error: {error}"),
        };
        let first = scope.spawn(read);
        let second = scope.spawn(read);
        first.join().unwrap();
        second.join().unwrap();
    });
    assert_eq!(snapshot.revision(), revision);
    drop(snapshot);
    assert_eq!(begin(&m, &parent).revision(), revision);
    assert!(!f.installation.owner_root().join("auth.json").exists());
}

#[test]
fn cancellation_after_encoding_denies_before_core() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let s = begin(&m, &parent);
    let cancellation = cancel.clone();
    after_encoding(move || cancellation.cancel());

    assert!(matches!(
        m.add_provider(s, "local".into(), config(), 1, 1),
        Err(ProfileManagerError::Authority(AuthError::Request))
    ));
    assert!(!f.installation.owner_root().join("auth.json").exists());
}

#[test]
fn captured_role_expiry_and_definition_generation_precede_publication() {
    for changed_definition in [false, true] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let clock = clock();
        let issuer = fixture_issuer(
            &f,
            &ctx,
            clock.clone(),
            &all_permissions(),
            &all_permissions(),
            limits(),
            7200,
        );
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let def = f.definition(
            Role::Coordinator,
            &[Permission::ManageAuthProfiles],
            1,
            limits(),
        );
        let reference = def.reference().clone();
        issuer.register_definition(&parent, def).unwrap();
        let run = issuer
            .start_run(&parent, &f.scope(), Duration::from_secs(900))
            .unwrap();
        let bearer = issuer
            .issue(
                &parent,
                &run,
                &reference,
                &[Permission::ManageAuthProfiles],
                &f.scope(),
            )
            .unwrap();
        let grant = issuer
            .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
            .unwrap();
        let m = manager(&issuer);
        let s = m.begin(ProfileManagerAuthority::Role(&grant)).unwrap();
        if changed_definition {
            issuer
                .replace_definition(
                    &parent,
                    &reference,
                    f.definition(
                        Role::Coordinator,
                        &[Permission::ManageAuthProfiles],
                        2,
                        limits(),
                    ),
                )
                .unwrap();
        } else {
            clock.seconds.store(900, Ordering::SeqCst);
        }

        assert!(m.add_provider(s, "local".into(), config(), 1, 1).is_err());
        assert!(!f.installation.owner_root().join("auth.json").exists());
    }
}

#[test]
fn broker_views_are_declared_data_and_local_api_cannot_replace_broker_credentials() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let mut remote = config();
    remote.kind = ProviderKind::Broker(crate::profile::BrokerService {
        endpoint: "https://broker.example".into(),
        mcp_resource: None,
        allow_anonymous: false,
    });
    remote.embedding = AiService::Broker;
    remote.rerank = AiService::Broker;
    remote.s3.source = S3Source::BrokerScope;
    // A typed storage fixture is explicitly unverified data, never a Broker producer.
    let raw = issuer.local.load(&ctx).unwrap();
    applied(
        issuer
            .local
            .mutate(raw, &ctx, |doc, ctx| {
                doc.add_provider("remote".into(), remote, doc.revision(), 1, ctx)?;
                let provider_revision = doc.provider("remote").unwrap().revision();
                doc.replace_profile_data(
                    crate::profile::ProfileData {
                        name: "remote-profile".into(),
                        provider: "remote".into(),
                        username: "declared-user".into(),
                        organization: Some("declared-org".into()),
                        teams: vec!["declared-team".into()],
                        credentials: crate::profile::ProfileCredentials::Broker {
                            issuer: "https://broker.example".into(),
                            subject: "declared-subject".into(),
                            tokens: crate::profile::OidcTokens {
                                access_token: Secret::new(SecretString::from("access-fixture")),
                                refresh_token: None,
                                id_token: Secret::new(SecretString::from("id-fixture")),
                                expires_at: 100,
                            },
                            s3_disabled: true,
                        },
                    },
                    ProfileExpectation {
                        document_revision: doc.revision(),
                        provider_revision,
                        profile_revision: None,
                    },
                    2,
                    ctx,
                )
            })
            .unwrap(),
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let s = begin(&m, &parent);

    let view = s.profiles().next().unwrap();
    assert_eq!(
        view.broker_identity(),
        Some(("https://broker.example", "declared-subject"))
    );
    assert_eq!(view.local_anonymous(), None);
    assert_eq!(view.organization(), Some("declared-org"));
    assert_eq!(
        view.teams()
            .expect("Code identity has teams")
            .collect::<Vec<_>>(),
        vec!["declared-team"]
    );

    let expected = ProfileExpectation {
        document_revision: s.revision(),
        provider_revision: view.provider_revision(),
        profile_revision: Some(view.revision()),
    };
    let mut local = input("remote-profile", None);
    local.provider = "remote".into();
    let path = f.installation.owner_root().join("auth.json");
    let bytes = std::fs::read(&path).unwrap();
    assert!(m.install_local(s, local, expected, 3).is_err());
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}
