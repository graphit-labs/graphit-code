use super::*;
use crate::profile::{
    AiService, LocalCredentials, ProfileCredentials, ProfileData, ProfileExpectation,
    ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
};

const KEY: &str = "local-key-fixture";
fn config() -> ProviderConfig {
    ProviderConfig {
        kind: ProviderKind::Local {
            allow_aws_credential_chain: false,
            broker: Some(crate::profile::BrokerService {
                endpoint: "https://broker.example".into(),
                mcp_resource: None,
                allow_anonymous: true,
            }),
        },
        embedding: AiService::Broker,
        rerank: AiService::Broker,
        s3: S3Location {
            bucket: None,
            region: None,
            endpoint: None,
            prefix: None,
            source: S3Source::LocalCredentials,
        },
    }
}
fn data(name: &str, key: Option<&str>, anonymous: bool) -> ProfileData {
    ProfileData {
        name: name.into(),
        provider: "local".into(),
        username: if anonymous {
            "anonymous".into()
        } else {
            name.into()
        },
        organization: None,
        teams: vec![],
        credentials: ProfileCredentials::Local(LocalCredentials {
            anonymous,
            mcp_key: key.map(|v| Secret::new(SecretString::from(v))),
            broker_key: None,
            embedding_api_key: None,
            rerank_api_key: None,
            s3: None,
        }),
    }
}
fn applied(value: graphit_core::auth_document::publication::AuthDocumentPublication) {
    assert_eq!(
        value.effect(),
        graphit_core::auth_document::publication::AuthPublicationEffect::Applied
    );
    assert_eq!(value.failure(), None);
    assert_eq!(value.cleanup_failure(), None);
}
fn put(store: &AuthStore, profile: ProfileData, activate: bool, ctx: &RequestContext<'_>) {
    let snapshot = store.load(ctx).unwrap();
    applied(
        store
            .mutate(snapshot, ctx, |doc, ctx| {
                if doc.provider("local").is_none() {
                    doc.add_provider("local".into(), config(), doc.revision(), 1, ctx)?;
                }
                let provider_revision = doc.provider("local").unwrap().revision();
                let previous = doc.profile(&profile.name);
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision,
                    profile_revision: previous.map(|p| p.revision()),
                };
                let now = previous.map_or(1, |p| p.updated_at() + 1);
                let name = profile.name.clone();
                doc.replace_profile_data(profile, expected, now, ctx)?;
                if activate {
                    doc.activate(&name, doc.revision(), ctx)?;
                }
                Ok(())
            })
            .unwrap(),
    );
}
fn local_issuer(f: &Fixture, ctx: &RequestContext<'_>, store: Arc<AuthStore>) -> RoleIssuer {
    let all = all_permissions();
    let sessions = crate::AdminSessionPolicy {
        lifetime: Duration::from_secs(7200),
        maximum: Duration::from_secs(7200),
    };
    let policy = crate::IssuerAuthorityPolicy::new(
        crate::SourceAuthorityPolicy {
            ordinary: &all,
            administrative: &all,
            sessions,
        },
        crate::SourceAuthorityPolicy {
            ordinary: &[Permission::InspectGit],
            administrative: &all,
            sessions,
        },
        limits(),
        deny_services(),
    )
    .unwrap();
    RoleIssuer::new(f.realm(), Arc::clone(&f.runtime), store, policy, ttl(), ctx).unwrap()
}

#[test]
fn real_local_and_runtime_ordinary_admin_roles_share_one_state_without_defaults() {
    let cancel = Cancellation::default();
    let startup = request(&cancel);
    let f = Fixture::new(&startup);
    let store = f.auth_store(&startup);
    let issuer = local_issuer(&f, &startup, Arc::clone(&store));
    assert!(!f.installation.owner_root().join("auth.json").exists());
    let runtime = issuer.authenticate_runtime(&f.secret(), &startup).unwrap();
    runtime.authorize(Permission::ManageAuthProfiles).unwrap();
    assert!(issuer.authenticate_local(KEY, &startup).is_err());
    assert!(issuer.lock().unwrap().sessions.is_empty());
    let mutation = request(&cancel);
    put(&store, data("a", Some(KEY), true), true, &mutation);
    let op = request_for_documents(&cancel);
    let local = issuer.authenticate_local(KEY, &op).unwrap();
    assert_eq!(
        local.authorize(Permission::InspectGit),
        Ok(Obligation::Unconditional)
    );
    assert_eq!(
        local.authorize(Permission::ManageAuthProfiles),
        Err(AuthError::Denied)
    );
    assert_ne!(
        actor_bytes(&runtime.actor().unwrap()),
        actor_bytes(&local.actor().unwrap())
    );
    assert!(issuer.lock().unwrap().sessions.is_empty());
    let session = issuer.bootstrap_admin(&local).unwrap();
    let parent = issuer.admin_parent(&local, &session).unwrap();
    assert!(issuer.admin_parent(&runtime, &session).is_err());
    let read = Permission::InspectGit;
    let def = f.definition(
        Role::Coordinator,
        &[read, Permission::Delegate],
        1,
        limits(),
    );
    let reference = def.reference().clone();
    issuer.register_definition(&parent, def).unwrap();
    let run = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let bearer = issuer
        .issue(
            &parent,
            &run,
            &reference,
            &[read, Permission::Delegate],
            &f.scope(),
        )
        .unwrap();

    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &op)
        .unwrap();

    grant.authorize(read).unwrap();

    grant.actor().unwrap();

    grant.refresh_due().unwrap();

    let child = issuer
        .delegate(&grant, &reference, &[read], Duration::from_secs(60))
        .unwrap();
    issuer
        .verify(
            child.bearer.expose_for_transport(),
            &reference,
            f.scope(),
            &op,
        )
        .unwrap()
        .authorize(read)
        .unwrap();
    let refreshed = issuer.refresh(&parent, &bearer, &f.scope()).unwrap();
    assert!(
        issuer
            .verify(bearer.expose_for_transport(), &reference, f.scope(), &op)
            .is_err()
    );
    issuer.revoke(&parent, &refreshed).unwrap();
    assert!(
        issuer
            .verify(
                child.bearer.expose_for_transport(),
                &reference,
                f.scope(),
                &op
            )
            .is_err()
    );
    issuer.close_run(&parent, &run).unwrap();
    runtime.authorize(Permission::ManageAuthProfiles).unwrap();
}

#[test]
fn local_observations_are_independent_and_release_with_their_original_callers() {
    let cancel = Cancellation::default();
    let setup = request(&cancel);
    let f = Fixture::new(&setup);
    let store = f.auth_store(&setup);
    put(&store, data("a", Some(KEY), false), true, &setup);
    let issuer = local_issuer(&f, &setup, store);
    let op = request(&cancel);
    let first = issuer.authenticate_local(KEY, &op).unwrap();
    let SourceProof::Local(proof) = &first.source else {
        panic!("local fixture")
    };
    let original = Arc::downgrade(proof);
    let second = issuer.authenticate_local(KEY, &op).unwrap();
    first.check().unwrap();
    second.check().unwrap();
    assert_eq!(
        actor_bytes(&first.actor().unwrap()),
        actor_bytes(&second.actor().unwrap())
    );
    drop(first);
    assert!(original.upgrade().is_none());
    second.check().unwrap();
    assert!(issuer.authenticate_local("wrong", &op).is_err());
    let third = issuer.authenticate_local(KEY, &op).unwrap();
    third.check().unwrap();
    second.check().unwrap();
}

#[test]
fn unrelated_edits_preserve_fresh_local_lifetime_but_original_proof_is_not_rebased() {
    let cancel = Cancellation::default();
    let setup = request(&cancel);
    let f = Fixture::new(&setup);
    let store = f.auth_store(&setup);
    put(&store, data("a", Some(KEY), false), true, &setup);
    let issuer = local_issuer(&f, &setup, Arc::clone(&store));
    let op = request(&cancel);
    let caller = issuer.authenticate_local(KEY, &op).unwrap();
    let actor = actor_bytes(&caller.actor().unwrap());
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let run = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let generation = caller.source.generation();
    let read = Permission::InspectGit;
    let definition = f.definition(Role::Coordinator, &[read], 1, limits());
    let reference = definition.reference().clone();
    issuer.register_definition(&parent, definition).unwrap();
    let bearer = issuer
        .issue(&parent, &run, &reference, &[read], &f.scope())
        .unwrap();
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &op)
        .unwrap();
    let expiry = issuer.lock().unwrap().sessions[&session.id].expiry;
    let mutation = request(&cancel);
    put(&store, data("b", Some("b-key"), false), false, &mutation);
    assert!(caller.check().is_err());
    assert!(grant.authorize(read).is_err());
    let fresh = request(&cancel);
    let caller = issuer.authenticate_local(KEY, &fresh).unwrap();
    assert_eq!(actor_bytes(&caller.actor().unwrap()), actor);
    assert_eq!(caller.source.generation(), generation);
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &fresh)
        .unwrap()
        .authorize(read)
        .unwrap();
    issuer.close_run(&parent, &run).unwrap();
    let current = issuer.lock().unwrap().sessions[&session.id].expiry;
    assert_eq!(current.utc, expiry.utc);
    assert_eq!(current.mono, expiry.mono);
}

#[test]
fn local_activation_and_logout_recreation_revoke_only_local_source() {
    let cancel = Cancellation::default();
    let setup = request(&cancel);
    let f = Fixture::new(&setup);
    let store = f.auth_store(&setup);
    put(&store, data("a", Some(KEY), false), true, &setup);
    put(&store, data("b", Some("b-key"), false), false, &setup);
    let issuer = local_issuer(&f, &setup, Arc::clone(&store));
    let op = request(&cancel);
    let runtime = issuer.authenticate_runtime(&f.secret(), &op).unwrap();
    let local = issuer.authenticate_local(KEY, &op).unwrap();
    let session = issuer.bootstrap_admin(&local).unwrap();
    let old = actor_bytes(&local.actor().unwrap());
    let mutation = request(&cancel);
    let snapshot = store.load(&mutation).unwrap();
    applied(
        store
            .mutate(snapshot, &mutation, |doc, ctx| {
                doc.activate("b", doc.revision(), ctx)?;
                doc.activate("a", doc.revision(), ctx)
            })
            .unwrap(),
    );
    let fresh = request(&cancel);
    let changed = issuer.authenticate_local(KEY, &fresh).unwrap();
    assert_ne!(actor_bytes(&changed.actor().unwrap()), old);
    assert!(issuer.admin_parent(&changed, &session).is_err());
    runtime.check().unwrap();
    let mutation = request(&cancel);
    let snapshot = store.load(&mutation).unwrap();
    applied(
        store
            .mutate(snapshot, &mutation, |doc, ctx| {
                doc.logout(Some("a"), doc.revision(), ctx)
            })
            .unwrap(),
    );
    assert!(issuer.authenticate_local(KEY, &request(&cancel)).is_err());
    runtime.check().unwrap();
    let mutation = request(&cancel);
    put(&store, data("a", Some(KEY), false), true, &mutation);
    let fresh = request(&cancel);
    let recreated = issuer.authenticate_local(KEY, &fresh).unwrap();
    assert!(issuer.admin_parent(&recreated, &session).is_err());
    assert_ne!(actor_bytes(&recreated.actor().unwrap()), old);
    runtime.check().unwrap();
}

#[test]
fn local_missing_empty_wrong_key_and_original_context_deny_without_fallback() {
    let cancel = Cancellation::default();
    let setup = request(&cancel);
    let f = Fixture::new(&setup);
    let store = f.auth_store(&setup);
    let issuer = local_issuer(&f, &setup, Arc::clone(&store));
    for key in [None, Some(KEY)] {
        let mutation = request(&cancel);
        put(&store, data("a", key, false), true, &mutation);
        let op = request(&cancel);
        assert!(issuer.authenticate_local("wrong", &op).is_err());
        assert!(issuer.authenticate_local("", &op).is_err());
        if key == Some(KEY) {
            let caller = issuer.authenticate_local(KEY, &op).unwrap();
            let other = request(&cancel);
            assert!(caller.source.check_physical(&issuer, &other).is_err());
        } else {
            assert!(issuer.authenticate_local(KEY, &op).is_err());
        }

        issuer.authenticate_runtime(&f.secret(), &op).unwrap();
    }
    let path = f.installation.owner_root().join("auth.json");
    let bytes = Zeroizing::new(std::fs::read_to_string(&path).unwrap());
    let malformed =
        Zeroizing::new(bytes.replace("\"mcp_key\":\"local-key-fixture\"", "\"mcp_key\":\"\""));
    assert_ne!(bytes.as_str(), malformed.as_str());
    std::fs::write(&path, malformed.as_bytes()).unwrap();
    let op = request(&cancel);
    assert!(issuer.authenticate_local(KEY, &op).is_err());
    issuer.authenticate_runtime(&f.secret(), &op).unwrap();
    let oversized = "x".repeat(limits().max_parent_bearer_bytes + 1);

    assert!(matches!(
        issuer.authenticate_local(&oversized, &op),
        Err(AuthError::Limit)
    ));
}

#[test]
fn local_proof_poison_fails_closed_without_blocking_fresh_observations() {
    let cancel = Cancellation::default();
    let setup = request(&cancel);
    let f = Fixture::new(&setup);
    let store = f.auth_store(&setup);
    put(&store, data("a", Some(KEY), false), true, &setup);
    let issuer = local_issuer(&f, &setup, store);
    let op = request(&cancel);
    let caller = issuer.authenticate_local(KEY, &op).unwrap();
    let SourceProof::Local(proof) = &caller.source else {
        panic!("local fixture")
    };
    let proof = Arc::clone(proof);
    assert!(
        std::thread::spawn(move || {
            let _guard = proof.observation.lock().unwrap();
            panic!("private local poison fixture")
        })
        .join()
        .is_err()
    );
    assert_eq!(caller.check(), Err(AuthError::State));
    let fresh = issuer.authenticate_local(KEY, &op).unwrap();
    fresh.check().unwrap();
    drop(caller);
    fresh.check().unwrap();
}

#[test]
fn local_finite_parent_expiry_does_not_expire_ordinary_source_or_create_session() {
    let cancel = Cancellation::default();
    let setup = request(&cancel);
    let f = Fixture::new(&setup);
    let store = f.auth_store(&setup);
    put(&store, data("a", Some(KEY), false), true, &setup);
    let clock = clock();
    let issuer = RoleIssuer::with_sources(
        f.realm(),
        Arc::clone(&f.runtime),
        store,
        authority(&all_permissions(), &all_permissions(), limits(), 60),
        ttl(),
        &setup,
        clock.clone(),
        Arc::new(OsEntropy),
    )
    .unwrap();
    let op = request(&cancel);
    let caller = issuer.authenticate_local(KEY, &op).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    clock.seconds.store(60, Ordering::SeqCst);
    caller.authorize(Permission::InspectGit).unwrap();
    assert!(issuer.admin_parent(&caller, &session).is_err());
    let before = issuer.lock().unwrap().sessions.len();
    caller.actor().unwrap();
    assert_eq!(issuer.lock().unwrap().sessions.len(), before);
}

#[test]
fn foreign_real_store_and_broker_profile_cannot_supply_local_authority() {
    let cancel = Cancellation::default();
    let setup = request_for_documents(&cancel);
    let f = Fixture::new(&setup);
    let other = Fixture::new(&setup);
    let foreign = other.auth_store(&setup);
    let policy = authority(&[], &[], limits(), 60);
    assert!(matches!(
        RoleIssuer::new(
            f.realm(),
            Arc::clone(&f.runtime),
            foreign,
            policy,
            ttl(),
            &setup
        ),
        Err(AuthError::Denied)
    ));
    let store = f.auth_store(&setup);
    put(&store, data("a", Some(KEY), false), true, &setup);
    let issuer = local_issuer(&f, &setup, Arc::clone(&store));
    let op = request(&cancel);
    let local = issuer.authenticate_local(KEY, &op).unwrap();
    let session = issuer.bootstrap_admin(&local).unwrap();
    let change = request(&cancel);
    let snapshot = store.load(&change).unwrap();
    applied(
        store
            .mutate(snapshot, &change, |doc, ctx| {
                let service = crate::profile::BrokerService {
                    endpoint: "https://broker.example".into(),
                    mcp_resource: None,
                    allow_anonymous: false,
                };
                let mut remote = config();
                remote.kind = ProviderKind::Broker(service);
                remote.s3.source = S3Source::BrokerScope;
                doc.add_provider("remote".into(), remote, doc.revision(), 1, ctx)?;
                let profile = ProfileData {
                    name: "remote".into(),
                    provider: "remote".into(),
                    username: "admin".into(),
                    organization: Some("admin".into()),
                    teams: vec!["admins".into()],
                    credentials: ProfileCredentials::Broker {
                        issuer: "https://broker.example".into(),
                        subject: "subject".into(),
                        tokens: crate::profile::OidcTokens {
                            access_token: Secret::new(SecretString::from(KEY)),
                            refresh_token: None,
                            id_token: Secret::new(SecretString::from("id")),
                            expires_at: 100,
                        },
                        s3_disabled: true,
                    },
                };
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("remote").unwrap().revision(),
                    profile_revision: None,
                };
                doc.replace_profile_data(profile, expected, 1, ctx)?;
                doc.activate("remote", doc.revision(), ctx)
            })
            .unwrap(),
    );
    let fresh = request(&cancel);
    assert!(issuer.authenticate_local(KEY, &fresh).is_err());
    assert!(!issuer.lock().unwrap().sessions.contains_key(&session.id));
    issuer.authenticate_runtime(&f.secret(), &fresh).unwrap();
}

#[cfg(unix)]
#[test]
fn local_private_file_symlink_hardlink_and_mode_changes_are_not_repaired() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for attack in 0..3 {
        let cancel = Cancellation::default();
        let setup = request(&cancel);
        let f = Fixture::new(&setup);
        let store = f.auth_store(&setup);
        put(&store, data("a", Some(KEY), false), true, &setup);
        let issuer = local_issuer(&f, &setup, store);
        let path = f.installation.owner_root().join("auth.json");
        let op = request(&cancel);
        let caller = issuer.authenticate_local(KEY, &op).unwrap();
        match attack {
            0 => std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap(),
            1 => {
                std::fs::hard_link(&path, path.with_file_name("foreign-hardlink")).unwrap();
            }
            _ => {
                let old = path.with_file_name("old-auth");
                std::fs::rename(&path, &old).unwrap();
                symlink(old, &path).unwrap();
            }
        }
        assert!(caller.check().is_err());
        let fresh = request(&cancel);
        assert!(issuer.authenticate_local(KEY, &fresh).is_err());

        if attack == 0 {
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o644
            );
        }
    }
}

#[test]
fn smaller_profile_parse_cap_rejects_full_body() {
    let cancel = Cancellation::default();
    let setup = request(&cancel);
    let f = Fixture::new(&setup);
    let store = f.auth_store(&setup);
    put(&store, data("a", Some(KEY), false), true, &setup);

    let small = f.auth_store_policy(
        &setup,
        crate::profile::ProfileLimits {
            bytes: 128,
            nodes: 4096,
            scalar_bytes: 256,
            collection_entries: 64,
            providers: 16,
            profiles: 16,
        },
    );
    let issuer = local_issuer(&f, &setup, small);
    let op = request(&cancel);
    assert!(issuer.authenticate_local(KEY, &op).is_err());

    assert!(issuer.authenticate_local(KEY, &op).is_err());
}

#[cfg(unix)]
#[test]
fn local_equal_body_replacement_during_scope_inspection_does_not_restore_proof() {
    use std::io::Write;
    let cancel = Cancellation::default();
    let setup = request(&cancel);
    let f = Fixture::new(&setup);
    let store = f.auth_store(&setup);
    put(&store, data("a", Some(KEY), false), true, &setup);
    let issuer = local_issuer(&f, &setup, store);
    let op = request_for_documents(&cancel);
    let caller = issuer.authenticate_local(KEY, &op).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let permission = Permission::InspectGit;
    let def = f.definition(Role::Coordinator, &[permission], 1, limits());
    let reference = def.reference().clone();
    issuer.register_definition(&parent, def).unwrap();
    let run = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(60))
        .unwrap();
    let token = issuer
        .issue(&parent, &run, &reference, &[permission], &f.scope())
        .unwrap();
    let path = f.installation.owner_root().join("auth.json");
    let original = Zeroizing::new(std::fs::read(&path).unwrap());
    let bytes = Zeroizing::new(original.to_vec());
    let retained = path.with_file_name("retained-auth");
    let replacement = path.clone();
    BEFORE_FINAL_SOURCE.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            std::fs::rename(&replacement, &retained).unwrap();
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&replacement)
                .unwrap();
            file.write_all(bytes.as_slice()).unwrap();
            file.sync_all().unwrap();
        }))
    });
    assert!(
        issuer
            .verify(token.expose_for_transport(), &reference, f.scope(), &op)
            .is_err()
    );
    assert_eq!(std::fs::read(path).unwrap(), original.as_slice());
}

#[cfg(unix)]
#[test]
fn local_last_entropy_rechecks_original_physical_source_outside_state() {
    use std::os::unix::fs::PermissionsExt;
    struct HookEntropy {
        entropy: TestEntropy,
        hook: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    }
    impl Entropy for HookEntropy {
        fn fill(&self, bytes: &mut [u8]) -> Result<(), AuthError> {
            self.entropy.fill(bytes)?;
            if let Some(hook) = self.hook.lock().unwrap().take() {
                hook();
            }
            Ok(())
        }
    }
    let cancel = Cancellation::default();
    let setup = request(&cancel);
    let f = Fixture::new(&setup);
    let store = f.auth_store(&setup);
    put(&store, data("a", Some(KEY), false), true, &setup);
    let entropy = Arc::new(HookEntropy {
        entropy: TestEntropy {
            fail: AtomicBool::new(false),
            counter: AtomicU64::new(0),
        },
        hook: Mutex::new(None),
    });
    let issuer = Arc::new(
        RoleIssuer::with_sources(
            f.realm(),
            Arc::clone(&f.runtime),
            store,
            authority(&all_permissions(), &all_permissions(), limits(), 7200),
            ttl(),
            &setup,
            Arc::new(SystemClock),
            entropy.clone(),
        )
        .unwrap(),
    );
    let op = request(&cancel);
    let caller = issuer.authenticate_local(KEY, &op).unwrap();
    let weak = Arc::downgrade(&issuer);
    let path = f.installation.owner_root().join("auth.json");
    *entropy.hook.lock().unwrap() = Some(Box::new(move || {
        let issuer = weak.upgrade().unwrap();
        assert!(
            issuer.state.try_lock().is_ok(),
            "entropy must run outside State"
        );
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o400)).unwrap();
    }));
    assert!(issuer.bootstrap_admin(&caller).is_err());
    assert!(issuer.lock().unwrap().sessions.is_empty());
}

#[test]
fn older_qualified_document_cannot_roll_back_local_watermark_or_session() {
    let cancel = Cancellation::default();
    let setup = request(&cancel);
    let f = Fixture::new(&setup);
    let store = f.auth_store(&setup);
    put(&store, data("a", Some(KEY), false), true, &setup);
    let path = f.installation.owner_root().join("auth.json");
    let old = Zeroizing::new(std::fs::read(&path).unwrap());
    let issuer = local_issuer(&f, &setup, Arc::clone(&store));
    let first = request(&cancel);
    let caller = issuer.authenticate_local(KEY, &first).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let change = request(&cancel);
    put(&store, data("b", Some("b-key"), false), false, &change);
    let fresh = request(&cancel);
    let current = issuer.authenticate_local(KEY, &fresh).unwrap();
    let before = issuer.lock().unwrap().local.watermark;
    let generation = current.source.generation();
    std::fs::write(path, old.as_slice()).unwrap();
    let rollback = request(&cancel);
    assert!(issuer.authenticate_local(KEY, &rollback).is_err());
    let state = issuer.lock().unwrap();
    assert_eq!(state.local.watermark, before);
    assert_eq!(state.local.generation, generation);
    assert!(state.sessions.contains_key(&session.id));
}

#[test]
fn local_identity_changes_and_missing_key_revoke_current_lineage() {
    let cancel = Cancellation::default();
    let setup = request(&cancel);
    let f = Fixture::new(&setup);
    let store = f.auth_store(&setup);
    put(&store, data("a", Some(KEY), false), true, &setup);
    let issuer = local_issuer(&f, &setup, Arc::clone(&store));
    let first = request(&cancel);
    let caller = issuer.authenticate_local(KEY, &first).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let old = actor_bytes(&caller.actor().unwrap());
    for profile in [
        {
            let mut p = data("a", Some(KEY), false);
            p.organization = Some("org".into());
            p
        },
        {
            let mut p = data("a", Some(KEY), false);
            p.teams = vec!["one".into(), "two".into()];
            p
        },
        {
            let mut p = data("a", Some(KEY), false);
            p.teams = vec!["two".into(), "one".into()];
            p
        },
        data("a", Some(KEY), true),
    ] {
        let change = request(&cancel);
        put(&store, profile, true, &change);
        let next = request(&cancel);
        let fresh = issuer.authenticate_local(KEY, &next).unwrap();
        assert_ne!(actor_bytes(&fresh.actor().unwrap()), old);
        assert!(issuer.admin_parent(&fresh, &session).is_err());
    }
    std::fs::remove_file(f.installation.owner_root().join("auth.json")).unwrap();
    let missing = request(&cancel);
    assert!(issuer.authenticate_local(KEY, &missing).is_err());
    assert!(issuer.lock().unwrap().sessions.is_empty());
    issuer.authenticate_runtime(&f.secret(), &missing).unwrap();
}

#[test]
fn malformed_local_body_releases_observation() {
    let cancel = Cancellation::default();
    let setup = request(&cancel);
    let f = Fixture::new(&setup);
    let store = f.auth_store(&setup);
    put(&store, data("a", Some(KEY), false), true, &setup);
    let issuer = local_issuer(&f, &setup, store);
    let path = f.installation.owner_root().join("auth.json");
    let bytes = Zeroizing::new(std::fs::read(&path).unwrap());
    std::fs::write(&path, b"not-json").unwrap();
    let op = request(&cancel);

    assert!(issuer.authenticate_local(KEY, &op).is_err());

    std::fs::write(path, bytes.as_slice()).unwrap();
    let fresh = request(&cancel);
    issuer.authenticate_local(KEY, &fresh).unwrap();
}

#[cfg(unix)]
#[test]
fn local_admission_rechecks_original_file_after_final_installation_inspection() {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let cancel = Cancellation::default();
    let setup = request(&cancel);
    let f = Fixture::new(&setup);
    let store = f.auth_store(&setup);
    put(&store, data("a", Some(KEY), false), true, &setup);
    let issuer = local_issuer(&f, &setup, store);
    let path = f.installation.owner_root().join("auth.json");
    let original = Zeroizing::new(std::fs::read(&path).unwrap());
    let replacement = path.clone();
    let bytes = Zeroizing::new(original.to_vec());
    super::super::source::AFTER_LOCAL_ADMISSION_INSPECTION.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            let retained = replacement.with_file_name("retained-before-final-admission");
            std::fs::rename(&replacement, retained).unwrap();
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(replacement)
                .unwrap();
            file.write_all(bytes.as_slice()).unwrap();
            file.sync_all().unwrap();
        }));
    });
    let op = request(&cancel);
    assert!(issuer.authenticate_local(KEY, &op).is_err());
    let state = issuer.lock().unwrap();
    assert_eq!(state.local.watermark, 0);
    assert_eq!(state.local.generation, 0);
    assert!(state.local.binding.is_none());
    drop(state);
    assert_eq!(std::fs::read(path).unwrap(), original.as_slice());

    let fresh = request(&cancel);
    issuer
        .authenticate_local(KEY, &fresh)
        .unwrap()
        .actor()
        .unwrap();
}

#[test]
fn cancelled_or_expired_state_wait_cannot_observe_source_changes() {
    for runtime in [true, false] {
        for expire in [false, true] {
            let setup_cancel = Cancellation::default();
            let setup = request(&setup_cancel);
            let f = Fixture::new(&setup);
            let store = f.auth_store(&setup);
            put(&store, data("active", Some(KEY), false), true, &setup);
            let issuer = local_issuer(&f, &setup, store);
            if runtime {
                std::fs::remove_file(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
                assert_eq!(f.runtime.maintain(setup.snapshot()).failure(), None);
            }
            let runtime_secret = f.secret();
            let cancellation = Cancellation::default();
            let deadline = Instant::now()
                + if expire {
                    Duration::from_millis(250)
                } else {
                    Duration::from_secs(30)
                };
            let ctx = request_before(&cancellation, deadline);
            let guard = issuer.lock().unwrap();
            let before = (
                guard.runtime_generation,
                guard.local.generation,
                guard.local.watermark,
            );
            std::thread::scope(|threads| {
                let (ready, waiting) = std::sync::mpsc::channel();
                let issuer = &issuer;
                let ctx = &ctx;
                let secret = &runtime_secret;
                let checked = threads.spawn(move || {
                    super::super::source::BEFORE_SOURCE_STATE_LOCK.with(|slot| {
                        *slot.borrow_mut() = Some(Box::new(move || {
                            assert!(Instant::now() < deadline);
                            ready.send(()).unwrap();
                        }));
                    });
                    if runtime {
                        issuer.authenticate_runtime(secret, ctx).err()
                    } else {
                        issuer.authenticate_local(KEY, ctx).err()
                    }
                });
                waiting.recv().unwrap();
                if expire {
                    while Instant::now() < deadline {
                        std::thread::yield_now();
                    }
                } else {
                    cancellation.cancel();
                }
                drop(guard);
                assert_eq!(checked.join().unwrap(), Some(AuthError::Request));
            });
            let state = issuer.lock().unwrap();
            assert_eq!(
                (
                    state.runtime_generation,
                    state.local.generation,
                    state.local.watermark
                ),
                before,
            );
        }
    }
}

#[test]
fn concurrent_local_calls_wait_and_preserve_original_and_generation() {
    let setup_cancel = Cancellation::default();
    let setup = request_for_documents(&setup_cancel);
    let f = Fixture::new(&setup);
    let store = f.auth_store(&setup);
    put(&store, data("a", Some(KEY), false), true, &setup);
    let issuer = local_issuer(&f, &setup, store);
    let caller = issuer.authenticate_local(KEY, &setup).unwrap();
    let barrier = std::sync::Barrier::new(16);
    std::thread::scope(|threads| {
        let workers: Vec<_> = (0..16)
            .map(|_| {
                let (caller, barrier) = (&caller, &barrier);
                threads.spawn(move || {
                    barrier.wait();
                    caller.authorize(Permission::InspectGit)
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap().unwrap();
        }
    });
    for expired in [false, true] {
        let cancel = Cancellation::default();
        let deadline = Instant::now()
            + if expired {
                Duration::from_millis(200)
            } else {
                Duration::from_secs(30)
            };
        let original = request_before(&cancel, deadline);
        let caller = issuer.authenticate_local(KEY, &original).unwrap();
        let SourceProof::Local(proof) = &caller.source else {
            panic!("expected actual Local proof");
        };
        let before = {
            let state = issuer.lock().unwrap();
            (state.local.watermark, state.local.generation)
        };
        let guard = proof.observation.lock().unwrap();
        std::thread::scope(|threads| {
            let (entered, waiting) = std::sync::mpsc::channel();
            let caller = &caller;
            let worker = threads.spawn(move || {
                source::BEFORE_LOCAL_OBSERVATION_LOCK.with(|slot| {
                    *slot.borrow_mut() = Some(Box::new(move || entered.send(()).unwrap()));
                });
                caller.authorize(Permission::InspectGit)
            });
            waiting.recv().unwrap();
            if expired {
                std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
            } else {
                cancel.cancel();
            }
            drop(guard);
            assert_eq!(worker.join().unwrap(), Err(AuthError::Request));
        });
        let state = issuer.lock().unwrap();
        assert_eq!((state.local.watermark, state.local.generation), before);
    }
}

#[test]
fn trusted_roots_reject_local_even_with_configure_permission() {
    let cancellation = Cancellation::default();
    let setup = request(&cancellation);
    let f = Fixture::new(&setup);
    let store = f.auth_store(&setup);
    put(&store, data("native-local", Some(KEY), false), true, &setup);
    let issuer = RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        store,
        authority(&all_permissions(), &all_permissions(), limits(), 7200),
        ttl(),
        &setup,
    )
    .unwrap();
    let original = request(&cancellation);
    let caller = issuer.authenticate_local(KEY, &original).unwrap();
    caller.authorize(Permission::Configure).unwrap();
    let capture = ScopeCapture {
        live: &f.project,
        record: &f.project_record,
    };
    assert_eq!(
        issuer.trust_project_root(&caller, &original, capture),
        Err(AuthError::Denied)
    );
}

#[test]
fn task_principal_tracks_authenticated_profile_without_credential_epoch() {
    use crate::{ProjectRecordAuthority, ProjectWriteOperation};
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let store = f.auth_store(&ctx);
    put(&store, data("participant-a", Some(KEY), false), true, &ctx);
    let make_issuer = || {
        RoleIssuer::new(
            f.realm(),
            Arc::clone(&f.runtime),
            Arc::clone(&store),
            authority(&all_permissions(), &all_permissions(), limits(), 7200),
            ttl(),
            &ctx,
        )
        .unwrap()
    };
    let first = make_issuer();
    let caller = first.authenticate_local(KEY, &ctx).unwrap();
    let first_authority = ProjectRecordAuthority::new(&first);
    let admission = first_authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Task)
        .unwrap();
    let principal = actor_bytes(&admission.task_actor().unwrap());
    let ordinary_actor = actor_bytes(&admission.actor().unwrap());
    let second = make_issuer();
    let new_caller = second.authenticate_local(KEY, &ctx).unwrap();
    let second_authority = ProjectRecordAuthority::new(&second);
    let new_admission = second_authority
        .admit_authenticated_write(&new_caller, &ctx, f.scope(), ProjectWriteOperation::Task)
        .unwrap();
    assert_eq!(principal, actor_bytes(&new_admission.task_actor().unwrap()));
    assert_ne!(ordinary_actor, actor_bytes(&new_admission.actor().unwrap()));
    let runtime = second.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let runtime_admission = second_authority
        .admit_authenticated_write(&runtime, &ctx, f.scope(), ProjectWriteOperation::Task)
        .unwrap();
    assert_ne!(
        principal,
        actor_bytes(&runtime_admission.task_actor().unwrap())
    );

    // Rotate the actual temporary profile's credential: old admission is revoked,
    // while a newly authenticated caller retains that canonical participant.
    put(
        &store,
        data("participant-a", Some("replacement-fixture-key"), false),
        true,
        &ctx,
    );
    assert!(admission.task_actor().is_err());
    assert!(new_admission.task_actor().is_err());
    assert!(second.authenticate_local(KEY, &ctx).is_err());
    let restarted = make_issuer();
    assert!(restarted.authenticate_local(KEY, &ctx).is_err());
    let restarted_authority = ProjectRecordAuthority::new(&restarted);
    let rotated = restarted
        .authenticate_local("replacement-fixture-key", &ctx)
        .unwrap();
    let rotated_admission = restarted_authority
        .admit_authenticated_write(&rotated, &ctx, f.scope(), ProjectWriteOperation::Task)
        .unwrap();
    assert_eq!(
        principal,
        actor_bytes(&rotated_admission.task_actor().unwrap())
    );
    put(
        &store,
        data("participant-b", Some("other-fixture-key"), false),
        true,
        &ctx,
    );
    assert!(rotated_admission.task_actor().is_err());
    let other = restarted
        .authenticate_local("other-fixture-key", &ctx)
        .unwrap();
    let other_admission = restarted_authority
        .admit_authenticated_write(&other, &ctx, f.scope(), ProjectWriteOperation::Task)
        .unwrap();
    assert_ne!(
        principal,
        actor_bytes(&other_admission.task_actor().unwrap())
    );
}
