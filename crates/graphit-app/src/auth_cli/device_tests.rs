use super::*;

#[tokio::test]
async fn device_cli_publishes_optional_account_and_logout_without_secret_events() {
    let f = Fixture::new(true);
    let b = Broker::new().await;
    seed(&f, &b);
    let verifier = b.verifier(&f);
    let oauth = NativeOAuth::new(&verifier, oauth_limits()).unwrap();
    let cancel = Cancellation::default();
    let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
    let caller = caller(&f, &ctx);
    let session = f.auth.issuer().bootstrap_admin(&caller).unwrap();
    let parent = f.auth.issuer().admin_parent(&caller, &session).unwrap();
    let manager = NativeProfileManager::new(f.auth.issuer());
    let mut input = login_args();
    input.device = true;
    let announced = std::cell::RefCell::new(String::new());
    applied(
        crate::auth_cli::device::login(
            &input,
            &manager,
            ProfileManagerAuthority::Admin(&parent),
            &oauth,
            |prompt| {
                let event = crate::admin_auth::Event::DeviceAuthorization {
                    verification_uri: prompt.verification_uri().into(),
                    verification_uri_complete: prompt
                        .verification_uri_complete()
                        .map(str::to_owned),
                    user_code: prompt.user_code().into(),
                };
                *announced.borrow_mut() = serde_json::to_string(&event)?;
                Ok(())
            },
            now,
        )
        .await
        .unwrap(),
    );
    let text = announced.into_inner();
    assert!(text.contains("ABCD-EFGH"));
    assert!(!text.contains("device-secret-canary"));
    assert!(!text.contains("access_token"));
    let account = f.read(&["account", "show"]);
    assert_eq!(account["profile"]["username"], Value::Null);
    assert_eq!(account["profile"]["teams"], Value::Null);
    f.publish(&["logout"], now().unwrap());
    assert!(f.read(&["account", "show"])["profile"].is_null());
    assert_eq!(b.state.rotation.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn device_cli_rejects_local_flags_and_revision_before_request() {
    for variant in 0..2 {
        let f = Fixture::new(true);
        let b = Broker::new().await;
        seed(&f, &b);
        let verifier = b.verifier(&f);
        let oauth = NativeOAuth::new(&verifier, oauth_limits()).unwrap();
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
        let caller = caller(&f, &ctx);
        let session = f.auth.issuer().bootstrap_admin(&caller).unwrap();
        let parent = f.auth.issuer().admin_parent(&caller, &session).unwrap();
        let manager = NativeProfileManager::new(f.auth.issuer());
        let mut input = login_args();
        input.device = true;
        if variant == 0 {
            input.username = Some("untrusted".into());
        } else {
            input.expected_revision = Some(u64::MAX);
        }
        let before = f.bytes();
        assert!(
            crate::auth_cli::device::login(
                &input,
                &manager,
                ProfileManagerAuthority::Admin(&parent),
                &oauth,
                |_| panic!("unexpected prompt"),
                now
            )
            .await
            .is_err()
        );
        assert_eq!(f.bytes(), before);
        assert_eq!(b.state.rotation.load(Ordering::SeqCst), 0);
    }
}
#[tokio::test]
async fn device_cli_announcement_failure_and_cancel_do_not_publish() {
    for announce_failure in [false, true] {
        let f = Fixture::new(true);
        let b = Broker::new().await;
        seed(&f, &b);
        let verifier = b.verifier(&f);
        let oauth = NativeOAuth::new(&verifier, oauth_limits()).unwrap();
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
        let caller = caller(&f, &ctx);
        let session = f.auth.issuer().bootstrap_admin(&caller).unwrap();
        let parent = f.auth.issuer().admin_parent(&caller, &session).unwrap();
        let manager = NativeProfileManager::new(f.auth.issuer());
        let mut input = login_args();
        input.device = true;
        let before = f.bytes();
        let result = crate::auth_cli::device::login(
            &input,
            &manager,
            ProfileManagerAuthority::Admin(&parent),
            &oauth,
            |_| {
                if announce_failure {
                    Err(io::ErrorKind::BrokenPipe.into())
                } else {
                    cancel.cancel();
                    Ok(())
                }
            },
            now,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(f.bytes(), before);
        assert_eq!(b.state.rotation.load(Ordering::SeqCst), 0);
    }
}
