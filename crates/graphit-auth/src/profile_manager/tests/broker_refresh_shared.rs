//! Independent consumers exclude one-use refreshes and retain durable uncertainty.
use super::*;
use crate::broker::oauth::NativeErrorKind;
use crate::profile::ProfileError;
use graphit_core::auth_document::AuthDocumentError;

#[tokio::test]
async fn distinct_managers_share_rotation_and_stale_loser_rereads_without_post() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = BrokerFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let provider_revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
    let verifier = broker.verifier(provider_revision, broker.origin(), None);
    let first_flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let second_flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let response = broker.response(false, 300, "native-subject", Some("rotated-refresh"));
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let first = manager(&issuer);
    let second = manager(&issuer);
    let first_snapshot = begin(&first, &parent);
    let second_snapshot = begin(&second, &parent);
    let old_revision = first_snapshot.revision();
    assert_eq!(second_snapshot.revision(), old_revision);
    let prior = issuer.profile_store().load(&ctx).unwrap();
    let prior_profile = prior.document().profile("a").unwrap();
    let old_profile = prior_profile.revision();
    let old_token = prior_profile.token_revision();
    drop(prior);
    let gate = broker.gate("/token");
    let mut winner = Box::pin(first.refresh_broker(first_snapshot, &first_flow, 2));
    tokio::select! {
        result = &mut winner => panic!("rotation returned before the real HTTP barrier: {:?}", result.err()),
        () = broker.wait("/token") => {}
    }
    let mut loser = Box::pin(second.refresh_broker(second_snapshot, &second_flow, 2));
    tokio::select! {
        biased;
        result = &mut loser => panic!("same-epoch contender did not wait for rotation: {:?}", result.err()),
        () = tokio::task::yield_now() => {}
    }
    // A faulty second HTTP request must finish and fail the count assertion,
    // rather than hide behind an unreleased fixture response barrier.
    broker.state.gates.lock().unwrap().remove("/token");
    gate.notify_waiters();
    let (winner, loser) = tokio::join!(winner, loser);
    applied(winner.unwrap());
    let loser = loser.expect_err("same-epoch snapshot must be stale");
    assert_eq!(loser.remote_effect(), RemoteEffect::NotSent);
    assert!(matches!(
        loser.kind(),
        BrokerRefreshErrorKind::Manager(ProfileManagerError::Store(
            AuthStoreError::Core(AuthDocumentError::Conflict)
                | AuthStoreError::Profile(ProfileError::Stale | ProfileError::Conflict)
        ))
    ));
    assert_eq!(broker.count("/token"), 1);
    let fresh = begin(&second, &parent);
    assert_eq!(fresh.revision(), old_revision + 2);
    assert_eq!(fresh.active_name(), Some("a"));
    let selected = fresh
        .profiles()
        .find(|profile| profile.name() == "a")
        .unwrap();
    assert_eq!(selected.token_revision(), old_token + 1);
    assert_eq!(selected.revision(), old_profile + 2);
    assert_eq!(selected.provider_revision(), provider_revision);
    assert_eq!(
        selected.broker_identity(),
        Some((broker.origin(), "native-subject"))
    );
    let current = issuer.profile_store().load(&ctx).unwrap();
    let ProfileCredentials::Broker { tokens, .. } = &current
        .document()
        .profile("a")
        .unwrap()
        .data()
        .unwrap()
        .credentials
    else {
        panic!("retained Broker credential")
    };
    assert_eq!(
        tokens.access_token.expose(),
        response["access_token"].as_str().unwrap()
    );
    assert_eq!(
        tokens.refresh_token.as_ref().unwrap().expose(),
        "rotated-refresh"
    );
    assert!(tokens.expires_at > 0);
    assert_eq!(
        broker.count("/token"),
        1,
        "rereading the winner performs no rotation"
    );
    assert_eq!(broker.count("/userinfo"), 0);
    let forms = broker.state.forms.lock().unwrap();
    assert_eq!(forms.len(), 1);
    let form: BTreeMap<_, _> = url::form_urlencoded::parse(forms[0].as_bytes())
        .into_owned()
        .collect();
    assert_eq!(form["refresh_token"], "old-refresh");
}

#[tokio::test]
async fn uncertain_or_dropped_rotation_cannot_replay_epoch_but_new_tokens_recover() {
    for variant in 0..3 {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = BrokerFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let provider_revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
        let verifier = broker.verifier(provider_revision, broker.origin(), None);
        let first_flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let second_flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        broker.response(
            false,
            300,
            "native-subject",
            Some("possibly-consumed-refresh"),
        );
        if variant == 0 {
            *broker.state.response.lock().unwrap() = "{malformed".into();
        }
        if variant == 1 {
            broker.state.disconnect_token.store(true, Ordering::SeqCst);
        }
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let first = manager(&issuer);
        let before = saved(&f);
        let error = if variant == 2 {
            let gate = broker.gate("/token");
            let mut pending =
                Box::pin(first.refresh_broker(begin(&first, &parent), &first_flow, 2));
            tokio::select! {
                result = &mut pending => panic!("rotation returned before the real HTTP barrier: {:?}", result.err()),
                () = broker.wait("/token") => {}
            }
            drop(pending);
            broker.state.gates.lock().unwrap().remove("/token");
            gate.notify_waiters();
            None
        } else {
            let error = first
                .refresh_broker(begin(&first, &parent), &first_flow, 2)
                .await
                .err()
                .unwrap();
            assert!(matches!(error.kind(), BrokerRefreshErrorKind::OAuth(_)));
            assert_denied(error, RemoteEffect::MayHaveApplied, &broker, 1, &f, &before);
            Some(error)
        };
        assert_credentials_preserved(&f, &before, true);
        assert_eq!(broker.count("/token"), 1);
        // Even an unrelated committed document revision cannot make the same
        // rotating credential safe to send again from another manager.
        competing(&issuer, &ctx, broker.origin(), Change::Document);
        let unchanged_epoch = saved(&f);
        broker.state.disconnect_token.store(false, Ordering::SeqCst);
        let valid = broker.response(false, 300, "native-subject", Some("recovery-refresh"));
        let second = manager(&issuer);
        let replay = second
            .refresh_broker(begin(&second, &parent), &second_flow, 3)
            .await
            .expect_err("uncertain epoch must remain fenced");
        assert_denied(
            replay,
            RemoteEffect::MayHaveApplied,
            &broker,
            1,
            &f,
            &unchanged_epoch,
        );
        let _original_error = error;
        assert_eq!(
            replay.kind(),
            BrokerRefreshErrorKind::OAuth(NativeErrorKind::InvalidRotation)
        );
        // A genuine newer credential publication changes the epoch. Keep the
        // verified issuer/subject while installing the fixture's signed access.
        let store = issuer.profile_store();
        applied(
            store
                .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                    let profile = doc.profile("a").unwrap();
                    let expected = TokenExpectation {
                        document_revision: doc.revision(),
                        provider_revision,
                        profile_revision: profile.revision(),
                        token_revision: profile.token_revision(),
                    };
                    let expires_at = i64::try_from(
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_micros(),
                    )
                    .unwrap()
                        + 300_000_000;
                    doc.rotate_tokens(
                        "a",
                        TokenUpdate {
                            access_token: secret(valid["access_token"].as_str().unwrap()),
                            refresh_token: secret("recovery-refresh"),
                            identity: ProfileIdentityUpdate::Preserve,
                            expires_at,
                        },
                        expected,
                        3,
                        ctx,
                    )
                })
                .unwrap(),
        );
        let recovered = broker.response(false, 300, "native-subject", Some("final-refresh"));
        let third = manager(&issuer);
        applied(
            third
                .refresh_broker(begin(&third, &parent), &second_flow, 4)
                .await
                .unwrap(),
        );
        assert_eq!(broker.count("/token"), 2);
        let current = store.load(&ctx).unwrap();
        let ProfileCredentials::Broker {
            issuer: stored_issuer,
            subject,
            tokens,
            ..
        } = &current
            .document()
            .profile("a")
            .unwrap()
            .data()
            .unwrap()
            .credentials
        else {
            panic!("recovered Broker credential")
        };
        assert_eq!(stored_issuer, broker.origin());
        assert_eq!(subject, "native-subject");
        assert_eq!(
            tokens.access_token.expose(),
            recovered["access_token"].as_str().unwrap()
        );
        assert_eq!(
            tokens.refresh_token.as_ref().unwrap().expose(),
            "final-refresh"
        );
        let forms = broker.state.forms.lock().unwrap();
        assert_eq!(forms.len(), 2);
        let form: BTreeMap<_, _> = url::form_urlencoded::parse(forms[1].as_bytes())
            .into_owned()
            .collect();
        assert_eq!(form["refresh_token"], "recovery-refresh");
    }
}

#[tokio::test]
async fn independent_issuers_serialize_the_same_rotation_before_post() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = BrokerFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let provider_revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
    let verifier = broker.verifier(provider_revision, broker.origin(), None);
    let first_flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let second_flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    broker.response(false, 300, "native-subject", Some("rotated-refresh"));
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let first = manager(&issuer);
    let independent = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let other_caller = independent.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let other_session = independent.bootstrap_admin(&other_caller).unwrap();
    let other_parent = independent
        .admin_parent(&other_caller, &other_session)
        .unwrap();
    let second = manager(&independent);
    let first_snapshot = begin(&first, &parent);
    let second_snapshot = begin(&second, &other_parent);
    let old_revision = first_snapshot.revision();
    assert_eq!(second_snapshot.revision(), old_revision);
    let gate = broker.gate("/token");
    let mut winner = Box::pin(first.refresh_broker(first_snapshot, &first_flow, 2));
    tokio::select! {
        result = &mut winner => panic!("rotation returned before the real HTTP barrier: {:?}", result.err()),
        () = broker.wait("/token") => {}
    }
    let mut loser = Box::pin(second.refresh_broker(second_snapshot, &second_flow, 2));
    tokio::select! {
        biased;
        result = &mut loser => panic!("same-epoch contender did not wait for rotation: {:?}", result.err()),
        () = tokio::task::yield_now() => {}
    }
    // A faulty second HTTP request must finish and fail the count assertion,
    // rather than hide behind an unreleased fixture response barrier.
    broker.state.gates.lock().unwrap().remove("/token");
    gate.notify_waiters();
    let (winner, loser) = tokio::join!(winner, loser);
    assert_eq!(
        broker.count("/token"),
        1,
        "independent issuers must exclude the same rotating credential before HTTP, not only at CAS"
    );
    applied(winner.unwrap());
    let loser = loser.expect_err("independent stale snapshot must remain unsent");
    assert_eq!(loser.remote_effect(), RemoteEffect::NotSent);
}

#[tokio::test]
async fn independent_issuer_cannot_replay_after_dispatched_future_is_dropped() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = BrokerFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
    let verifier = broker.verifier(revision, broker.origin(), None);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    broker.response(false, 300, "native-subject", Some("lost-response-refresh"));
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let first = manager(&issuer);
    let token_revision = begin(&first, &parent)
        .profiles()
        .find(|p| p.name() == "a")
        .unwrap()
        .token_revision();
    let gate = broker.gate("/token");
    let mut pending = Box::pin(first.refresh_broker(begin(&first, &parent), &flow, 2));
    tokio::select! {
        result = &mut pending => panic!("rotation returned before dispatch barrier: {:?}", result.err()),
        () = broker.wait("/token") => {}
    }
    let current = issuer.profile_store().load(&ctx).unwrap();
    assert!(current.document().profile("a").unwrap().refresh_uncertain());
    drop(current);
    drop(pending);
    broker.state.gates.lock().unwrap().remove("/token");
    gate.notify_waiters();

    let independent = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let caller = independent.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = independent.bootstrap_admin(&caller).unwrap();
    let parent = independent.admin_parent(&caller, &session).unwrap();
    let second = manager(&independent);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let denied = second
        .refresh_broker(begin(&second, &parent), &flow, 3)
        .await
        .expect_err("uncertain predecessor must remain blocked");
    assert_eq!(
        denied.kind(),
        BrokerRefreshErrorKind::OAuth(NativeErrorKind::InvalidRotation)
    );
    assert_eq!(denied.remote_effect(), RemoteEffect::MayHaveApplied);
    assert_eq!(broker.count("/token"), 1);
    let current = independent.profile_store().load(&ctx).unwrap();
    let selected = current.document().profile("a").unwrap();
    assert!(selected.refresh_uncertain());
    assert_eq!(selected.token_revision(), token_revision);
}

#[tokio::test]
async fn unrelated_document_mutation_during_http_preserves_rotation_and_other_data() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = BrokerFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
    let verifier = broker.verifier(revision, broker.origin(), None);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    broker.response(false, 300, "native-subject", Some("rotated-refresh"));
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let snapshot = begin(&m, &parent);
    let old_token = snapshot
        .profiles()
        .find(|p| p.name() == "a")
        .unwrap()
        .token_revision();
    let gate = broker.gate("/token");
    let mut pending = Box::pin(m.refresh_broker(snapshot, &flow, 2));
    tokio::select! {
        result = &mut pending => panic!("rotation returned before HTTP barrier: {:?}", result.err()),
        () = broker.wait("/token") => {}
    }
    competing(&issuer, &ctx, broker.origin(), Change::Document);
    broker.state.gates.lock().unwrap().remove("/token");
    gate.notify_waiters();
    applied(pending.await.unwrap());
    let current = issuer.profile_store().load(&ctx).unwrap();
    assert!(current.document().provider("unrelated").is_some());
    let profile = current.document().profile("a").unwrap();
    assert_eq!(profile.token_revision(), old_token + 1);
    assert!(!profile.refresh_uncertain());
    let ProfileCredentials::Broker { tokens, .. } = &profile.data().unwrap().credentials else {
        panic!("Broker credential must remain selected");
    };
    assert_eq!(
        tokens.refresh_token.as_ref().unwrap().expose(),
        "rotated-refresh"
    );
    assert_eq!(broker.count("/token"), 1);
}

#[tokio::test]
async fn waiting_profile_lock_keeps_original_cancellation_and_deadline_without_post() {
    for expired in [false, true] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = BrokerFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        broker.response(false, 300, "native-subject", Some("new-refresh"));
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let first = manager(&issuer);
        let gate = broker.gate("/token");
        let mut pending = Box::pin(first.refresh_broker(begin(&first, &parent), &flow, 2));
        tokio::select! {
            result = &mut pending => panic!("rotation returned before HTTP barrier: {:?}", result.err()),
            () = broker.wait("/token") => {}
        }
        let before = saved(&f);
        let waiting_cancel = Cancellation::default();
        let deadline = Instant::now() + Duration::from_millis(if expired { 100 } else { 30_000 });
        let waiting = request_before(&waiting_cancel, deadline);
        let independent = issuer_for(&f, &waiting, clock(), profile_policy(), &all_permissions());
        let caller = independent
            .authenticate_runtime(&f.secret(), &waiting)
            .unwrap();
        let session = independent.bootstrap_admin(&caller).unwrap();
        let parent = independent.admin_parent(&caller, &session).unwrap();
        let second = manager(&independent);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let mut loser = Box::pin(second.refresh_broker(begin(&second, &parent), &flow, 2));
        tokio::select! {
            result = &mut loser => panic!("lock contender must initially wait: {:?}", result.err()),
            () = tokio::task::yield_now() => {}
        }
        if !expired {
            waiting_cancel.cancel();
        }
        let error = loser.await.expect_err("Original must terminate lock wait");
        assert_eq!(error.remote_effect(), RemoteEffect::NotSent);
        assert_eq!(
            error.kind(),
            BrokerRefreshErrorKind::Manager(ProfileManagerError::Store(AuthStoreError::Profile(
                if expired {
                    ProfileError::Deadline
                } else {
                    ProfileError::Cancelled
                }
            )))
        );
        assert_eq!(broker.count("/token"), 1);
        assert_eq!(saved(&f), before);
        drop(pending);
        broker.state.gates.lock().unwrap().remove("/token");
        gate.notify_waiters();
    }
}

// The child exits without running destructors after the real POST is observed.
// Only isolated fixture paths and a non-secret revision cross this test boundary.
#[tokio::test]
#[ignore]
async fn refresh_process_exit_child() {
    let root = std::path::PathBuf::from(std::env::var_os("GRAPHIT_REFRESH_CRASH_FIXTURE").unwrap());
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = BrokerFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
    let verifier = broker.verifier(revision, broker.origin(), None);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    broker.response(false, 300, "native-subject", Some("lost-response-refresh"));
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let snapshot = begin(&m, &parent);
    let token_revision = snapshot
        .profiles()
        .find(|profile| profile.name() == "a")
        .unwrap()
        .token_revision();
    let _gate = broker.gate("/token");
    let mut pending = Box::pin(m.refresh_broker(snapshot, &flow, 2));
    tokio::select! {
        result = &mut pending => panic!("refresh returned before crash barrier: {:?}", result.err()),
        () = broker.wait("/token") => {}
    }
    assert_eq!(broker.count("/token"), 1);
    assert!(
        issuer
            .profile_store()
            .load(&ctx)
            .unwrap()
            .document()
            .profile("a")
            .unwrap()
            .refresh_uncertain()
    );
    std::fs::write(
        root.join("fixture.json"),
        serde_json::to_vec(&json!({
            "installation": f.installation.owner_root().to_str().unwrap(),
            "token_revision": token_revision,
        }))
        .unwrap(),
    )
    .unwrap();
    std::process::exit(73);
}

#[test]
fn process_exit_after_dispatch_releases_lock_but_retains_uncertainty() {
    let directory = tempfile::tempdir().unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "issuer::tests::profile_manager::broker_refresh::shared::refresh_process_exit_child",
            "--ignored",
            "--exact",
        ])
        .env("GRAPHIT_REFRESH_CRASH_FIXTURE", directory.path())
        .env("TMPDIR", directory.path())
        .env("TEMP", directory.path())
        .env("TMP", directory.path())
        .env_remove("GRAPHIT_GLOBAL_DIR")
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(73),
        "fixture must exit at the real dispatched-POST barrier"
    );
    let receipt: Value =
        serde_json::from_slice(&std::fs::read(directory.path().join("fixture.json")).unwrap())
            .unwrap();
    let root = std::path::PathBuf::from(receipt["installation"].as_str().unwrap());
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let paths = graphit_core::project_paths::InstallationPaths::resolve(
        graphit_core::brand::Brand::default(),
        &root,
        None,
        Some(&root),
    )
    .unwrap();
    let scope = graphit_core::artifact_store::PublicationScope::installation(
        &paths,
        InstallationId::new("local-installation").unwrap(),
        [root.clone()],
    )
    .unwrap();
    let runtime = Arc::new(RuntimeKeyOwner::start(root.clone(), ctx.snapshot()).unwrap());
    let store = Arc::new(
        crate::store::AuthStore::new(
            graphit_core::auth_document::AuthDocumentStore::new(
                root,
                graphit_core::auth_document::AuthDocumentLimits::new(4096).unwrap(),
                ctx.snapshot(),
            )
            .unwrap(),
            profile_policy(),
            &ctx,
        )
        .unwrap(),
    );
    let observed = store.load(&ctx).unwrap();
    let profile = observed.document().profile("a").unwrap();
    assert!(profile.refresh_uncertain());
    assert_eq!(
        profile.token_revision(),
        receipt["token_revision"].as_u64().unwrap()
    );
    let provider = observed.document().provider("broker").unwrap();
    let revision = provider.revision();
    let ProviderKind::Broker(config) = &provider.config().kind else {
        panic!("Broker fixture")
    };
    let endpoint = config.endpoint.clone();
    drop(observed);
    let verifier = BrokerVerifier::new(
        BrokerPolicy {
            endpoint: &endpoint,
            mcp_resource: None,
            provider_revision: revision,
            network: ClientPolicy {
                origin: &endpoint,
                allow_loopback_http: true,
                proxy: None,
                additional_roots: &[],
            },
        },
        crate::broker::tests::network_limits(),
        crate::broker::tests::limits(),
    )
    .unwrap();
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let issuer = RoleIssuer::with_sources(
        &scope,
        runtime,
        store,
        authority(&all_permissions(), &all_permissions(), limits(), 60),
        ttl(),
        &ctx,
        clock(),
        Arc::new(OsEntropy),
    )
    .unwrap();
    let secret = std::fs::read_to_string(scope.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let error = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(m.refresh_broker(begin(&m, &parent), &flow, 3))
        .expect_err("crashed predecessor must not be sent to the now-absent fixture server");
    assert_eq!(
        error.kind(),
        BrokerRefreshErrorKind::OAuth(NativeErrorKind::InvalidRotation)
    );
    assert_eq!(error.remote_effect(), RemoteEffect::MayHaveApplied);
}
