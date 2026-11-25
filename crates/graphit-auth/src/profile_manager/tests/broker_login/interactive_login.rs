use super::*;
use crate::profile_manager::{
    InteractiveLoginConfig, InteractiveLoginError, InteractiveLoginLimits,
    InteractiveTransportError, ManagedInteractiveBrokerLogin,
};
use std::{
    future::Future,
    net::{IpAddr, SocketAddr},
    task::Poll,
};
use tokio::net::TcpStream;

fn transport_limits() -> InteractiveLoginLimits {
    InteractiveLoginLimits {
        head_bytes: 1024,
        header_entries: 8,
        target_bytes: 512,
        callback_bytes: 1024,
    }
}
fn config(bind: SocketAddr) -> InteractiveLoginConfig<'static> {
    InteractiveLoginConfig {
        bind,
        callback_path: "/callback",
        limits: transport_limits(),
    }
}
fn callback_parts(pending: &mut ManagedInteractiveBrokerLogin<'_>) -> (url::Url, String) {
    let authorization = url::Url::parse(pending.authorization_url().unwrap()).unwrap();
    let params: BTreeMap<String, String> = authorization.query_pairs().into_owned().collect();
    let mut redirect = url::Url::parse(&params["redirect_uri"]).unwrap();
    redirect.set_query(Some(&format!("code=canary-code&state={}", params["state"])));
    (redirect, params["nonce"].clone())
}
fn address(callback: &url::Url) -> SocketAddr {
    let ip = match callback.host().unwrap() {
        url::Host::Ipv4(ip) => IpAddr::V4(ip),
        url::Host::Ipv6(ip) => IpAddr::V6(ip),
        _ => panic!("callback is not a literal IP"),
    };
    SocketAddr::new(ip, callback.port().unwrap())
}
fn request(callback: &url::Url) -> String {
    format!(
        "GET {}?{} HTTP/1.1\r\nHost: {}\r\n\r\n",
        callback.path(),
        callback.query().unwrap(),
        address(callback)
    )
}
async fn send(callback: &url::Url, bytes: &[u8], fragmented: bool) -> Vec<u8> {
    let mut peer = TcpStream::connect(address(callback)).await.unwrap();
    if fragmented {
        for chunk in bytes.chunks(7) {
            peer.write_all(chunk).await.unwrap();
            tokio::task::yield_now().await;
        }
    } else {
        peer.write_all(bytes).await.unwrap();
    }
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), peer.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    response
}

#[tokio::test]
async fn actual_public_entrypoint_ipv4_ipv6_and_fixed_port_publish_once_after_receive() {
    for variant in 0..3 {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), false);
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let reserved = if variant == 2 {
            Some(TcpListener::bind("127.0.0.1:0").await.unwrap())
        } else {
            None
        };
        let bind = if let Some(listener) = &reserved {
            listener.local_addr().unwrap()
        } else if variant == 1 {
            "[::1]:0".parse().unwrap()
        } else {
            "127.0.0.1:0".parse().unwrap()
        };
        drop(reserved);
        let mut pending = m
            .begin_interactive_broker_login(begin(&m, &parent), &flow, selected("a"), config(bind))
            .await
            .unwrap();
        let (callback, nonce) = callback_parts(&mut pending);
        if variant == 2 {
            assert_eq!(address(&callback), bind);
        }
        broker.response(
            &nonce,
            300,
            "interactive-subject",
            Some("interactive-refresh"),
        );
        let wire = request(&callback);
        let (received, response) =
            tokio::join!(pending.receive(), send(&callback, wire.as_bytes(), true));
        assert!(response.starts_with(b"HTTP/1.1 204"));
        assert_eq!(broker.count("/token"), 0);
        assert!(TcpStream::connect(address(&callback)).await.is_err());
        let now = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )
        .unwrap();
        applied(received.unwrap().publish(now).await.unwrap());
        let snapshot = issuer.profile_store().load(&ctx).unwrap();
        assert_eq!(snapshot.document().active_name(), Some("a"));
        assert_eq!(snapshot.document().profile("a").unwrap().created_at(), now);
        assert_eq!(broker.count("/token"), 1);
        assert_eq!(broker.count("/userinfo"), 0);
        broker.shutdown().await;
    }
}

#[tokio::test]
async fn real_wrong_state_ends_attempt_without_post_or_second_callback_listener() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = LoginFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let revision = seed(&issuer, &ctx, broker.origin(), false);
    let verifier = broker.verifier(revision, broker.origin(), None);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let mut pending = m
        .begin_interactive_broker_login(
            begin(&m, &parent),
            &flow,
            selected("a"),
            config("127.0.0.1:0".parse().unwrap()),
        )
        .await
        .unwrap();
    let (mut callback, _) = callback_parts(&mut pending);
    callback.set_query(Some("code=canary-code&state=wrong"));
    let prior = saved(&f);
    let wire = request(&callback);
    let (received, response) =
        tokio::join!(pending.receive(), send(&callback, wire.as_bytes(), false));
    assert!(response.starts_with(b"HTTP/1.1 204"));
    let error = received.unwrap().publish(2).await.err().unwrap();
    assert_eq!(
        error.kind(),
        BrokerLoginErrorKind::OAuth(crate::broker::oauth::NativeErrorKind::InvalidCallback)
    );
    assert_eq!(error.remote_effect(), RemoteEffect::NotSent);
    assert_eq!(broker.count("/token"), 0);
    assert_eq!(saved(&f), prior);
    assert!(TcpStream::connect(address(&callback)).await.is_err());
    broker.shutdown().await;
}

#[tokio::test]
async fn actual_socket_head_exact_then_plus_one_eof_and_framing_reject_before_post() {
    for variant in 0..9 {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), false);
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let bind = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap()
            .local_addr()
            .unwrap();
        let fake = url::Url::parse(&format!("http://{bind}/callback?code=c&state=s")).unwrap();
        let sample = request(&fake);
        let mut policy = config(bind);
        if variant <= 1 {
            policy.limits.head_bytes = sample.len() - usize::from(variant == 1);
            policy.limits.header_entries = 1;
        }
        if variant == 5 {
            policy.limits.header_entries = 1;
        }
        if variant == 6 || variant == 7 {
            policy.limits.target_bytes =
                fake.path().len() + 1 + fake.query().unwrap().len() - usize::from(variant == 7);
        }
        let mut pending = m
            .begin_interactive_broker_login(begin(&m, &parent), &flow, selected("a"), policy)
            .await
            .unwrap();
        let (callback, _) = callback_parts(&mut pending);
        let mut wire = sample.clone();
        if variant == 2 {
            wire = wire.replacen("GET ", "POST ", 1);
        }
        if variant == 3 {
            wire = wire.replacen("\r\n\r\n", "\r\nContent-Length: 1\r\n\r\n", 1);
        }
        if variant == 4 {
            wire.truncate(5);
        }
        if variant == 5 {
            wire = wire.replacen("\r\n\r\n", "\r\nX-Extra: x\r\n\r\n", 1);
        }
        if variant == 8 {
            wire = wire.replacen(&format!("Host: {bind}"), "Host: wrong", 1);
        }
        let original = wire.clone();
        let send_operation = async {
            let mut peer = TcpStream::connect(address(&callback)).await.unwrap();
            peer.write_all(original.as_bytes()).await.unwrap();
            peer.shutdown().await.unwrap();
            let mut response = Vec::new();
            tokio::time::timeout(Duration::from_secs(5), peer.read_to_end(&mut response))
                .await
                .unwrap()
                .unwrap();
            if variant != 0 && variant != 6 {
                cancel.cancel();
            }
            response
        };
        let (received, response) = tokio::join!(pending.receive(), send_operation);
        if variant == 0 || variant == 6 {
            assert!(received.is_ok());
            assert!(response.starts_with(b"HTTP/1.1 204"));
            drop(received);
        } else {
            assert!(matches!(
                received.err().unwrap(),
                InteractiveLoginError::Transport(InteractiveTransportError::Cancelled)
            ));
            assert!(response.starts_with(b"HTTP/1.1 400"));
        }
        assert_eq!(broker.count("/token"), 0);
        broker.shutdown().await;
    }
}

#[tokio::test]
async fn physical_cas_or_authority_changes_during_real_listener_wait_deny_same_pending() {
    for authority_change in [false, true] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let test_clock = clock();
        let issuer = issuer_for(
            &f,
            &ctx,
            Arc::clone(&test_clock),
            profile_policy(),
            &all_permissions(),
        );
        let revision = seed(&issuer, &ctx, broker.origin(), true);
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let mut pending = m
            .begin_interactive_broker_login(
                begin(&m, &parent),
                &flow,
                selected("a"),
                config("127.0.0.1:0".parse().unwrap()),
            )
            .await
            .unwrap();
        let (callback, nonce) = callback_parts(&mut pending);
        broker.response(&nonce, 300, "interactive-subject", None);
        let mut receiving = Box::pin(pending.receive());
        std::future::poll_fn(|cx| {
            assert!(matches!(receiving.as_mut().poll(cx), Poll::Pending));
            Poll::Ready(())
        })
        .await;
        if authority_change {
            test_clock.seconds.store(61, Ordering::SeqCst);
        } else {
            mutate(&issuer, &ctx, broker.origin(), 0);
        }
        let winner = saved(&f);
        let wire = request(&callback);
        let (received, response) = tokio::join!(receiving, send(&callback, wire.as_bytes(), true));
        assert!(response.starts_with(b"HTTP/1.1 204"));
        let error = received.unwrap().publish(2).await.err().unwrap();
        assert_eq!(error.remote_effect(), RemoteEffect::NotSent);
        assert_eq!(broker.count("/token"), 0);
        assert_eq!(saved(&f), winner);
        assert!(TcpStream::connect(address(&callback)).await.is_err());
        broker.shutdown().await;
    }
}

#[tokio::test]
async fn original_cancel_of_genuinely_pending_public_receive_closes_listener_and_releases_snapshot()
{
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = LoginFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let revision = seed(&issuer, &ctx, broker.origin(), false);
    let verifier = broker.verifier(revision, broker.origin(), None);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let mut pending = m
        .begin_interactive_broker_login(
            begin(&m, &parent),
            &flow,
            selected("a"),
            config("127.0.0.1:0".parse().unwrap()),
        )
        .await
        .unwrap();
    let (callback, _) = callback_parts(&mut pending);
    let prior = saved(&f);
    let mut receiving = Box::pin(pending.receive());
    std::future::poll_fn(|cx| {
        assert!(matches!(receiving.as_mut().poll(cx), Poll::Pending));
        Poll::Ready(())
    })
    .await;
    cancel.cancel();
    assert_eq!(
        receiving.await.err().unwrap(),
        InteractiveLoginError::Transport(InteractiveTransportError::Cancelled)
    );
    assert_eq!(broker.count("/token"), 0);
    assert_eq!(saved(&f), prior);
    assert!(TcpStream::connect(address(&callback)).await.is_err());
    broker.shutdown().await;
}

#[tokio::test]
async fn stale_actual_admin_snapshot_denies_before_occupied_bind_or_discovery() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = LoginFixture::new().await;
    let test_clock = clock();
    let issuer = issuer_for(
        &f,
        &ctx,
        Arc::clone(&test_clock),
        profile_policy(),
        &all_permissions(),
    );
    let revision = seed(&issuer, &ctx, broker.origin(), false);
    let verifier = broker.verifier(revision, broker.origin(), None);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let snapshot = begin(&m, &parent);
    let occupied = TcpListener::bind("127.0.0.1:0").await.unwrap();
    test_clock.seconds.store(61, Ordering::SeqCst);
    let error = m
        .begin_interactive_broker_login(
            snapshot,
            &flow,
            selected("a"),
            config(occupied.local_addr().unwrap()),
        )
        .await
        .err()
        .unwrap();
    let InteractiveLoginError::Broker(error) = error else {
        panic!("stale authority did not deny before bind")
    };
    assert_eq!(error.remote_effect(), RemoteEffect::NotSent);
    assert_eq!(broker.count("/metadata"), 0);
    assert_eq!(broker.count("/token"), 0);
    drop(occupied);
    broker.shutdown().await;
}

#[tokio::test]
async fn rejected_http_requests_allow_valid_callback_on_same_original() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = LoginFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let revision = seed(&issuer, &ctx, broker.origin(), false);
    let verifier = broker.verifier(revision, broker.origin(), None);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let mut pending = m
        .begin_interactive_broker_login(
            begin(&m, &parent),
            &flow,
            selected("a"),
            config("127.0.0.1:0".parse().unwrap()),
        )
        .await
        .unwrap();
    let (callback, nonce) = callback_parts(&mut pending);
    broker.response(&nonce, 300, "interactive-subject", None);
    let valid = request(&callback);
    let invalid = valid.replacen("/callback", "/wrong", 1);
    let sending = async {
        for _ in 0..4 {
            let response = send(&callback, invalid.as_bytes(), false).await;
            assert!(response.starts_with(b"HTTP/1.1 400"));
            assert_eq!(broker.count("/token"), 0);
        }
        let response = send(&callback, valid.as_bytes(), false).await;
        assert!(response.starts_with(b"HTTP/1.1 204"));
    };
    let (received, ()) = tokio::join!(pending.receive(), sending);
    assert!(TcpStream::connect(address(&callback)).await.is_err());
    applied(received.unwrap().publish(2).await.unwrap());
    assert_eq!(broker.count("/token"), 1);
    assert_eq!(begin(&m, &parent).active_name(), Some("a"));
    broker.shutdown().await;
}
