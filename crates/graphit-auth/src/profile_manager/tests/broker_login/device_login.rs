use super::*;

fn access_only(broker: &LoginFixture) {
    let mut body = broker.response("unused", 300, "device-subject", None);
    body.as_object_mut().unwrap().remove("id_token");
    body["scope"] = "".into();
    *broker.state.response.lock().unwrap() = body.to_string();
}
#[tokio::test]
async fn device_publish_activates_access_only_account_and_logout_without_id() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = LoginFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let revision = seed(&issuer, &ctx, broker.origin(), true);
    let verifier = broker.verifier(revision, broker.origin(), None);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let baseline = issuer
        .profile_store()
        .load(&ctx)
        .unwrap()
        .document()
        .revision();
    access_only(&broker);
    let mut pending = m
        .begin_device_broker_login(begin(&m, &parent), &flow, selected("a"))
        .await
        .unwrap();
    assert_eq!(pending.instructions().unwrap().user_code(), "ABCD-EFGH");
    let received = pending.receive().await.unwrap();
    applied(received.publish(2).await.unwrap());
    let snapshot = m.begin(ProfileManagerAuthority::Admin(&parent)).unwrap();
    assert_eq!(snapshot.revision(), baseline + 2);
    assert_eq!(snapshot.active_name(), Some("a"));
    let view = snapshot.profiles().find(|p| p.name() == "a").unwrap();
    assert_eq!(view.username(), None);
    assert!(view.teams().is_none());
    assert_eq!(
        view.broker_identity(),
        Some((broker.origin(), "device-subject"))
    );
    let store = issuer.profile_store().load(&ctx).unwrap();
    let data = store.document().profile("a").unwrap();
    assert!(data.data().is_none());
    let device = data.broker_device_data().unwrap();
    assert!(device.metadata.username.is_none());
    assert!(device.metadata.teams.is_none());
    let value: Value = serde_json::from_slice(&saved(&f)).unwrap();
    let encoded = value.to_string();
    assert!(!encoded.contains("unused"));
    let owned = value["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["data"]["name"] == "a")
        .unwrap();
    assert_eq!(owned["data"]["credentials"]["kind"], "broker_device");
    assert!(
        owned["data"]["credentials"]["tokens"]
            .get("id_token")
            .is_none()
    );
    let revision = snapshot.revision();
    applied(m.logout(snapshot, Some("a"), revision).unwrap());
    assert_eq!(broker.count("/userinfo"), 0);
    assert_eq!(broker.count("/revoke"), 0);
    broker.shutdown().await;
}
#[tokio::test]
async fn device_mutated_snapshot_denies_before_next_post_and_after_consumption() {
    for after_receive in [false, true] {
        for variant in 0..6 {
            let cancel = Cancellation::default();
            let ctx = operation(&cancel);
            let f = Fixture::new(&ctx);
            let broker = LoginFixture::new().await;
            let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
            let revision = seed(&issuer, &ctx, broker.origin(), true);
            let verifier = broker.verifier(revision, broker.origin(), None);
            let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
            let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
            let session = issuer.bootstrap_admin(&caller).unwrap();
            let parent = issuer.admin_parent(&caller, &session).unwrap();
            let m = manager(&issuer);
            access_only(&broker);
            let pending = m
                .begin_device_broker_login(begin(&m, &parent), &flow, selected("a"))
                .await
                .unwrap();
            if after_receive {
                let received = pending.receive().await.unwrap();
                mutate(&issuer, &ctx, broker.origin(), variant);
                let before = saved(&f);
                let error = received.publish(3).await.err().unwrap();
                denied(error, RemoteEffect::MayHaveApplied, &broker, 1, &f, &before);
            } else {
                mutate(&issuer, &ctx, broker.origin(), variant);
                let before = saved(&f);
                let error = pending.receive().await.err().unwrap();
                denied(error, RemoteEffect::NotSent, &broker, 0, &f, &before);
            }
            broker.shutdown().await;
        }
    }
}
#[tokio::test]
async fn device_cancel_after_token_and_after_prepare_preserves_real_effects() {
    for after_prepare in [false, true] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), true);
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        access_only(&broker);
        let before = saved(&f);
        let pending = m
            .begin_device_broker_login(begin(&m, &parent), &flow, selected("a"))
            .await
            .unwrap();
        let received = pending.receive().await.unwrap();
        if !after_prepare {
            cancel.cancel();
            let error = received.publish(3).await.err().unwrap();
            denied(error, RemoteEffect::MayHaveApplied, &broker, 1, &f, &before);
        } else {
            let signal = cancel.clone();
            after_encoding(move || signal.cancel());
            let error = received.publish(3).await.err().unwrap();
            denied(error, RemoteEffect::MayHaveApplied, &broker, 1, &f, &before);
        }
        broker.shutdown().await;
    }
}
