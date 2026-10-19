//! Distinct consumers share one issuer's rotating Broker refresh epoch.
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
    let loser = loser.err().expect("same-epoch snapshot must be stale");
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
    assert_eq!(fresh.revision(), old_revision + 1);
    assert_eq!(fresh.active_name(), Some("a"));
    let selected = fresh
        .profiles()
        .find(|profile| profile.name() == "a")
        .unwrap();
    assert_eq!(selected.token_revision(), old_token + 1);
    assert_eq!(selected.revision(), old_profile + 1);
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
        assert_eq!(saved(&f), before);
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
            .err()
            .expect("uncertain epoch must remain fenced");
        assert_denied(
            replay,
            RemoteEffect::MayHaveApplied,
            &broker,
            1,
            &f,
            &unchanged_epoch,
        );
        match error {
            Some(error) => assert_eq!(
                replay, error,
                "replay retains the original typed uncertainty"
            ),
            None => assert_eq!(
                replay.kind(),
                BrokerRefreshErrorKind::OAuth(NativeErrorKind::InvalidRotation)
            ),
        }
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
