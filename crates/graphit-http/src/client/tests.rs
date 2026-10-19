use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::{JoinHandle, JoinSet},
};

#[derive(Default)]
struct StreamSink {
    heads: usize,
    chunks: usize,
    status: Option<StatusCode>,
    bytes: Vec<u8>,
    reject_head: bool,
    fail_after_bytes: Option<usize>,
    after_head: Option<Box<dyn FnMut()>>,
    after_chunk: Option<Box<dyn FnMut(usize)>>,
}
impl NetworkResponseSink for StreamSink {
    type Error = GateDenied;

    fn head(&mut self, status: StatusCode, headers: &HeaderMap) -> Result<(), GateDenied> {
        self.heads += 1;
        self.status = Some(status);
        assert!(headers.values().all(HeaderValue::is_sensitive));
        if let Some(action) = &mut self.after_head {
            action();
        }
        if self.reject_head {
            Err(GateDenied("private-head-canary"))
        } else {
            Ok(())
        }
    }
    fn chunk(&mut self, bytes: &[u8]) -> Result<(), GateDenied> {
        assert_eq!(self.heads, 1);
        self.chunks += 1;
        let failed = self
            .fail_after_bytes
            .is_some_and(|limit| self.bytes.len() >= limit);
        self.bytes.extend_from_slice(if failed {
            &bytes[..bytes.len().min(1)]
        } else {
            bytes
        });
        if let Some(action) = &mut self.after_chunk {
            action(self.bytes.len());
        }
        if failed {
            Err(GateDenied("private-chunk-canary"))
        } else {
            Ok(())
        }
    }
}
fn stream_gate() -> Result<(), GateDenied> {
    Ok(())
}
async fn two_stream_chunks(release: Arc<Semaphore>) -> Fixture {
    Fixture::script(vec![
        FixtureStep::Bytes(
            Duration::ZERO,
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n".to_vec(),
        ),
        FixtureStep::Wait(release),
        FixtureStep::Bytes(Duration::ZERO, b"3\r\ndef\r\n0\r\n\r\n".to_vec()),
    ])
    .await
}

#[tokio::test]
async fn stream_head_precedes_ordered_chunks_and_rejects_before_writing() {
    let release = Arc::new(Semaphore::new(0));
    let fixture = two_stream_chunks(release.clone()).await;
    let mut sink = StreamSink {
        after_chunk: Some(Box::new(move |bytes| {
            if bytes >= 3 {
                release.add_permits(1);
            }
        })),
        ..StreamSink::default()
    };
    let cancellation = Cancellation::default();
    let result = client(&fixture.url, limits())
        .execute_stream_guarded(
            get(&fixture.url, &HeaderMap::new()),
            context(&cancellation),
            &mut stream_gate,
            &mut sink,
        )
        .await
        .unwrap();
    assert_eq!(result.status(), StatusCode::OK);
    assert_eq!(result.accepted_bytes(), 6);
    assert_eq!(sink.bytes, b"abcdef");
    assert_eq!(sink.heads, 1);
    assert!(sink.chunks >= 2);

    let fixture =
        Fixture::response("HTTP/1.1 503 Unavailable\r\nContent-Length: 4\r\n\r\noops").await;
    let mut sink = StreamSink {
        reject_head: true,
        ..StreamSink::default()
    };
    let result = client(&fixture.url, limits())
        .execute_stream_guarded(
            get(&fixture.url, &HeaderMap::new()),
            context(&cancellation),
            &mut stream_gate,
            &mut sink,
        )
        .await;
    assert!(matches!(
        result,
        Err(NetworkStreamError::Sink {
            accepted_bytes: 0,
            effect: NetworkEffect::MayHaveApplied,
            ..
        })
    ));
    assert_eq!(sink.status, Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(sink.chunks, 0);
    assert!(sink.bytes.is_empty());
}

#[tokio::test]
async fn stream_body_and_content_length_limits_precede_sink_effects() {
    for capacity in [5, 6] {
        let release = Arc::new(Semaphore::new(0));
        let fixture = two_stream_chunks(release.clone()).await;
        let mut sink = StreamSink {
            after_chunk: Some(Box::new(move |bytes| {
                if bytes >= 3 {
                    release.add_permits(1);
                }
            })),
            ..StreamSink::default()
        };
        let cancellation = Cancellation::default();
        let mut policy = limits();
        policy.response_body_bytes = capacity;
        let result = client(&fixture.url, policy)
            .execute_stream_guarded(
                get(&fixture.url, &HeaderMap::new()),
                context(&cancellation),
                &mut stream_gate,
                &mut sink,
            )
            .await;
        if capacity == 5 {
            assert!(matches!(
                result,
                Err(NetworkStreamError::Network {
                    source: NetworkError::Limit,
                    accepted_bytes,
                    ..
                }) if accepted_bytes == sink.bytes.len()
            ));
            assert!((3..=5).contains(&sink.bytes.len()));
            assert_eq!(sink.bytes, &b"abcdef"[..sink.bytes.len()]);
        } else {
            assert_eq!(result.unwrap().accepted_bytes(), 6);
            assert_eq!(sink.bytes, b"abcdef");
        }
    }
    let fixture = Fixture::response("HTTP/1.1 200 OK\r\nContent-Length: 1025\r\n\r\n").await;
    let cancellation = Cancellation::default();
    let mut sink = StreamSink::default();
    let result = client(&fixture.url, limits())
        .execute_stream_guarded(
            get(&fixture.url, &HeaderMap::new()),
            context(&cancellation),
            &mut stream_gate,
            &mut sink,
        )
        .await;
    assert!(matches!(
        result,
        Err(NetworkStreamError::Network {
            source: NetworkError::Limit,
            accepted_bytes: 0,
            ..
        })
    ));
    assert_eq!((sink.heads, sink.chunks), (0, 0));
}

#[tokio::test]
async fn stream_sink_failure_preserves_partial_effect_and_first_typed_cause() {
    use std::error::Error;
    let release = Arc::new(Semaphore::new(0));
    let fixture = two_stream_chunks(release.clone()).await;
    let valid = Arc::new(AtomicBool::new(true));
    let revoke = valid.clone();
    let mut sink = StreamSink {
        fail_after_bytes: Some(3),
        after_chunk: Some(Box::new(move |bytes| {
            if bytes > 3 {
                revoke.store(false, Ordering::SeqCst);
            }
            if bytes >= 3 {
                release.add_permits(1);
            }
        })),
        ..StreamSink::default()
    };
    let mut gate = || {
        if valid.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(GateDenied("private-admission-canary"))
        }
    };
    let cancellation = Cancellation::default();
    let result = client(&fixture.url, limits())
        .execute_stream_guarded(
            get(&fixture.url, &HeaderMap::new()),
            context(&cancellation),
            &mut gate,
            &mut sink,
        )
        .await;
    let error = result.err().unwrap();
    assert!(matches!(
        error,
        NetworkStreamError::Sink {
            accepted_bytes: 3,
            ..
        }
    ));
    assert_eq!(error.effect(), NetworkEffect::MayHaveApplied);
    assert_eq!(sink.bytes, b"abcd"); // The rejected chunk wrote one byte.
    assert_eq!(error.accepted_bytes(), 3);
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<GateDenied>()
            .is_some()
    );
    for diagnostic in [format!("{error:?}"), error.to_string()] {
        assert!(!diagnostic.contains("private-"));
    }
}

#[tokio::test]
async fn stream_pre_send_admission_denials_have_no_socket_or_sink_effects() {
    for pre_send in [false, true] {
        let fixture = Fixture::response("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
        let valid = Arc::new(AtomicBool::new(pre_send));
        let mut transport = client(&fixture.url, limits());
        if pre_send {
            let revoke = valid.clone();
            transport.before_send = Some(Box::new(move || revoke.store(false, Ordering::SeqCst)));
        }
        let mut gate = || {
            if valid.load(Ordering::SeqCst) {
                Ok(())
            } else {
                Err(GateDenied("denied"))
            }
        };
        let mut sink = StreamSink::default();
        let cancellation = Cancellation::default();
        let result = transport
            .execute_stream_guarded(
                get(&fixture.url, &HeaderMap::new()),
                context(&cancellation),
                &mut gate,
                &mut sink,
            )
            .await;
        assert!(matches!(
            result,
            Err(NetworkStreamError::Admission {
                effect: NetworkEffect::NotSent,
                accepted_bytes: 0,
                ..
            })
        ));
        assert_eq!((sink.heads, sink.chunks), (0, 0));
        assert_eq!(fixture.socket_count.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn stream_successful_callbacks_cannot_hide_original_cancel_or_deadline() {
    for at_head in [false, true] {
        for cancel in [false, true] {
            let fixture =
                Fixture::response("HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabc").await;
            let cancellation = Cancellation::default();
            let deadline = Instant::now() + Duration::from_secs(1);
            let original = NetworkRequestContext::new(deadline, &cancellation);
            let signal = cancellation.clone();
            let mut action: Box<dyn FnMut()> = Box::new(move || {
                if cancel {
                    signal.cancel();
                } else {
                    std::thread::sleep(
                        deadline.saturating_duration_since(Instant::now())
                            + Duration::from_millis(1),
                    );
                }
            });
            let mut sink = StreamSink::default();
            if at_head {
                sink.after_head = Some(action);
            } else {
                sink.after_chunk = Some(Box::new(move |_| action()));
            }
            let result = client(&fixture.url, limits())
                .execute_stream_guarded(
                    get(&fixture.url, &HeaderMap::new()),
                    original,
                    &mut stream_gate,
                    &mut sink,
                )
                .await;
            assert!(
                matches!(result, Err(NetworkStreamError::Network { source, effect: NetworkEffect::MayHaveApplied, .. }) if source == if cancel { NetworkError::Cancelled } else { NetworkError::Deadline })
            );
            assert_eq!(sink.heads, 1);
            assert_eq!(sink.chunks, usize::from(!at_head));
        }
    }
}

#[tokio::test]
async fn stream_revocation_after_accepted_chunk_stops_following_delivery() {
    let release = Arc::new(Semaphore::new(0));
    let fixture = two_stream_chunks(release.clone()).await;
    let valid = Arc::new(AtomicBool::new(true));
    let revoke = valid.clone();
    let mut sink = StreamSink {
        after_chunk: Some(Box::new(move |bytes| {
            if bytes >= 3 {
                revoke.store(false, Ordering::SeqCst);
                release.add_permits(1);
            }
        })),
        ..StreamSink::default()
    };
    let mut gate = || {
        if valid.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(GateDenied("revoked"))
        }
    };
    let cancellation = Cancellation::default();
    let result = client(&fixture.url, limits())
        .execute_stream_guarded(
            get(&fixture.url, &HeaderMap::new()),
            context(&cancellation),
            &mut gate,
            &mut sink,
        )
        .await;
    assert!(matches!(
        result,
        Err(NetworkStreamError::Admission {
            accepted_bytes: 3,
            effect: NetworkEffect::MayHaveApplied,
            ..
        })
    ));
    assert_eq!(sink.bytes, b"abc");
    assert!(sink.chunks >= 1);
}

#[tokio::test]
async fn stream_transport_failure_after_chunk_preserves_network_cause() {
    let release = Arc::new(Semaphore::new(0));
    let fixture = Fixture::script(vec![
        FixtureStep::Bytes(
            Duration::ZERO,
            b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nabc".to_vec(),
        ),
        FixtureStep::Wait(release.clone()),
    ])
    .await;
    let mut sink = StreamSink {
        after_chunk: Some(Box::new(move |bytes| {
            if bytes >= 3 {
                release.add_permits(1);
            }
        })),
        ..StreamSink::default()
    };
    let cancellation = Cancellation::default();
    let result = client(&fixture.url, limits())
        .execute_stream_guarded(
            get(&fixture.url, &HeaderMap::new()),
            context(&cancellation),
            &mut stream_gate,
            &mut sink,
        )
        .await;
    assert!(matches!(
        result,
        Err(NetworkStreamError::Network {
            source: NetworkError::Connection,
            accepted_bytes: 3,
            effect: NetworkEffect::MayHaveApplied
        })
    ));
    assert_eq!(sink.bytes, b"abc");
}

#[tokio::test]
async fn stream_cancel_deadline_and_future_drop_release_busy_capacity() {
    for mode in ["cancel", "deadline", "drop"] {
        let seen = Arc::new(Semaphore::new(0));
        let release = Arc::new(Semaphore::new(0));
        let fixture = two_stream_chunks(release.clone()).await;
        let mut policy = limits();
        policy.max_in_flight = 1;
        let transport = client(&fixture.url, policy);
        let cancellation = Cancellation::default();
        let deadline = Instant::now() + Duration::from_secs(1);
        let notify = seen.clone();
        let mut sink = StreamSink {
            after_chunk: Some(Box::new(move |bytes| {
                if bytes >= 3 {
                    notify.add_permits(1);
                }
            })),
            ..StreamSink::default()
        };
        let headers = HeaderMap::new();
        let mut gate = stream_gate;
        let mut pending = Box::pin(transport.execute_stream_guarded(
            get(&fixture.url, &headers),
            NetworkRequestContext::new(deadline, &cancellation),
            &mut gate,
            &mut sink,
        ));
        tokio::select! {
            _ = seen.acquire() => {},
            result = &mut pending => panic!("stream completed before barrier: {}", result.is_ok()),
        }
        let peer_signal = Cancellation::default();
        let mut peer_sink = StreamSink::default();
        let busy = transport
            .execute_stream_guarded(
                get(&fixture.url, &headers),
                context(&peer_signal),
                &mut stream_gate,
                &mut peer_sink,
            )
            .await;
        assert!(matches!(
            busy,
            Err(NetworkStreamError::Network {
                source: NetworkError::Busy,
                effect: NetworkEffect::NotSent,
                accepted_bytes: 0
            })
        ));
        if mode == "drop" {
            drop(pending);
        } else {
            if mode == "cancel" {
                cancellation.cancel();
            }
            let error = pending.await.err().unwrap();
            assert!(
                matches!(error, NetworkStreamError::Network { source, accepted_bytes: 3, effect: NetworkEffect::MayHaveApplied } if source == if mode == "cancel" { NetworkError::Cancelled } else { NetworkError::Deadline })
            );
        }
        assert_eq!(sink.bytes, b"abc");
        release.add_permits(2); // Release the discarded request and the subsequent one.
        let mut next_sink = StreamSink::default();
        let next = transport
            .execute_stream_guarded(
                get(&fixture.url, &headers),
                context(&peer_signal),
                &mut stream_gate,
                &mut next_sink,
            )
            .await
            .unwrap();
        assert_eq!(next.accepted_bytes(), 6);
        assert_eq!(next_sink.bytes, b"abcdef");
    }
}

#[tokio::test]
async fn stream_counting_sink_consumes_large_body_and_reuses_connection() {
    struct Counter {
        heads: usize,
        bytes: usize,
        chunks: usize,
    }
    impl NetworkResponseSink for Counter {
        type Error = GateDenied;
        fn head(&mut self, status: StatusCode, _: &HeaderMap) -> Result<(), GateDenied> {
            assert_eq!(status, StatusCode::OK);
            self.heads += 1;
            Ok(())
        }
        fn chunk(&mut self, bytes: &[u8]) -> Result<(), GateDenied> {
            assert!(bytes.iter().all(|byte| *byte == b'x'));
            self.bytes += bytes.len();
            self.chunks += 1;
            Ok(())
        }
    }
    const BYTES: usize = 2 * 1024 * 1024;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let block = [b'x'; 64 * 1024];
        for _ in 0..2 {
            assert!(!read_request(&mut socket).await.is_empty());
            socket
                .write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {BYTES}\r\n\r\n").as_bytes())
                .await
                .unwrap();
            for _ in 0..32 {
                socket.write_all(&block).await.unwrap();
            }
        }
    });
    let mut policy = limits();
    policy.response_body_bytes = BYTES;
    let transport = client(&url, policy);
    let cancellation = Cancellation::default();
    for _ in 0..2 {
        let mut sink = Counter {
            heads: 0,
            bytes: 0,
            chunks: 0,
        };
        let response = transport
            .execute_stream_guarded(
                get(&url, &HeaderMap::new()),
                context(&cancellation),
                &mut stream_gate,
                &mut sink,
            )
            .await
            .unwrap();
        assert_eq!(
            (response.accepted_bytes(), sink.bytes, sink.heads),
            (BYTES, BYTES, 1)
        );
        assert!(sink.chunks > 1);
    }
    server.await.unwrap();
}

fn limits() -> Limits {
    Limits {
        request_body_bytes: 1024,
        response_body_bytes: 1024,
        url_bytes: 2048,
        header_bytes: 4096,
        header_count: 32,
        max_in_flight: 4,
        max_idle_per_host: 2,
        idle_timeout: Duration::from_secs(5),
        max_certificates: 4,
        max_certificate_bytes: 16384,
    }
}
fn client(url: &str, limits: Limits) -> BoundedClient {
    BoundedClient::new(
        ClientPolicy {
            origin: url,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        limits,
    )
    .unwrap()
}
fn context(cancellation: &Cancellation) -> NetworkRequestContext<'_> {
    NetworkRequestContext::new(Instant::now() + Duration::from_secs(3), cancellation)
}
fn get<'a>(url: &'a str, headers: &'a HeaderMap) -> NetworkRequest<'a> {
    NetworkRequest {
        method: Method::GET,
        url,
        headers,
        body: &[],
    }
}
fn error(result: Result<NetworkResponse, NetworkError>, expected: NetworkError) {
    assert_eq!(result.err(), Some(expected));
}
async fn read_request(socket: &mut (impl AsyncRead + Unpin)) -> Vec<u8> {
    let mut bytes = Vec::new();
    while let Ok(byte) = socket.read_u8().await {
        bytes.push(byte);
        assert!(bytes.len() < 16384);
        if bytes.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    bytes
}

#[derive(Clone)]
enum FixtureStep {
    Bytes(Duration, Vec<u8>),
    Wait(Arc<Semaphore>),
    Signal(Arc<Semaphore>),
    Revoke(Arc<AtomicBool>),
}

struct Fixture {
    url: String,
    task: JoinHandle<()>,
    accepted: Arc<Semaphore>,
    count: Arc<AtomicUsize>,
    socket_count: Arc<AtomicUsize>,
    request: Arc<std::sync::Mutex<Vec<u8>>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Fixture {
    async fn new(pieces: Vec<(Duration, Vec<u8>)>) -> Self {
        Self::script(
            pieces
                .into_iter()
                .map(|(delay, bytes)| FixtureStep::Bytes(delay, bytes))
                .collect(),
        )
        .await
    }
    async fn script(pieces: Vec<FixtureStep>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let accepted = Arc::new(Semaphore::new(0));
        let count = Arc::new(AtomicUsize::new(0));
        let socket_count = Arc::new(AtomicUsize::new(0));
        let request = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (signal, calls, captured) = (accepted.clone(), count.clone(), request.clone());
        let sockets = socket_count.clone();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        let (mut socket, _) = result.unwrap();
                        sockets.fetch_add(1, Ordering::SeqCst);
                        let (signal, calls, captured, pieces) = (signal.clone(), calls.clone(), captured.clone(), pieces.clone());
                        connections.spawn(async move {
                            let bytes = read_request(&mut socket).await;
                            if bytes.is_empty() { return; }
                            *captured.lock().unwrap() = bytes;
                            calls.fetch_add(1, Ordering::SeqCst);
                            signal.add_permits(1);
                            if pieces.is_empty() {
                                let _ = socket.read_u8().await;
                            }
                            for piece in pieces {
                                match piece {
                                    FixtureStep::Bytes(delay, bytes) => {
                                        if !delay.is_zero() { tokio::time::sleep(delay).await; }
                                        if socket.write_all(&bytes).await.is_err() { break; }
                                    }
                                    FixtureStep::Wait(signal) => { signal.acquire().await.unwrap().forget(); }
                                    FixtureStep::Signal(signal) => signal.add_permits(1),
                                    FixtureStep::Revoke(valid) => valid.store(false, Ordering::SeqCst),
                                }
                            }
                        });
                    }
                    _ = connections.join_next(), if !connections.is_empty() => {}
                }
            }
        });
        Self {
            url,
            task,
            accepted,
            count,
            socket_count,
            request,
        }
    }
    async fn response(response: &str) -> Self {
        Self::new(vec![(Duration::ZERO, response.as_bytes().to_vec())]).await
    }
    async fn seen(&self) {
        self.accepted.acquire().await.unwrap().forget();
    }
}

#[tokio::test]
async fn exact_body_limits_and_streaming_overflow_never_return_partial() {
    for (wire, capacity, expected) in [
        ("HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\ntest", 4, None),
        (
            "HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\ntest",
            3,
            Some(NetworkError::Limit),
        ),
        (
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nte\r\n2\r\nst\r\n0\r\n\r\n",
            4,
            None,
        ),
        (
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nte\r\n2\r\nst\r\n0\r\n\r\n",
            3,
            Some(NetworkError::Limit),
        ),
        ("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\ntest", 4, None),
        (
            "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\ntest",
            3,
            Some(NetworkError::Limit),
        ),
    ] {
        let fixture = Fixture::response(wire).await;
        let mut policy = limits();
        policy.response_body_bytes = capacity;
        let cancellation = Cancellation::default();
        let result = client(&fixture.url, policy)
            .execute(get(&fixture.url, &HeaderMap::new()), context(&cancellation))
            .await;
        if let Some(expected) = expected {
            error(result, expected);
        } else {
            assert_eq!(result.unwrap().into_body().as_slice(), b"test");
        }
    }
}

#[tokio::test]
async fn redirects_and_all_encoding_values_remain_under_caller_control() {
    let sink = Fixture::response("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
    let redirect = Fixture::response(&format!(
        "HTTP/1.1 302 Found\r\nContent-Length: 0\r\nLocation: {}/canary\r\n\r\n",
        sink.url
    ))
    .await;
    let cancellation = Cancellation::default();
    let response = client(&redirect.url, limits())
        .execute(
            get(&redirect.url, &HeaderMap::new()),
            context(&cancellation),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(sink.count.load(Ordering::SeqCst), 0);
    for encoding in [
        "gzip",
        "br",
        "identity, gzip",
        "identity\r\nContent-Encoding: gzip",
    ] {
        let fixture = Fixture::response(&format!(
            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nContent-Encoding: {encoding}\r\n\r\n"
        ))
        .await;
        error(
            client(&fixture.url, limits())
                .execute(get(&fixture.url, &HeaderMap::new()), context(&cancellation))
                .await,
            NetworkError::Encoding,
        );
    }
}

#[tokio::test]
async fn original_signal_deadline_busy_and_future_drop_release_capacity() {
    let stalled = Fixture::new(Vec::new()).await;
    let mut policy = limits();
    policy.max_in_flight = 1;
    let client = Arc::new(client(&stalled.url, policy));
    let cancellation = Cancellation::default();
    let child = cancellation.clone();
    let cloned = client.clone();
    let url = stalled.url.clone();
    let first = tokio::spawn(async move {
        cloned
            .execute(
                get(&url, &HeaderMap::new()),
                NetworkRequestContext::service(&child),
            )
            .await
    });
    stalled.seen().await;
    let fresh = Cancellation::default();
    error(
        client
            .execute(get(&stalled.url, &HeaderMap::new()), context(&fresh))
            .await,
        NetworkError::Busy,
    );
    cancellation.cancel();
    error(
        tokio::time::timeout(Duration::from_secs(1), first)
            .await
            .unwrap()
            .unwrap(),
        NetworkError::Cancelled,
    );
    assert_eq!(client.permits.available_permits(), 1);
    let child = fresh.clone();
    let cloned = client.clone();
    let url = stalled.url.clone();
    let second = tokio::spawn(async move {
        cloned
            .execute(get(&url, &HeaderMap::new()), context(&child))
            .await
    });
    stalled.seen().await;
    second.abort();
    let _ = second.await;
    assert_eq!(client.permits.available_permits(), 1);
    let headers = HeaderMap::new();
    error(
        client
            .execute(
                get(&stalled.url, &headers),
                NetworkRequestContext::new(Instant::now() + Duration::from_millis(50), &fresh),
            )
            .await,
        NetworkError::Deadline,
    );
    assert_eq!(client.permits.available_permits(), 1);
    let before = stalled.count.load(Ordering::SeqCst);
    error(
        client
            .execute(get(&stalled.url, &headers), context(&cancellation))
            .await,
        NetworkError::Cancelled,
    );
    error(
        client
            .execute(
                get(&stalled.url, &headers),
                NetworkRequestContext::new(Instant::now(), &fresh),
            )
            .await,
        NetworkError::Deadline,
    );
    assert_eq!(stalled.count.load(Ordering::SeqCst), before);

    let chunks = Fixture::new(vec![
        (
            Duration::ZERO,
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec(),
        ),
        (Duration::from_millis(100), b"1\r\na\r\n".to_vec()),
        (Duration::from_millis(100), b"1\r\nb\r\n".to_vec()),
        (Duration::from_millis(100), b"1\r\nc\r\n0\r\n\r\n".to_vec()),
    ])
    .await;
    error(
        super::tests::client(&chunks.url, limits())
            .execute(
                get(&chunks.url, &headers),
                NetworkRequestContext::new(Instant::now() + Duration::from_millis(250), &fresh),
            )
            .await,
        NetworkError::Deadline,
    );
}

#[tokio::test]
async fn async_waiters_share_permanent_signal_without_lost_wakeups() {
    let signal = Cancellation::default();
    let mut waiters = Vec::new();
    for _ in 0..16 {
        let signal = signal.clone();
        waiters.push(tokio::spawn(async move {
            signal.cancelled().await;
        }));
    }
    tokio::task::yield_now().await;
    std::thread::scope(|scope| {
        scope.spawn(|| signal.cancel()).join().unwrap();
    });
    for waiter in waiters {
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .unwrap()
            .unwrap();
    }
    tokio::time::timeout(Duration::from_secs(1), signal.cancelled())
        .await
        .unwrap();
    for _ in 0..128 {
        let signal = Cancellation::default();
        let cloned = signal.clone();
        let waiter = tokio::spawn(async move {
            cloned.cancelled().await;
        });
        signal.cancel();
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn cancellation_and_deadline_during_preparation_prevent_socket_effects() {
    let fixture = Fixture::response("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
    let cancellation = Cancellation::default();
    let signal = cancellation.clone();
    let mut client = client(&fixture.url, limits());
    client.before_send = Some(Box::new(move || signal.cancel()));
    error(
        client
            .execute(get(&fixture.url, &HeaderMap::new()), context(&cancellation))
            .await,
        NetworkError::Cancelled,
    );
    assert_eq!(fixture.count.load(Ordering::SeqCst), 0);
    let cancellation = Cancellation::default();
    client.before_send = Some(Box::new(|| std::thread::sleep(Duration::from_millis(30))));
    error(
        client
            .execute(
                get(&fixture.url, &HeaderMap::new()),
                NetworkRequestContext::new(
                    Instant::now() + Duration::from_millis(20),
                    &cancellation,
                ),
            )
            .await,
        NetworkError::Deadline,
    );
    assert_eq!(fixture.count.load(Ordering::SeqCst), 0);
    tokio::task::yield_now().await;
    assert_eq!(fixture.socket_count.load(Ordering::SeqCst), 0);
    assert_eq!(client.permits.available_permits(), limits().max_in_flight);
}

#[tokio::test]
async fn request_bounds_origin_and_managed_headers_fail_before_socket_effects() {
    let fixture = Fixture::response("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
    let client = client(&fixture.url, limits());
    let cancellation = Cancellation::default();
    for (url, expected) in [
        ("http://127.0.0.1:1", NetworkError::Origin),
        ("https://127.0.0.1", NetworkError::Origin),
        ("http://canary@127.0.0.1", NetworkError::InvalidRequest),
        ("http://@127.0.0.1", NetworkError::InvalidRequest),
        ("http://127.0.0.1/#canary", NetworkError::InvalidRequest),
        ("http://127.0.0.1/\ncanary", NetworkError::InvalidRequest),
    ] {
        error(
            client
                .execute(get(url, &HeaderMap::new()), context(&cancellation))
                .await,
            expected,
        );
    }
    let large = vec![b'x'; 1025];
    error(
        client
            .execute(
                NetworkRequest {
                    method: Method::POST,
                    url: &fixture.url,
                    headers: &HeaderMap::new(),
                    body: &large,
                },
                context(&cancellation),
            )
            .await,
        NetworkError::Limit,
    );
    for name in [
        "host",
        "content-length",
        "transfer-encoding",
        "connection",
        "proxy-authorization",
        "proxy-connection",
        "keep-alive",
        "te",
        "trailer",
        "upgrade",
        "accept-encoding",
    ] {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_static("canary"),
        );
        error(
            client
                .execute(get(&fixture.url, &headers), context(&cancellation))
                .await,
            NetworkError::InvalidRequest,
        );
    }
    let mut policy = limits();
    policy.header_count = 3;
    error(
        super::tests::client(&fixture.url, policy)
            .execute(get(&fixture.url, &HeaderMap::new()), context(&cancellation))
            .await,
        NetworkError::Limit,
    );
    assert_eq!(fixture.count.load(Ordering::SeqCst), 0);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer fake-test-canary"),
    );
    let response = client
        .execute(get(&fixture.url, &headers), context(&cancellation))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let captured = fixture.request.lock().unwrap();
    let request = std::str::from_utf8(&captured).unwrap();
    assert!(request.contains("accept-encoding: identity\r\n"));
    assert!(request.contains("authorization: Bearer fake-test-canary\r\n"));
    for error in [
        NetworkError::Origin,
        NetworkError::Transport,
        NetworkError::InvalidRequest,
    ] {
        assert!(!format!("{error:?} {error}").contains("canary"));
    }
}

#[test]
fn invalid_policy_and_bounded_certificates_are_rejected_before_client_birth() {
    for origin in [
        "http://localhost",
        "http://192.0.2.1",
        "ftp://127.0.0.1",
        "https://canary@host",
        "https://host/#canary",
    ] {
        assert_eq!(
            BoundedClient::new(
                ClientPolicy {
                    origin,
                    allow_loopback_http: true,
                    proxy: None,
                    additional_roots: &[]
                },
                limits()
            )
            .err(),
            Some(NetworkError::InvalidPolicy)
        );
    }
    assert!(
        BoundedClient::new(
            ClientPolicy {
                origin: "http://[::1]",
                allow_loopback_http: true,
                proxy: None,
                additional_roots: &[]
            },
            limits()
        )
        .is_ok()
    );
    let mut bad = limits();
    bad.response_body_bytes = usize::MAX;
    assert_eq!(client_result(bad), Some(NetworkError::InvalidPolicy));
    bad = limits();
    bad.header_count = 0;
    assert_eq!(client_result(bad), Some(NetworkError::InvalidPolicy));
    bad = limits();
    bad.max_in_flight = Semaphore::MAX_PERMITS + 1;
    assert_eq!(client_result(bad), Some(NetworkError::InvalidPolicy));
    bad = limits();
    bad.idle_timeout = Duration::MAX;
    assert_eq!(client_result(bad), Some(NetworkError::InvalidPolicy));
    let roots: &[&[u8]] = &[b"fake-root-one", b"fake-root-two"];
    bad = limits();
    bad.max_certificates = 1;
    assert_eq!(
        BoundedClient::new(
            ClientPolicy {
                origin: "https://example.test",
                allow_loopback_http: false,
                proxy: None,
                additional_roots: roots
            },
            bad
        )
        .err(),
        Some(NetworkError::InvalidPolicy)
    );
    bad = limits();
    bad.max_certificate_bytes = 1;
    assert_eq!(
        BoundedClient::new(
            ClientPolicy {
                origin: "https://example.test",
                allow_loopback_http: false,
                proxy: None,
                additional_roots: roots
            },
            bad
        )
        .err(),
        Some(NetworkError::InvalidPolicy)
    );
}
fn client_result(limits: Limits) -> Option<NetworkError> {
    BoundedClient::new(
        ClientPolicy {
            origin: "https://example.test",
            allow_loopback_http: false,
            proxy: None,
            additional_roots: &[],
        },
        limits,
    )
    .err()
}

#[tokio::test]
async fn exact_request_response_header_and_url_budgets() {
    let fixture = Fixture::response("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
    let cancellation = Cancellation::default();
    let headers = HeaderMap::new();
    let mut policy = limits();
    policy.header_count = 4;
    // Four managed headers: host, accept-encoding, accept and content-length.
    let header_bytes = fixture.url.strip_prefix("http://").unwrap().len() + 51;
    policy.header_bytes = header_bytes - 1;
    error(
        client(&fixture.url, policy)
            .execute(get(&fixture.url, &headers), context(&cancellation))
            .await,
        NetworkError::Limit,
    );
    assert_eq!(fixture.count.load(Ordering::SeqCst), 0);
    policy.header_bytes = header_bytes;
    assert!(
        client(&fixture.url, policy)
            .execute(get(&fixture.url, &headers), context(&cancellation))
            .await
            .is_ok()
    );
    policy = limits();
    policy.url_bytes = fixture.url.len() + 1;
    let client = client(&fixture.url, policy);
    error(
        client
            .execute(
                get(&format!("{}/x", fixture.url), &headers),
                context(&cancellation),
            )
            .await,
        NetworkError::Limit,
    );
    assert!(
        client
            .execute(get(&fixture.url, &headers), context(&cancellation))
            .await
            .is_ok()
    );
    let expanded_url = format!("{}/路径", fixture.url);
    let mut policy = limits();
    policy.url_bytes = expanded_url.len();
    let unicode_client = super::tests::client(&fixture.url, policy);
    let before = fixture.count.load(Ordering::SeqCst);
    error(
        unicode_client
            .execute(get(&expanded_url, &headers), context(&cancellation))
            .await,
        NetworkError::Limit,
    );
    assert_eq!(fixture.count.load(Ordering::SeqCst), before);
    assert_eq!(
        BoundedClient::new(
            ClientPolicy {
                origin: &expanded_url,
                allow_loopback_http: true,
                proxy: None,
                additional_roots: &[]
            },
            policy
        )
        .err(),
        Some(NetworkError::InvalidPolicy)
    );
    assert_eq!(
        BoundedClient::new(
            ClientPolicy {
                origin: &fixture.url,
                allow_loopback_http: true,
                proxy: Some(&expanded_url),
                additional_roots: &[]
            },
            policy
        )
        .err(),
        Some(NetworkError::InvalidPolicy)
    );
    let body = vec![b'x'; 1024];
    assert!(
        client
            .execute(
                NetworkRequest {
                    method: Method::POST,
                    url: &fixture.url,
                    headers: &headers,
                    body: &body
                },
                context(&cancellation)
            )
            .await
            .is_ok()
    );
    for capacity in [99, 100] {
        let response = Fixture::response(&format!(
            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nX-Test: {}\r\n\r\n",
            "x".repeat(79)
        ))
        .await;
        let mut policy = limits();
        policy.header_bytes = capacity;
        let result = super::tests::client(&response.url, policy)
            .execute(get(&response.url, &headers), context(&cancellation))
            .await;
        if capacity == 99 {
            error(result, NetworkError::Limit);
        } else {
            assert!(
                result
                    .unwrap()
                    .headers()
                    .values()
                    .all(HeaderValue::is_sensitive)
            );
        }
    }
}

#[tokio::test]
async fn completed_calls_reuse_the_same_origin_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        for _ in 0..2 {
            assert!(!read_request(&mut socket).await.is_empty());
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\nx")
                .await
                .unwrap();
        }
    });
    let client = client(&url, limits());
    let cancellation = Cancellation::default();
    for _ in 0..2 {
        assert_eq!(
            client
                .execute(get(&url, &HeaderMap::new()), context(&cancellation))
                .await
                .unwrap()
                .body(),
            b"x"
        );
    }
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn ambient_proxy_isolation_is_proved_in_a_child_process() {
    let origin = Fixture::response("HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\norigin").await;
    let proxy = Fixture::response("HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nproxy").await;
    let (url, proxy_url) = (origin.url.clone(), proxy.url.clone());
    let result = tokio::task::spawn_blocking(move || {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--ignored",
                "--exact",
                "client::tests::ambient_proxy_child_probe",
            ])
            .env("GRAPHIT_HTTP_TEST_ORIGIN", url)
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
    assert_eq!(origin.count.load(Ordering::SeqCst), 1);
    assert_eq!(proxy.count.load(Ordering::SeqCst), 0);
}

#[test]
#[ignore = "executed by the child-process proxy isolation fixture"]
fn ambient_proxy_child_probe() {
    let url = std::env::var("GRAPHIT_HTTP_TEST_ORIGIN").unwrap();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let cancellation = Cancellation::default();
            let response = client(&url, limits())
                .execute(get(&url, &HeaderMap::new()), context(&cancellation))
                .await
                .unwrap();
            assert_eq!(response.body(), b"origin");
        });
}

#[tokio::test]
async fn incomplete_http_responses_are_connection_failures_without_replaying_the_request() {
    for response in [
        "",
        "HTTP/1.1 200",
        "HTTP/1.1 200 OK\r\nContent-Length: 8\r\n",
        "HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\nshort",
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n8\r\nshort",
    ] {
        let fixture = Fixture::response(response).await;
        let cancellation = Cancellation::default();
        network_error(
            client(&fixture.url, limits())
                .execute_guarded(
                    NetworkRequest {
                        method: Method::PUT,
                        url: &fixture.url,
                        headers: &HeaderMap::new(),
                        body: &[],
                    },
                    context(&cancellation),
                    &mut stream_gate,
                )
                .await,
            NetworkError::Connection,
            NetworkEffect::MayHaveApplied,
        );
        assert_eq!(fixture.count.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn malformed_http_headers_and_chunk_frames_remain_terminal_transport_failures() {
    for response in [
        "not an HTTP response",
        "HTTP/1.1 200 OK\r\nContent-Length: invalid\r\n\r\n",
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\ninvalid-size\r\n",
    ] {
        let fixture = Fixture::response(response).await;
        let cancellation = Cancellation::default();
        network_error(
            client(&fixture.url, limits())
                .execute_guarded(
                    get(&fixture.url, &HeaderMap::new()),
                    context(&cancellation),
                    &mut stream_gate,
                )
                .await,
            NetworkError::Transport,
            NetworkEffect::MayHaveApplied,
        );
        assert_eq!(fixture.count.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn connection_refused_is_distinct_from_invalid_http_response() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let cancellation = Cancellation::default();
    error(
        client(&url, limits())
            .execute(get(&url, &HeaderMap::new()), context(&cancellation))
            .await,
        NetworkError::Connection,
    );
    let fixture = Fixture::response("not an HTTP response").await;
    error(
        client(&fixture.url, limits())
            .execute(get(&fixture.url, &HeaderMap::new()), context(&cancellation))
            .await,
        NetworkError::Transport,
    );
}

#[tokio::test]
async fn tls_verifies_actual_roots_and_hostname() {
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
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("https://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            if let Ok(mut socket) = acceptor.accept(socket).await {
                read_request(&mut socket).await;
                let _ = socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                    .await;
            }
        });
        let roots: Vec<&[u8]> = if trust { vec![&root] } else { Vec::new() };
        let client = BoundedClient::new(
            ClientPolicy {
                origin: &url,
                allow_loopback_http: false,
                proxy: None,
                additional_roots: &roots,
            },
            limits(),
        )
        .unwrap();
        let cancellation = Cancellation::default();
        let result = client
            .execute(get(&url, &HeaderMap::new()), context(&cancellation))
            .await;
        if success {
            assert_eq!(result.unwrap().body(), b"ok");
        } else {
            error(result, NetworkError::Transport);
        }
        task.await.unwrap();
    }
}

#[tokio::test]
async fn only_captured_proxy_is_used() {
    let proxy = Fixture::response("HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nproxy").await;
    let url = "http://127.0.0.1:1/from-proxy";
    let client = BoundedClient::new(
        ClientPolicy {
            origin: url,
            allow_loopback_http: true,
            proxy: Some(&proxy.url),
            additional_roots: &[],
        },
        limits(),
    )
    .unwrap();
    let cancellation = Cancellation::default();
    assert_eq!(
        client
            .execute(get(url, &HeaderMap::new()), context(&cancellation))
            .await
            .unwrap()
            .body(),
        b"proxy"
    );
    assert!(
        std::str::from_utf8(&proxy.request.lock().unwrap())
            .unwrap()
            .starts_with("GET http://127.0.0.1:1/from-proxy HTTP/1.1\r\n")
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GateDenied(&'static str);
impl std::fmt::Display for GateDenied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for GateDenied {}

fn admission_error(
    result: Result<NetworkResponse, GuardedNetworkError<GateDenied>>,
    effect: NetworkEffect,
) -> GateDenied {
    match result {
        Err(GuardedNetworkError::Admission {
            source,
            effect: actual,
        }) => {
            assert_eq!(actual, effect);
            source
        }
        Err(error) => panic!("unexpected safe transport failure: {error:?}"),
        Ok(_) => panic!("revoked admission returned a response"),
    }
}
fn network_error<E>(
    result: Result<NetworkResponse, GuardedNetworkError<E>>,
    source: NetworkError,
    effect: NetworkEffect,
) {
    match result {
        Err(GuardedNetworkError::Network {
            source: actual,
            effect: actual_effect,
        }) => {
            assert_eq!(actual, source);
            assert_eq!(actual_effect, effect);
        }
        Err(error) => panic!("unexpected safe transport failure: {error:?}"),
        Ok(_) => panic!("expected network failure"),
    }
}
fn require_send<T: Send>(value: T) -> T {
    value
}

#[tokio::test]
async fn guarded_admission_denial_and_pre_send_revocation_have_no_socket_effect() {
    let fixture = Fixture::response("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
    let cancellation = Cancellation::default();
    let headers = HeaderMap::new();
    let mut transport = client(&fixture.url, limits());
    let mut deny = || Err(GateDenied("entry"));
    assert_eq!(
        admission_error(
            transport
                .execute_guarded(
                    get(&fixture.url, &headers),
                    context(&cancellation),
                    &mut deny
                )
                .await,
            NetworkEffect::NotSent
        ),
        GateDenied("entry")
    );
    let valid = Arc::new(AtomicBool::new(true));
    let revoked = valid.clone();
    transport.before_send = Some(Box::new(move || revoked.store(false, Ordering::SeqCst)));
    let mut gate = || {
        if valid.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(GateDenied("pre-send"))
        }
    };
    assert_eq!(
        admission_error(
            require_send(transport.execute_guarded(
                get(&fixture.url, &headers),
                context(&cancellation),
                &mut gate
            ))
            .await,
            NetworkEffect::NotSent
        ),
        GateDenied("pre-send")
    );
    tokio::task::yield_now().await;
    assert_eq!(fixture.socket_count.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.count.load(Ordering::SeqCst), 0);
    assert_eq!(
        transport.permits.available_permits(),
        limits().max_in_flight
    );
}

#[tokio::test]
async fn guarded_revocation_during_response_preserves_first_admission_cause_and_effect() {
    let release = Arc::new(Semaphore::new(0));
    let fixture = Fixture::script(vec![
        FixtureStep::Wait(release.clone()),
        FixtureStep::Bytes(
            Duration::ZERO,
            b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\ntest".to_vec(),
        ),
    ])
    .await;
    let valid = Arc::new(AtomicBool::new(true));
    let cancellation = Cancellation::default();
    let headers = HeaderMap::new();
    let transport = client(&fixture.url, limits());
    let mut failures = 0;
    let mut gate = || {
        if valid.load(Ordering::SeqCst) {
            Ok(())
        } else {
            failures += 1;
            Err(GateDenied(if failures == 1 {
                "first admission cause"
            } else {
                "later admission cause"
            }))
        }
    };
    let request = transport.execute_guarded(
        get(&fixture.url, &headers),
        context(&cancellation),
        &mut gate,
    );
    let revoke = async {
        fixture.seen().await;
        valid.store(false, Ordering::SeqCst);
        release.add_permits(1);
    };
    let (result, ()) = tokio::join!(request, revoke);
    assert_eq!(
        admission_error(result, NetworkEffect::MayHaveApplied),
        GateDenied("first admission cause")
    );
    assert!(failures >= 2);
    assert_eq!(fixture.count.load(Ordering::SeqCst), 1);
    assert_eq!(
        transport.permits.available_permits(),
        limits().max_in_flight
    );
}

#[tokio::test(flavor = "current_thread")]
async fn guarded_chunk_eof_and_output_checks_discard_the_bounded_body() {
    for boundary in ["chunk", "eof", "output"] {
        let headers_sent = Arc::new(Semaphore::new(0));
        let chunk_sent = Arc::new(Semaphore::new(0));
        let eof_ready = Arc::new(Semaphore::new(0));
        let first_chunk = Arc::new(Semaphore::new(0));
        let ending = Arc::new(Semaphore::new(0));
        let valid = Arc::new(AtomicBool::new(true));
        let mut steps = vec![
            FixtureStep::Bytes(
                Duration::ZERO,
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec(),
            ),
            FixtureStep::Signal(headers_sent.clone()),
            FixtureStep::Wait(first_chunk.clone()),
            FixtureStep::Bytes(Duration::ZERO, b"1\r\na\r\n".to_vec()),
            FixtureStep::Signal(chunk_sent.clone()),
            FixtureStep::Wait(ending.clone()),
        ];
        if boundary == "output" {
            steps.push(FixtureStep::Signal(eof_ready.clone()));
        } else {
            steps.push(FixtureStep::Revoke(valid.clone()));
        }
        steps.push(FixtureStep::Bytes(
            Duration::ZERO,
            if boundary == "chunk" {
                b"1\r\nb\r\n0\r\n\r\n".to_vec()
            } else {
                b"0\r\n\r\n".to_vec()
            },
        ));
        let fixture = Fixture::script(steps).await;
        let transport = client(&fixture.url, limits());
        let cancellation = Cancellation::default();
        let mut armed_after_eof = false;
        let mut gate = || {
            if !valid.load(Ordering::SeqCst) {
                return Err(GateDenied(boundary));
            }
            if let Ok(permit) = headers_sent.try_acquire() {
                permit.forget();
                first_chunk.add_permits(1);
            }
            if let Ok(permit) = chunk_sent.try_acquire() {
                permit.forget();
                ending.add_permits(1);
            }
            if let Ok(permit) = eof_ready.try_acquire() {
                permit.forget();
                armed_after_eof = true;
                valid.store(false, Ordering::SeqCst);
            }
            Ok(())
        };
        let result = transport
            .execute_guarded(
                get(&fixture.url, &HeaderMap::new()),
                context(&cancellation),
                &mut gate,
            )
            .await;
        assert_eq!(
            admission_error(result, NetworkEffect::MayHaveApplied),
            GateDenied(boundary)
        );
        assert_eq!(armed_after_eof, boundary == "output");
        assert_eq!(fixture.count.load(Ordering::SeqCst), 1);
        assert_eq!(
            transport.permits.available_permits(),
            limits().max_in_flight
        );
    }
}

#[tokio::test]
async fn guarded_transport_failure_is_not_replaced_by_a_later_gate_failure() {
    let valid = Arc::new(AtomicBool::new(true));
    let fixture = Fixture::script(vec![
        FixtureStep::Revoke(valid.clone()),
        FixtureStep::Bytes(Duration::ZERO, b"not an HTTP response".to_vec()),
    ])
    .await;
    let cancellation = Cancellation::default();
    let transport = client(&fixture.url, limits());
    let mut rejected = 0;
    let mut gate = || {
        if valid.load(Ordering::SeqCst) {
            Ok(())
        } else {
            rejected += 1;
            Err(GateDenied("later than transport"))
        }
    };
    network_error(
        transport
            .execute_guarded(
                get(&fixture.url, &HeaderMap::new()),
                context(&cancellation),
                &mut gate,
            )
            .await,
        NetworkError::Transport,
        NetworkEffect::MayHaveApplied,
    );
    assert!(rejected >= 1);
    assert_eq!(fixture.count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn guarded_preflight_busy_and_original_context_fail_before_send() {
    let fixture = Fixture::response("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
    let transport = client(&fixture.url, limits());
    let headers = HeaderMap::new();
    let cancellation = Cancellation::default();
    let mut gate = || Ok::<(), GateDenied>(());
    network_error(
        transport
            .execute_guarded(
                get("http://127.0.0.1:1", &headers),
                context(&cancellation),
                &mut gate,
            )
            .await,
        NetworkError::Origin,
        NetworkEffect::NotSent,
    );
    let body = vec![0u8; limits().request_body_bytes + 1];
    network_error(
        transport
            .execute_guarded(
                NetworkRequest {
                    method: Method::POST,
                    url: &fixture.url,
                    headers: &headers,
                    body: &body,
                },
                context(&cancellation),
                &mut gate,
            )
            .await,
        NetworkError::Limit,
        NetworkEffect::NotSent,
    );
    let permit = transport
        .permits
        .clone()
        .try_acquire_many_owned(limits().max_in_flight as u32)
        .unwrap();
    network_error(
        transport
            .execute_guarded(
                get(&fixture.url, &headers),
                context(&cancellation),
                &mut gate,
            )
            .await,
        NetworkError::Busy,
        NetworkEffect::NotSent,
    );
    drop(permit);
    network_error(
        transport
            .execute_guarded(
                get(&fixture.url, &headers),
                NetworkRequestContext::new(Instant::now(), &cancellation),
                &mut gate,
            )
            .await,
        NetworkError::Deadline,
        NetworkEffect::NotSent,
    );
    cancellation.cancel();
    network_error(
        transport
            .execute_guarded(
                get(&fixture.url, &headers),
                context(&cancellation),
                &mut gate,
            )
            .await,
        NetworkError::Cancelled,
        NetworkEffect::NotSent,
    );
    tokio::task::yield_now().await;
    assert_eq!(fixture.socket_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn guarded_original_cancellation_after_send_and_future_drop_release_permits() {
    let fixture = Fixture::new(Vec::new()).await;
    let transport = client(&fixture.url, limits());
    let headers = HeaderMap::new();
    let cancellation = Cancellation::default();
    let mut gate = || Ok::<(), GateDenied>(());
    let call = transport.execute_guarded(
        get(&fixture.url, &headers),
        context(&cancellation),
        &mut gate,
    );
    let cancel = async {
        fixture.seen().await;
        cancellation.cancel();
    };
    let (result, ()) = tokio::join!(call, cancel);
    network_error(
        result,
        NetworkError::Cancelled,
        NetworkEffect::MayHaveApplied,
    );
    assert_eq!(
        transport.permits.available_permits(),
        limits().max_in_flight
    );
    let fresh = Cancellation::default();
    let mut pending = Box::pin(transport.execute_guarded(
        get(&fixture.url, &headers),
        context(&fresh),
        &mut gate,
    ));
    tokio::select! {
        _ = fixture.seen() => {},
        _ = &mut pending => panic!("fixture should hold the response"),
    }
    drop(pending);
    assert_eq!(
        transport.permits.available_permits(),
        limits().max_in_flight
    );
    assert_eq!(fixture.count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn guarded_deadline_after_observed_send_reports_possible_remote_effect() {
    let fixture = Fixture::new(Vec::new()).await;
    let transport = client(&fixture.url, limits());
    let cancellation = Cancellation::default();
    let mut gate = || Ok::<(), GateDenied>(());
    let result = transport
        .execute_guarded(
            get(&fixture.url, &HeaderMap::new()),
            NetworkRequestContext::new(Instant::now() + Duration::from_millis(250), &cancellation),
            &mut gate,
        )
        .await;
    network_error(
        result,
        NetworkError::Deadline,
        NetworkEffect::MayHaveApplied,
    );
    assert_eq!(fixture.count.load(Ordering::SeqCst), 1);
    assert_eq!(
        transport.permits.available_permits(),
        limits().max_in_flight
    );
}

#[tokio::test]
async fn guarded_valid_calls_preserve_the_existing_response_contract() {
    let fixture = Fixture::response(
        "HTTP/1.1 200 OK\r\nContent-Length: 4\r\nX-Response: captured\r\n\r\ntest",
    )
    .await;
    let transport = client(&fixture.url, limits());
    let cancellation = Cancellation::default();
    let headers = HeaderMap::new();
    let ordinary = transport
        .execute(get(&fixture.url, &headers), context(&cancellation))
        .await
        .unwrap();
    let mut gate = || Ok::<(), GateDenied>(());
    let guarded = transport
        .execute_guarded(
            get(&fixture.url, &headers),
            context(&cancellation),
            &mut gate,
        )
        .await
        .unwrap();
    assert_eq!(ordinary.status(), guarded.status());
    assert_eq!(ordinary.headers(), guarded.headers());
    assert_eq!(ordinary.body(), guarded.body());
    assert_eq!(fixture.count.load(Ordering::SeqCst), 2);
}

#[test]
fn guarded_diagnostics_keep_admission_cause_opaque_but_typed() {
    let error = GuardedNetworkError::Admission {
        source: GateDenied("private-fixture-canary"),
        effect: NetworkEffect::MayHaveApplied,
    };
    assert!(!format!("{error:?} {error}").contains("private-fixture-canary"));
    assert_eq!(error.effect(), NetworkEffect::MayHaveApplied);
    let source = std::error::Error::source(&error).unwrap();
    assert_eq!(
        source.downcast_ref::<GateDenied>(),
        Some(&GateDenied("private-fixture-canary"))
    );
    let error: GuardedNetworkError<GateDenied> = GuardedNetworkError::Network {
        source: NetworkError::Transport,
        effect: NetworkEffect::MayHaveApplied,
    };
    assert_eq!(
        std::error::Error::source(&error)
            .unwrap()
            .downcast_ref::<NetworkError>(),
        Some(&NetworkError::Transport)
    );
}

#[tokio::test]
async fn guarded_pre_send_callback_context_changes_prevent_socket_effects() {
    for cancel in [true, false] {
        let fixture = Fixture::response("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
        let mut transport = client(&fixture.url, limits());
        let before_send = Arc::new(AtomicBool::new(false));
        let armed = before_send.clone();
        transport.before_send = Some(Box::new(move || armed.store(true, Ordering::SeqCst)));
        let cancellation = Cancellation::default();
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut triggered = false;
        let mut gate = || {
            if before_send.load(Ordering::SeqCst) {
                triggered = true;
                if cancel {
                    cancellation.cancel();
                } else {
                    std::thread::sleep(
                        deadline.saturating_duration_since(Instant::now())
                            + Duration::from_millis(5),
                    );
                }
            }
            Ok::<(), GateDenied>(())
        };
        network_error(
            transport
                .execute_guarded(
                    get(&fixture.url, &HeaderMap::new()),
                    NetworkRequestContext::new(deadline, &cancellation),
                    &mut gate,
                )
                .await,
            if cancel {
                NetworkError::Cancelled
            } else {
                NetworkError::Deadline
            },
            NetworkEffect::NotSent,
        );
        assert!(triggered);
        tokio::task::yield_now().await;
        assert_eq!(fixture.socket_count.load(Ordering::SeqCst), 0);
        assert_eq!(fixture.count.load(Ordering::SeqCst), 0);
        assert_eq!(
            transport.permits.available_permits(),
            limits().max_in_flight
        );
    }
}

#[tokio::test]
async fn guarded_last_callback_context_changes_prevent_response_release() {
    // Learn the final callback from a complete real response rather than fixing an internal count.
    let wire = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
    let baseline = Fixture::response(wire).await;
    let transport = client(&baseline.url, limits());
    let cancellation = Cancellation::default();
    let mut final_call = 0;
    let mut valid = || {
        final_call += 1;
        Ok::<(), GateDenied>(())
    };
    let response = transport
        .execute_guarded(
            get(&baseline.url, &HeaderMap::new()),
            context(&cancellation),
            &mut valid,
        )
        .await
        .unwrap();
    assert!(response.body().is_empty());
    assert_eq!(baseline.count.load(Ordering::SeqCst), 1);
    assert!(final_call > 1);
    for cancel in [true, false] {
        let fixture = Fixture::response(wire).await;
        let transport = client(&fixture.url, limits());
        let cancellation = Cancellation::default();
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut calls = 0;
        let mut triggered = false;
        let mut gate = || {
            calls += 1;
            if calls == final_call {
                triggered = true;
                if cancel {
                    cancellation.cancel();
                } else {
                    std::thread::sleep(
                        deadline.saturating_duration_since(Instant::now())
                            + Duration::from_millis(5),
                    );
                }
            }
            Ok::<(), GateDenied>(())
        };
        network_error(
            transport
                .execute_guarded(
                    get(&fixture.url, &HeaderMap::new()),
                    NetworkRequestContext::new(deadline, &cancellation),
                    &mut gate,
                )
                .await,
            if cancel {
                NetworkError::Cancelled
            } else {
                NetworkError::Deadline
            },
            NetworkEffect::MayHaveApplied,
        );
        assert!(triggered);
        assert_eq!(calls, final_call);
        assert_eq!(fixture.count.load(Ordering::SeqCst), 1);
        assert_eq!(
            transport.permits.available_permits(),
            limits().max_in_flight
        );
    }
}
