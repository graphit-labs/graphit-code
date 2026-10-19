use super::*;

fn split_issuer(
    fixture: &Fixture,
    store: Arc<AuthStore>,
    original: &RequestContext<'_>,
    ordinary: &[Permission],
    administrative: &[Permission],
) -> RoleIssuer {
    RoleIssuer::new(
        fixture.realm(),
        Arc::clone(&fixture.runtime),
        store,
        authority(ordinary, administrative, limits(), 7200),
        ttl(),
        original,
    )
    .unwrap()
}

fn populate_native(store: &AuthStore, server: &Server, original: &RequestContext<'_>, expiry: u64) {
    put(
        store,
        broker_config(&server.origin, true, false),
        native_profile(&server.origin, &server.token(true, expiry), expiry),
        true,
        original,
    );
}

fn verifier(
    server: &Server,
    issuer: &RoleIssuer,
    original: &RequestContext<'_>,
) -> crate::broker::BrokerVerifier {
    let revision = issuer
        .local
        .load(original)
        .unwrap()
        .document()
        .provider("service")
        .unwrap()
        .revision();
    server.verifier(false, revision)
}

#[tokio::test]
async fn native_parent_fenced_write_keeps_split_authority_and_role_ceiling() {
    let server = Server::new().await;
    server.userinfo_status.store(503, Ordering::SeqCst);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = profile_store(&f, &ctx);
    populate_native(&store, &server, &ctx, now_seconds() + 600);
    let issuer = make_issuer(&f, store, &ctx, &[Permission::MutateUser], false);
    let verifier = verifier(&server, &issuer, &ctx);
    let caller = issuer
        .authenticate_runtime(&f.secret(), &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
    let capture = ScopeCapture {
        live: &scope,
        record: &record,
    };
    assert_eq!(
        caller.authorize(Permission::WriteFencedMemory),
        Err(AuthError::Denied)
    );
    assert!(
        issuer
            .admit_authenticated_user_memory_write(
                &caller,
                &ctx,
                capture,
                UserMemoryWriteOperation::FencedMemory,
            )
            .is_err()
    );
    let before = (
        server.count("/.well-known/graphit-broker"),
        server.count("/.well-known/openid-configuration"),
        server.count("/keys"),
    );
    let write = parent
        .admit_native_user_memory_fenced_write(&ctx, capture)
        .unwrap();
    for _ in 0..2 {
        assert_eq!(write.recheck(), Ok(Obligation::MemoryFenceRequired));
        assert_eq!(
            write.operation().unwrap(),
            UserMemoryWriteOperation::FencedMemory
        );
        assert_eq!(write.identity().unwrap().issuer(), server.origin);
        assert_eq!(write.identity().unwrap().subject(), USER_SUB);
        assert!(std::ptr::eq(write.request().unwrap(), &ctx));
        assert_eq!(write.user().unwrap().live.owner(), scope.owner());
        assert!(write.actor().unwrap() == caller.actor().unwrap());
    }
    assert_eq!(
        caller.authorize(Permission::WriteFencedMemory),
        Err(AuthError::Denied)
    );
    let definition = f.definition(Role::Dream, &[Permission::WriteFencedMemory], 1, limits());
    let reference = definition.reference().clone();
    issuer.register_definition(&parent, definition).unwrap();
    let run = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(60))
        .unwrap();
    let bearer = issuer
        .issue(
            &parent,
            &run,
            &reference,
            &[Permission::WriteFencedMemory],
            &f.scope(),
        )
        .unwrap();
    let role = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    assert_eq!(
        role.authorize(Permission::WriteFencedMemory),
        Ok(Obligation::MemoryFenceRequired)
    );
    assert_eq!(role.authorize(Permission::ReadUser), Err(AuthError::Denied));
    assert_eq!(
        role.authorize(Permission::MutateUser),
        Err(AuthError::Denied)
    );
    assert!(!scope.runtime_root().exists());
    assert_eq!(
        before,
        (
            server.count("/.well-known/graphit-broker"),
            server.count("/.well-known/openid-configuration"),
            server.count("/keys"),
        )
    );
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
}

#[tokio::test]
async fn native_parent_fenced_write_requires_both_policy_halves() {
    let server = Server::new().await;
    for (ordinary, administrative) in [
        (vec![], vec![]),
        (vec![Permission::MutateUser], vec![]),
        (vec![], vec![Permission::WriteFencedMemory]),
        (
            vec![Permission::WriteFencedMemory],
            vec![Permission::MutateUser],
        ),
        (vec![Permission::MutateUser], vec![Permission::WriteMemory]),
    ] {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = profile_store(&f, &ctx);
        populate_native(&store, &server, &ctx, now_seconds() + 600);
        let issuer = split_issuer(&f, store, &ctx, &ordinary, &administrative);
        let verifier = verifier(&server, &issuer, &ctx);
        let caller = issuer
            .authenticate_runtime(&f.secret(), &ctx)
            .unwrap()
            .authenticate_native(&verifier)
            .await
            .unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
        assert!(matches!(
            parent.admit_native_user_memory_fenced_write(
                &ctx,
                ScopeCapture {
                    live: &scope,
                    record: &record
                },
            ),
            Err(AuthError::Denied)
        ));
        assert!(!scope.runtime_root().exists());
    }
}

#[tokio::test]
async fn native_parent_fenced_write_rejects_missing_native_and_local_origins() {
    use super::super::service_credentials::local_profile;
    let server = Server::new().await;
    for local in [false, true] {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = profile_store(&f, &ctx);
        if local {
            put(
                &store,
                broker_config(&server.origin, false, true),
                local_profile("own-local", "own-local-key", None, false),
                true,
                &ctx,
            );
        }
        let issuer = split_issuer(&f, store, &ctx, &ALL, &ALL);
        let caller = if local {
            issuer.authenticate_local("own-local-key", &ctx).unwrap()
        } else {
            issuer.authenticate_runtime(&f.secret(), &ctx).unwrap()
        };
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
        assert!(matches!(
            parent.admit_native_user_memory_fenced_write(
                &ctx,
                ScopeCapture {
                    live: &scope,
                    record: &record
                },
            ),
            Err(AuthError::Denied)
        ));
        assert!(!scope.runtime_root().exists());
    }
}

#[tokio::test]
async fn native_parent_fenced_write_rejects_foreign_original_identity_and_scope() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = profile_store(&f, &ctx);
    populate_native(&store, &server, &ctx, now_seconds() + 600);
    let issuer = split_issuer(
        &f,
        Arc::clone(&store),
        &ctx,
        &[Permission::MutateUser],
        &[Permission::WriteFencedMemory],
    );
    let verifier = verifier(&server, &issuer, &ctx);
    let caller = issuer
        .authenticate_runtime(&f.secret(), &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
    let fresh = context(&cancel);
    assert!(matches!(
        parent.admit_native_user_memory_fenced_write(
            &fresh,
            ScopeCapture {
                live: &scope,
                record: &record
            },
        ),
        Err(AuthError::Denied)
    ));
    let foreign_issuer = split_issuer(&f, store, &ctx, &ALL, &ALL);
    assert!(matches!(
        foreign_issuer.admin_parent(&caller, &session),
        Err(AuthError::Denied)
    ));
    let foreign = Fixture::new(&ctx);
    for (bad, binding) in [
        user(&f, &ctx, &server.origin, "another-user"),
        user(&f, &ctx, "https://another-issuer.example", USER_SUB),
        user(&foreign, &ctx, &server.origin, USER_SUB),
    ] {
        assert!(matches!(
            parent.admit_native_user_memory_fenced_write(
                &ctx,
                ScopeCapture {
                    live: &bad,
                    record: &binding
                },
            ),
            Err(AuthError::Denied)
        ));
        assert!(!bad.runtime_root().exists());
    }
    for bad in [
        f.realm(),
        ScopeCapture {
            live: &f.project,
            record: &f.project_record,
        },
        ScopeCapture {
            live: &scope,
            record: &f.project_record,
        },
    ] {
        assert!(matches!(
            parent.admit_native_user_memory_fenced_write(&ctx, bad),
            Err(AuthError::Denied)
        ));
    }
    assert!(!scope.runtime_root().exists());
}

#[tokio::test]
async fn native_parent_fenced_write_rechecks_parent_session_and_profile() {
    let server = Server::new().await;
    for ending in ["removed_parent", "expired_parent", "logout"] {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = profile_store(&f, &ctx);
        populate_native(&store, &server, &ctx, now_seconds() + 600);
        let clock = Arc::new(TestClock {
            start: Moment {
                utc: SystemTime::now(),
                mono: Instant::now(),
            },
            seconds: AtomicU64::new(0),
            rollback: AtomicBool::new(false),
        });
        let issuer = RoleIssuer::with_sources(
            f.realm(),
            Arc::clone(&f.runtime),
            Arc::clone(&store),
            authority(
                &[Permission::MutateUser],
                &[Permission::WriteFencedMemory],
                limits(),
                7200,
            ),
            ttl(),
            &ctx,
            clock.clone(),
            Arc::new(OsEntropy),
        )
        .unwrap();
        let verifier = verifier(&server, &issuer, &ctx);
        let caller = issuer
            .authenticate_runtime(&f.secret(), &ctx)
            .unwrap()
            .authenticate_native(&verifier)
            .await
            .unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
        let write = parent
            .admit_native_user_memory_fenced_write(
                &ctx,
                ScopeCapture {
                    live: &scope,
                    record: &record,
                },
            )
            .unwrap();
        match ending {
            "removed_parent" => {
                // Exercise the actual session table retained by this Parent,
                // without introducing a test-only public revocation API.
                assert!(
                    issuer
                        .lock()
                        .unwrap()
                        .sessions
                        .remove(&session.id)
                        .is_some()
                );
                assert_eq!(write.recheck(), Err(AuthError::Denied));
            }
            "expired_parent" => {
                clock.seconds.store(7200, Ordering::SeqCst);
                assert_eq!(write.recheck(), Err(AuthError::Expired));
            }
            "logout" => {
                let publication = store
                    .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                        doc.logout(None, doc.revision(), ctx)
                    })
                    .unwrap();
                assert!(publication.failure().is_none());
                assert!(write.recheck().is_err());
            }
            _ => unreachable!(),
        }
        assert!(write.actor().is_err());
        assert!(write.user().is_err());
        assert!(!scope.runtime_root().exists());
    }
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
}

#[tokio::test]
async fn native_parent_fenced_write_rechecks_physical_user_capture() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = profile_store(&f, &ctx);
    populate_native(&store, &server, &ctx, now_seconds() + 600);
    let issuer = split_issuer(
        &f,
        store,
        &ctx,
        &[Permission::MutateUser],
        &[Permission::WriteFencedMemory],
    );
    let verifier = verifier(&server, &issuer, &ctx);
    let caller = issuer
        .authenticate_runtime(&f.secret(), &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (initial, _) = user(&f, &ctx, &server.origin, USER_SUB);
    std::fs::create_dir_all(initial.runtime_root()).unwrap();
    let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
    let write = parent
        .admit_native_user_memory_fenced_write(
            &ctx,
            ScopeCapture {
                live: &scope,
                record: &record,
            },
        )
        .unwrap();
    std::fs::rename(
        scope.runtime_root(),
        scope.runtime_root().with_extension("retired"),
    )
    .unwrap();
    std::fs::create_dir(scope.runtime_root()).unwrap();
    assert!(write.recheck().is_err());
    assert!(write.user().is_err());
}

#[tokio::test]
async fn native_parent_fenced_write_keeps_cancel_deadline_and_signed_expiry() {
    use std::time::UNIX_EPOCH;
    let server = Server::new().await;
    server.userinfo_status.store(503, Ordering::SeqCst);
    for ending in ["cancel", "deadline", "signed_expiry"] {
        let cancel = Cancellation::default();
        let deadline = Instant::now() + Duration::from_secs(2);
        let ctx = if ending == "deadline" {
            request_before(&cancel, deadline)
        } else {
            context(&cancel)
        };
        let f = Fixture::new(&ctx);
        let store = profile_store(&f, &ctx);
        let expiry = now_seconds() + if ending == "signed_expiry" { 2 } else { 600 };
        populate_native(&store, &server, &ctx, expiry);
        let issuer = split_issuer(
            &f,
            store,
            &ctx,
            &[Permission::MutateUser],
            &[Permission::WriteFencedMemory],
        );
        let verifier = verifier(&server, &issuer, &ctx);
        let caller = issuer
            .authenticate_runtime(&f.secret(), &ctx)
            .unwrap()
            .authenticate_native(&verifier)
            .await
            .unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let (scope, record) = user(&f, &ctx, &server.origin, USER_SUB);
        let write = parent
            .admit_native_user_memory_fenced_write(
                &ctx,
                ScopeCapture {
                    live: &scope,
                    record: &record,
                },
            )
            .unwrap();
        match ending {
            "cancel" => cancel.cancel(),
            "deadline" => tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await,
            "signed_expiry" => {
                tokio::time::sleep(
                    UNIX_EPOCH
                        .checked_add(Duration::from_secs(expiry))
                        .unwrap()
                        .duration_since(SystemTime::now())
                        .unwrap_or_default()
                        + Duration::from_millis(10),
                )
                .await
            }
            _ => unreachable!(),
        }
        assert_eq!(
            write.recheck(),
            Err(if ending == "signed_expiry" {
                AuthError::Expired
            } else {
                AuthError::Request
            })
        );
        assert!(write.actor().is_err());
        assert!(!scope.runtime_root().exists());
    }
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
}
