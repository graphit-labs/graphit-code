use futures_util::{SinkExt, StreamExt};
use graphit_core::file_lock::Cancellation;
use graphit_http::{
    client::{ClientPolicy, Limits, NetworkError, NetworkRequestContext},
    websocket::{
        CloseCode, Message, WebSocketClient, WebSocketError, WebSocketFailureKind, WebSocketLimits,
        WebSocketRead,
    },
};
use http::{HeaderMap, HeaderValue, header};
use std::{
    error::Error,
    num::NonZeroUsize,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpSocket},
};
use tokio_tungstenite::{
    accept_async,
    tungstenite::{
        Message as PeerMessage,
        protocol::{
            CloseFrame,
            frame::{
                Frame,
                coding::{Data, OpCode},
            },
        },
    },
};

fn nonzero(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}
fn network() -> Limits {
    Limits {
        request_body_bytes: 1024,
        response_body_bytes: 1024,
        url_bytes: 2048,
        header_bytes: 4096,
        header_count: 32,
        max_in_flight: 2,
        max_idle_per_host: 2,
        idle_timeout: Duration::from_secs(5),
        max_certificates: 2,
        max_certificate_bytes: 8192,
    }
}
fn caps(frame: usize, message: usize, send: usize) -> WebSocketLimits {
    WebSocketLimits {
        frame_bytes: nonzero(frame),
        message_bytes: nonzero(message),
        send_bytes: nonzero(send),
    }
}
fn client(origin: &str, limits: WebSocketLimits) -> WebSocketClient {
    WebSocketClient::new(
        ClientPolicy {
            origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        network(),
        limits,
    )
    .unwrap()
}
fn original(signal: &Cancellation, duration: Duration) -> NetworkRequestContext<'_> {
    NetworkRequestContext::new(Instant::now() + duration, signal)
}
fn headers() -> HeaderMap {
    let mut result = HeaderMap::new();
    result.insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer transport-only-marker"),
    );
    result
}
async fn listen() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1/realtime", listener.local_addr().unwrap());
    (listener, url)
}
fn sdk_kind(error: WebSocketError, expected: WebSocketFailureKind) {
    match error {
        WebSocketError::Sdk(cause) => assert_eq!(cause.kind(), expected),
        other => panic!("{other}"),
    }
}

#[tokio::test]
#[expect(
    clippy::result_large_err,
    reason = "SDK handshake callback fixes the unboxed HTTP error response ABI"
)]
async fn fragmented_message_survives_wakeup_heartbeat_and_close_terminalizes() {
    let (listener, url) = listen().await;
    let peer = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_hdr_async(
            socket,
            |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                assert_eq!(request.uri().path(), "/v1/realtime");
                assert_eq!(
                    request.headers()[header::AUTHORIZATION],
                    "Bearer transport-only-marker"
                );
                assert_eq!(request.headers()[header::ACCEPT_ENCODING], "identity");
                Ok(response)
            },
        )
        .await
        .unwrap();
        socket
            .send(PeerMessage::Frame(Frame::message(
                b"hello ".to_vec(),
                OpCode::Data(Data::Text),
                false,
            )))
            .await
            .unwrap();
        assert_eq!(
            socket.next().await.unwrap().unwrap().into_text().unwrap(),
            "heartbeat"
        );
        socket
            .send(PeerMessage::Frame(Frame::message(
                "🌊".as_bytes().to_vec(),
                OpCode::Data(Data::Continue),
                true,
            )))
            .await
            .unwrap();
        socket
            .send(PeerMessage::Close(Some(CloseFrame {
                code: 1013.into(),
                reason: "finite".into(),
            })))
            .await
            .unwrap();
    });
    let signal = Cancellation::default();
    let client = client(&url, caps(64, 64, 64));
    let mut socket = client
        .connect(&url, headers(), original(&signal, Duration::from_secs(5)))
        .await
        .unwrap();
    assert!(matches!(
        socket
            .receive_until(Instant::now() + Duration::from_millis(30))
            .await
            .unwrap(),
        WebSocketRead::Wakeup
    ));
    socket
        .send(Message::Text("heartbeat".into()))
        .await
        .unwrap();
    match socket
        .receive_until(Instant::now() + Duration::from_secs(1))
        .await
        .unwrap()
    {
        WebSocketRead::Message(Message::Text(text)) => assert_eq!(text, "hello 🌊"),
        _ => panic!("missing full text"),
    }
    match socket
        .receive_until(Instant::now() + Duration::from_secs(1))
        .await
        .unwrap()
    {
        WebSocketRead::Message(Message::Close { code, .. }) => assert_eq!(u16::from(code), 1013),
        _ => panic!("missing close"),
    }
    assert!(matches!(
        socket.send(Message::Text("after close".into())).await,
        Err(WebSocketError::Closed)
    ));
    peer.await.unwrap();
}

#[tokio::test]
async fn frame_and_reassembled_message_caps_are_distinct_native_sdk_limits() {
    for fragmented in [false, true] {
        let (listener, url) = listen().await;
        let peer = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(socket).await.unwrap();
            if fragmented {
                socket
                    .send(PeerMessage::Frame(Frame::message(
                        vec![b'a'; 4],
                        OpCode::Data(Data::Text),
                        false,
                    )))
                    .await
                    .unwrap();
                socket
                    .send(PeerMessage::Frame(Frame::message(
                        vec![b'b'; 4],
                        OpCode::Data(Data::Continue),
                        true,
                    )))
                    .await
                    .unwrap();
            } else {
                socket
                    .send(PeerMessage::Text("12345678".into()))
                    .await
                    .unwrap();
            }
        });
        let signal = Cancellation::default();
        let limits = if fragmented {
            caps(4, 7, 16)
        } else {
            caps(7, 16, 16)
        };
        let mut socket = client(&url, limits)
            .connect(
                &url,
                HeaderMap::new(),
                original(&signal, Duration::from_secs(5)),
            )
            .await
            .unwrap();
        let error = match socket
            .receive_until(Instant::now() + Duration::from_secs(1))
            .await
        {
            Err(error) => error,
            Ok(_) => panic!("oversized payload accepted"),
        };
        sdk_kind(error, WebSocketFailureKind::Capacity);
        assert!(matches!(
            socket.send(Message::Text("x".into())).await,
            Err(WebSocketError::Closed)
        ));
        peer.await.unwrap();
    }
}

#[tokio::test]
async fn elapsed_heartbeat_wakeup_precedes_ready_data_without_losing_it() {
    let (listener, url) = listen().await;
    let (sent_tx, sent_rx) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(socket).await.unwrap();
        socket
            .send(PeerMessage::Text("queued snapshot".into()))
            .await
            .unwrap();
        sent_tx.send(()).unwrap();
        assert!(matches!(
            socket.next().await,
            Some(Ok(PeerMessage::Close(_)))
        ));
    });
    let signal = Cancellation::default();
    let mut socket = client(&url, caps(64, 64, 64))
        .connect(
            &url,
            HeaderMap::new(),
            original(&signal, Duration::from_secs(3)),
        )
        .await
        .unwrap();
    sent_rx.await.unwrap();
    assert!(matches!(
        socket
            .receive_until(Instant::now() - Duration::from_secs(1))
            .await
            .unwrap(),
        WebSocketRead::Wakeup
    ));
    match socket
        .receive_until(Instant::now() + Duration::from_secs(1))
        .await
        .unwrap()
    {
        WebSocketRead::Message(Message::Text(text)) => assert_eq!(text, "queued snapshot"),
        _ => panic!("ready data lost"),
    }
    socket.close(CloseCode::Normal).await.unwrap();
    peer.await.unwrap();
}

#[tokio::test]
async fn original_cancel_deadline_and_abandoned_read_end_the_native_socket() {
    for mode in ["cancel", "deadline", "drop"] {
        let (listener, url) = listen().await;
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let peer = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(socket).await.unwrap();
            ready_tx.send(()).unwrap();
            let received = tokio::time::timeout(Duration::from_secs(2), socket.next())
                .await
                .unwrap();
            assert!(!matches!(received, Some(Ok(PeerMessage::Text(_)))));
        });
        let signal = Cancellation::default();
        let duration = if mode == "deadline" {
            Duration::from_millis(250)
        } else {
            Duration::from_secs(3)
        };
        let mut socket = client(&url, caps(64, 64, 64))
            .connect(&url, HeaderMap::new(), original(&signal, duration))
            .await
            .unwrap();
        ready_rx.await.unwrap();
        if mode == "drop" {
            let result = tokio::time::timeout(
                Duration::from_millis(20),
                socket.receive_until(Instant::now() + Duration::from_secs(1)),
            )
            .await;
            assert!(result.is_err());
        } else {
            if mode == "cancel" {
                signal.cancel();
            }
            let error = match socket
                .receive_until(Instant::now() + Duration::from_secs(1))
                .await
            {
                Err(error) => error,
                Ok(_) => panic!("expired Original accepted"),
            };
            assert!(matches!(
                (mode, error),
                ("cancel", WebSocketError::Network(NetworkError::Cancelled))
                    | ("deadline", WebSocketError::Network(NetworkError::Deadline))
            ));
        }
        assert!(matches!(
            socket.send(Message::Text("x".into())).await,
            Err(WebSocketError::Closed)
        ));
        peer.await.unwrap();
    }
}

#[tokio::test]
async fn service_original_keeps_socket_active_until_real_cancellation() {
    let (listener, url) = listen().await;
    let peer = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(socket).await.unwrap();
        let received = tokio::time::timeout(Duration::from_secs(2), socket.next())
            .await
            .unwrap();
        assert!(!matches!(received, Some(Ok(PeerMessage::Text(_)))));
    });
    let signal = Cancellation::default();
    let mut socket = client(&url, caps(64, 64, 64))
        .connect(
            &url,
            HeaderMap::new(),
            NetworkRequestContext::service(&signal),
        )
        .await
        .unwrap();
    let mut pending = Box::pin(socket.receive());
    tokio::select! {
        result = &mut pending => panic!("service read ended before cancellation: {}", result.is_ok()),
        _ = tokio::time::sleep(Duration::from_millis(20)) => {}
    }
    signal.cancel();
    assert!(matches!(
        pending.await,
        Err(WebSocketError::Network(NetworkError::Cancelled))
    ));
    assert!(matches!(
        socket.send(Message::Text("x".into())).await,
        Err(WebSocketError::Closed)
    ));
    peer.await.unwrap();
}

#[tokio::test]
async fn oversized_or_abandoned_send_is_terminal_without_flushing_pending_data() {
    for oversized in [true, false] {
        let listener = TcpSocket::new_v4().unwrap();
        listener.set_recv_buffer_size(4096).unwrap();
        listener.bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let listener = listener.listen(1).unwrap();
        let url = format!("http://{}/v1/realtime", listener.local_addr().unwrap());
        let (check_peer, after_send) = tokio::sync::oneshot::channel();
        let peer = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(socket).await.unwrap();
            after_send.await.unwrap();
            let received = tokio::time::timeout(Duration::from_secs(2), socket.next())
                .await
                .unwrap();
            assert!(!matches!(received, Some(Ok(PeerMessage::Text(_)))));
        });
        let signal = Cancellation::default();
        let mut socket = client(
            &url,
            caps(64, 64, if oversized { 4 } else { 20 * 1024 * 1024 }),
        )
        .connect(
            &url,
            HeaderMap::new(),
            original(&signal, Duration::from_secs(5)),
        )
        .await
        .unwrap();
        if oversized {
            assert!(matches!(
                socket.send(Message::Text("12345".into())).await,
                Err(WebSocketError::Network(NetworkError::Limit))
            ));
        } else {
            let result = tokio::time::timeout(
                Duration::from_millis(50),
                socket.send(Message::Text("x".repeat(16 * 1024 * 1024))),
            )
            .await;
            assert!(
                result.is_err(),
                "native send unexpectedly completed before abandonment"
            );
        }
        assert!(matches!(
            socket.send(Message::Text("x".into())).await,
            Err(WebSocketError::Closed)
        ));
        check_peer.send(()).unwrap();
        peer.await.unwrap();
    }
}

#[tokio::test]
async fn origin_headers_and_original_are_checked_before_handshake() {
    let (listener, url) = listen().await;
    let signal = Cancellation::default();
    let client = client(&url, caps(64, 64, 64));
    let result = client
        .connect(
            "http://127.0.0.1:1/other",
            HeaderMap::new(),
            original(&signal, Duration::from_secs(1)),
        )
        .await;
    assert!(matches!(
        result,
        Err(WebSocketError::Network(NetworkError::Origin))
    ));
    let mut hostile = HeaderMap::new();
    hostile.insert(
        header::SEC_WEBSOCKET_KEY,
        HeaderValue::from_static("override"),
    );
    assert!(matches!(
        client
            .connect(&url, hostile, original(&signal, Duration::from_secs(1)))
            .await,
        Err(WebSocketError::Network(NetworkError::InvalidRequest))
    ));
    let mut bound = network();
    bound.header_count = 4;
    let tiny = WebSocketClient::new(
        ClientPolicy {
            origin: &url,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        bound,
        caps(64, 64, 64),
    )
    .unwrap();
    assert!(matches!(
        tiny.connect(
            &url,
            HeaderMap::new(),
            original(&signal, Duration::from_secs(1))
        )
        .await,
        Err(WebSocketError::Network(NetworkError::Limit))
    ));
    signal.cancel();
    assert!(matches!(
        client
            .connect(&url, headers(), original(&signal, Duration::from_secs(1)))
            .await,
        Err(WebSocketError::Network(NetworkError::Cancelled))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(20), listener.accept())
            .await
            .is_err()
    );
    assert!(
        WebSocketClient::new(
            ClientPolicy {
                origin: &url,
                allow_loopback_http: false,
                proxy: None,
                additional_roots: &[]
            },
            network(),
            caps(64, 64, 64)
        )
        .is_err()
    );
}

#[tokio::test]
async fn invalid_sdk_handshake_cause_is_retained_but_its_chain_is_redacted() {
    let (listener, url) = listen().await;
    let peer = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = [0u8; 4096];
        assert!(socket.read(&mut bytes).await.unwrap() > 0);
        socket.write_all(b"HTTP/1.1 101 Switching Protocols\r\nConnection: upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Accept: private-handshake-canary\r\n\r\n").await.unwrap();
    });
    let signal = Cancellation::default();
    let error = match client(&url, caps(64, 64, 64))
        .connect(&url, headers(), original(&signal, Duration::from_secs(1)))
        .await
    {
        Err(error) => error,
        Ok(_) => panic!("invalid handshake accepted"),
    };
    assert!(
        matches!(&error, WebSocketError::Sdk(cause) if cause.kind() == WebSocketFailureKind::Handshake)
    );
    assert!(matches!(&error, WebSocketError::Sdk(cause) if cause.http_status().is_none()));
    assert!(format!("{error}").contains("unexpected_header_value"));
    let mut source: Option<&dyn Error> = Some(&error);
    while let Some(error) = source {
        assert!(!format!("{error:?} {error}").contains("private-handshake-canary"));
        source = error.source();
    }
    peer.await.unwrap();
}

#[tokio::test]
async fn pre_upgrade_http_status_is_actual_typed_data_with_redacted_diagnostics() {
    for status in [401u16, 403, 503, 504] {
        let (listener, url) = listen().await;
        let peer = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0u8; 4096];
            assert!(socket.read(&mut bytes).await.unwrap() > 0);
            socket.write_all(format!("HTTP/1.1 {status} private-http-status-canary\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        });
        let signal = Cancellation::default();
        let error = match client(&url, caps(64, 64, 64))
            .connect(&url, headers(), original(&signal, Duration::from_secs(2)))
            .await
        {
            Err(error) => error,
            Ok(_) => panic!("error HTTP response upgraded"),
        };
        assert!(
            matches!(&error, WebSocketError::Sdk(cause) if cause.kind() == WebSocketFailureKind::Handshake && cause.http_status() == Some(status))
        );
        assert!(format!("{error}").contains(&format!("status: {status}")));
        let mut source: Option<&dyn Error> = Some(&error);
        while let Some(error) = source {
            assert!(!format!("{error:?} {error}").contains("private-http-status-canary"));
            source = error.source();
        }
        peer.await.unwrap();
    }
}

#[test]
fn sdk_diagnostics_identify_io_and_reqwest_without_exposing_retained_details() {
    let private = "private-sdk-diagnostic-canary";
    let io_error = WebSocketError::from(reqwest_websocket::Error::Tungstenite(
        tungstenite::Error::Io(std::io::Error::other(private)),
    ));
    let reqwest_error = reqwest::Client::builder()
        .user_agent("\nprivate-sdk-diagnostic-canary")
        .build()
        .unwrap_err()
        .with_url(
            reqwest::Url::parse("https://private.invalid/path?token=private-sdk-diagnostic-canary")
                .unwrap(),
        );
    let reqwest_error = WebSocketError::from(reqwest_websocket::Error::Reqwest(reqwest_error));
    for (error, sdk, reason) in [
        (io_error, "tungstenite", "io"),
        (reqwest_error, "reqwest", "builder"),
    ] {
        assert!(
            matches!(&error, WebSocketError::Sdk(cause) if cause.kind() == WebSocketFailureKind::Transport && cause.http_status().is_none())
        );
        let diagnostic = format!("{error:?} {error}");
        assert!(diagnostic.contains(&format!("sdk: {sdk:?}")));
        assert!(diagnostic.contains(&format!("reason: {reason:?}")));
        if sdk == "tungstenite" {
            assert!(diagnostic.contains("io_kind: Other"));
        }
        let mut cause: Option<&dyn Error> = Some(&error);
        while let Some(error) = cause {
            let diagnostic = format!("{error:?} {error}");
            assert!(!diagnostic.contains(private));
            assert!(!diagnostic.contains("private.invalid"));
            cause = error.source();
        }
    }
}

#[tokio::test]
#[expect(
    clippy::result_large_err,
    reason = "SDK handshake callback fixes the unboxed HTTP error response ABI"
)]
async fn exact_native_header_budget_includes_host_accept_and_sdk_upgrade_headers() {
    let (listener, url) = listen().await;
    let (measured_tx, measured_rx) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let mut measured_tx = Some(measured_tx);
        for _ in 0..2 {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_hdr_async(
                socket,
                |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                    assert!(request.headers().contains_key(header::HOST));
                    assert_eq!(request.headers()[header::ACCEPT], "*/*");
                    if let Some(tx) = measured_tx.take() {
                        tx.send((
                            request.headers().len(),
                            request
                                .headers()
                                .iter()
                                .map(|(name, value)| name.as_str().len() + value.as_bytes().len())
                                .sum::<usize>(),
                        ))
                        .unwrap();
                    }
                    Ok(response)
                },
            )
            .await
            .unwrap();
            assert!(matches!(
                socket.next().await,
                Some(Ok(PeerMessage::Close(_)))
            ));
        }
        listener
    });
    let signal = Cancellation::default();
    let first = client(&url, caps(64, 64, 64))
        .connect(
            &url,
            HeaderMap::new(),
            original(&signal, Duration::from_secs(3)),
        )
        .await
        .unwrap();
    let (count, bytes) = measured_rx.await.unwrap();
    first.close(CloseCode::Normal).await.unwrap();
    let mut exact = network();
    exact.header_count = count;
    exact.header_bytes = bytes;
    let policy = || ClientPolicy {
        origin: &url,
        allow_loopback_http: true,
        proxy: None,
        additional_roots: &[],
    };
    WebSocketClient::new(policy(), exact, caps(64, 64, 64))
        .unwrap()
        .connect(
            &url,
            HeaderMap::new(),
            original(&signal, Duration::from_secs(3)),
        )
        .await
        .unwrap()
        .close(CloseCode::Normal)
        .await
        .unwrap();
    let listener = peer.await.unwrap();
    for by_count in [true, false] {
        let mut too_small = exact;
        if by_count {
            too_small.header_count -= 1;
        } else {
            too_small.header_bytes -= 1;
        }
        let result = WebSocketClient::new(policy(), too_small, caps(64, 64, 64))
            .unwrap()
            .connect(
                &url,
                HeaderMap::new(),
                original(&signal, Duration::from_secs(3)),
            )
            .await;
        assert!(matches!(
            result,
            Err(WebSocketError::Network(NetworkError::Limit))
        ));
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(20), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn tls_uses_captured_roots_and_still_rejects_wrong_hostname() {
    use std::sync::Arc;
    use tokio_rustls::{
        TlsAcceptor,
        rustls::{ServerConfig, pki_types::PrivatePkcs8KeyDer},
    };
    for (name, trust, success) in [
        ("127.0.0.1", true, true),
        ("127.0.0.1", false, false),
        ("wrong.example.test", true, false),
    ] {
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec![name.to_owned()]).unwrap();
        let root = cert.der().to_vec();
        let config = ServerConfig::builder_with_provider(Arc::new(
            tokio_rustls::rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.der().clone()],
            PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
        )
        .unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let (listener, plain) = listen().await;
        let url = plain.replacen("http:", "https:", 1);
        let peer = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            if let Ok(socket) = acceptor.accept(socket).await {
                let mut socket = accept_async(socket).await.unwrap();
                socket
                    .send(PeerMessage::Text("trusted TLS".into()))
                    .await
                    .unwrap();
            }
        });
        let roots: Vec<&[u8]> = if trust { vec![&root] } else { Vec::new() };
        let client = WebSocketClient::new(
            ClientPolicy {
                origin: &url,
                allow_loopback_http: false,
                proxy: None,
                additional_roots: &roots,
            },
            network(),
            caps(64, 64, 64),
        )
        .unwrap();
        let signal = Cancellation::default();
        let result = client
            .connect(
                &url,
                HeaderMap::new(),
                original(&signal, Duration::from_secs(3)),
            )
            .await;
        if success {
            match result
                .unwrap()
                .receive_until(Instant::now() + Duration::from_secs(1))
                .await
                .unwrap()
            {
                WebSocketRead::Message(Message::Text(text)) => assert_eq!(text, "trusted TLS"),
                _ => panic!("missing TLS text"),
            }
        } else {
            match result {
                Err(error) => sdk_kind(error, WebSocketFailureKind::Transport),
                Ok(_) => panic!("untrusted TLS must remain rejected"),
            }
        }
        peer.await.unwrap();
    }
}

#[tokio::test]
#[expect(
    clippy::result_large_err,
    reason = "SDK handshake callback fixes the unboxed HTTP error response ABI"
)]
async fn explicit_proxy_carries_the_actual_upgrade_and_consuming_close() {
    let (listener, proxy_url) = listen().await;
    let peer = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_hdr_async(
            socket,
            |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                assert_eq!(request.uri().to_string(), "http://127.0.0.1:1/v1/realtime");
                Ok(response)
            },
        )
        .await
        .unwrap();
        socket
            .send(PeerMessage::Text("captured proxy".into()))
            .await
            .unwrap();
        assert!(
            matches!(socket.next().await.unwrap().unwrap(), PeerMessage::Close(Some(frame)) if frame.code == 1000.into())
        );
    });
    let url = "http://127.0.0.1:1/v1/realtime";
    let client = WebSocketClient::new(
        ClientPolicy {
            origin: url,
            allow_loopback_http: true,
            proxy: Some(&proxy_url),
            additional_roots: &[],
        },
        network(),
        caps(64, 64, 64),
    )
    .unwrap();
    let signal = Cancellation::default();
    let mut socket = client
        .connect(url, headers(), original(&signal, Duration::from_secs(3)))
        .await
        .unwrap();
    match socket
        .receive_until(Instant::now() + Duration::from_secs(1))
        .await
        .unwrap()
    {
        WebSocketRead::Message(Message::Text(text)) => assert_eq!(text, "captured proxy"),
        _ => panic!("proxy not used"),
    }
    socket.close(CloseCode::Normal).await.unwrap();
    peer.await.unwrap();
}

#[tokio::test]
async fn ambient_proxy_isolation_uses_child_environment_only() {
    let (listener, url) = listen().await;
    let (proxy, proxy_url) = listen().await;
    let peer = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(socket).await.unwrap();
        socket
            .send(PeerMessage::Text("direct origin".into()))
            .await
            .unwrap();
    });
    let result = tokio::task::spawn_blocking(move || {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--ignored",
                "--exact",
                "ambient_proxy_child_probe",
                "--test-threads=1",
            ])
            .env("GRAPHIT_WS_TEST_ORIGIN", url)
            .env("NO_PROXY", "")
            .env("no_proxy", "");
        for key in [
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
        ] {
            command.env(key, &proxy_url);
        }
        command.output().unwrap()
    })
    .await
    .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    peer.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(20), proxy.accept())
            .await
            .is_err()
    );
}

#[test]
#[ignore = "executed by the child-only ambient proxy isolation test"]
fn ambient_proxy_child_probe() {
    let url = std::env::var("GRAPHIT_WS_TEST_ORIGIN").unwrap();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let signal = Cancellation::default();
            let mut socket = client(&url, caps(64, 64, 64))
                .connect(
                    &url,
                    HeaderMap::new(),
                    original(&signal, Duration::from_secs(3)),
                )
                .await
                .unwrap();
            match socket
                .receive_until(Instant::now() + Duration::from_secs(1))
                .await
                .unwrap()
            {
                WebSocketRead::Message(Message::Text(text)) => assert_eq!(text, "direct origin"),
                _ => panic!("ambient proxy captured"),
            }
        });
}

#[tokio::test]
async fn abrupt_tcp_drop_and_reserved_opcode_have_distinct_terminal_causes() {
    for invalid_frame in [false, true] {
        let (listener, url) = listen().await;
        let peer = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(socket).await.unwrap();
            assert_eq!(
                socket.next().await.unwrap().unwrap().into_text().unwrap(),
                "ready"
            );
            if invalid_frame {
                // Reserved data opcode3, no mask: invalid even for an empty server frame.
                socket.get_mut().write_all(&[0x83, 0x00]).await.unwrap();
                socket.get_mut().flush().await.unwrap();
            }
            // Drop actual TCP without a WebSocket Close frame.
        });
        let stop = Cancellation::default();
        let mut socket = client(&url, caps(64, 64, 64))
            .connect(&url, headers(), original(&stop, Duration::from_secs(5)))
            .await
            .unwrap();
        socket.send(Message::Text("ready".into())).await.unwrap();
        let error = match socket.receive().await {
            Err(error) => error,
            Ok(_) => panic!("peer must end the channel"),
        };
        sdk_kind(
            error,
            if invalid_frame {
                WebSocketFailureKind::Protocol
            } else {
                WebSocketFailureKind::Closed
            },
        );
        assert!(matches!(
            socket.receive().await,
            Err(WebSocketError::Closed)
        ));
        peer.await.unwrap();
    }
}
#[tokio::test]
async fn refused_connection_is_closed_without_turning_invalid_handshake_into_a_disconnect() {
    let (listener, url) = listen().await;
    drop(listener);
    let stop = Cancellation::default();
    let error = match client(&url, caps(64, 64, 64))
        .connect(&url, headers(), original(&stop, Duration::from_secs(5)))
        .await
    {
        Err(error) => error,
        Ok(_) => panic!("the actual listener is closed"),
    };
    sdk_kind(error, WebSocketFailureKind::Closed);
}
