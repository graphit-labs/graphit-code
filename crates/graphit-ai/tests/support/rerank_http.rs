//! Actual TCP fixture shared by native reranking integration suites.
use serde_json::Value;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Notify, Semaphore},
    task::JoinHandle,
};

pub(super) struct Request {
    pub(super) head: String,
    pub(super) body: Value,
}
pub(super) struct Reply {
    pub(super) status: u16,
    pub(super) body: String,
    pub(super) delay: Duration,
}
pub(super) fn reply(value: Value) -> Reply {
    Reply {
        status: 200,
        body: value.to_string(),
        delay: Duration::ZERO,
    }
}
async fn read_request(socket: &mut TcpStream) -> Request {
    let mut bytes = Vec::new();
    let end = loop {
        if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        let mut buf = [0; 1024];
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
        assert!(bytes.len() < 210000);
    };
    let head = String::from_utf8(bytes[..end].to_vec()).unwrap();
    let length: usize = head
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .map(str::to_owned)
        })
        .map(|length| length.parse().unwrap())
        .unwrap_or_else(|| {
            assert!(
                head.starts_with("GET "),
                "only discovery GET may omit Content-Length"
            );
            0
        });
    while bytes.len() < end + length {
        let mut buf = [0; 4096];
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
        assert!(bytes.len() <= 210000);
    }
    let body = if length == 0 && head.starts_with("GET ") {
        Value::Null
    } else {
        serde_json::from_slice(&bytes[end..end + length]).unwrap()
    };
    Request { head, body }
}
pub(super) struct Fixture {
    pub(super) origin: String,
    pub(super) observed: Arc<Notify>,
    pub(super) server: JoinHandle<Vec<Request>>,
}
pub(super) async fn fixture(replies: Vec<Reply>) -> Fixture {
    fixture_with_barriers(replies, Vec::new()).await
}
pub(super) async fn fixture_with_barriers(
    replies: Vec<Reply>,
    releases: Vec<Arc<Semaphore>>,
) -> Fixture {
    fixture_with_headers_and_barriers(replies, Vec::new(), releases).await
}
#[allow(
    dead_code,
    reason = "shared fixture; response headers are exercised by graphit_embedding unit tests"
)]
pub(super) async fn fixture_with_headers(
    replies: Vec<Reply>,
    headers: Vec<Vec<(String, String)>>,
) -> Fixture {
    fixture_with_headers_and_barriers(replies, headers, Vec::new()).await
}
async fn fixture_with_headers_and_barriers(
    replies: Vec<Reply>,
    headers: Vec<Vec<(String, String)>>,
    releases: Vec<Arc<Semaphore>>,
) -> Fixture {
    fixture_with_origin_and_barriers(|_| (replies, headers), releases, None).await
}
/// Reuse the same bounded TCP server for discovery documents containing its origin.
#[allow(
    dead_code,
    reason = "shared fixture; discovery origins are exercised by App consumer tests"
)]
pub(super) async fn fixture_with_origin(
    replies: impl FnOnce(&str) -> (Vec<Reply>, Vec<Vec<(String, String)>>),
) -> Fixture {
    fixture_with_origin_and_barriers(replies, Vec::new(), None).await
}
/// A workflow may perform Native commits or deliberate no-op reads between HTTP
/// requests. Bound those waits by the caller's existing absolute budget instead
/// of treating five seconds of intentional network silence as a service failure.
#[allow(
    dead_code,
    reason = "used by App Native index and Source consumer workflows"
)]
pub(super) async fn fixture_with_deadline(replies: Vec<Reply>, deadline: Instant) -> Fixture {
    fixture_with_origin_and_barriers(|_| (replies, Vec::new()), Vec::new(), Some(deadline)).await
}
async fn fixture_with_origin_and_barriers(
    prepare: impl FnOnce(&str) -> (Vec<Reply>, Vec<Vec<(String, String)>>),
    releases: Vec<Arc<Semaphore>>,
    deadline: Option<Instant>,
) -> Fixture {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (replies, headers) = prepare(&origin);
    let observed = Arc::new(Notify::new());
    let signal = observed.clone();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for (index, reply) in replies.into_iter().enumerate() {
            let accept_deadline = match deadline {
                Some(deadline) => tokio::time::Instant::from_std(deadline),
                None => tokio::time::Instant::now() + Duration::from_secs(5),
            };
            let (mut socket, _) = tokio::time::timeout_at(accept_deadline, listener.accept())
                .await
                .expect("expected fixture request before its existing deadline")
                .unwrap();
            requests.push(read_request(&mut socket).await);
            let mut header = format!(
                "HTTP/1.1 {} fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
                reply.status,
                reply.body.len()
            );
            if let Some(headers) = headers.get(index) {
                for (name, value) in headers {
                    header.push_str(&format!("{name}: {value}\r\n"));
                }
            }
            header.push_str("\r\n");
            socket.write_all(header.as_bytes()).await.unwrap();
            signal.notify_one();
            if let Some(release) = releases.get(index) {
                release.acquire().await.unwrap().forget();
            }
            if !reply.delay.is_zero() {
                tokio::time::sleep(reply.delay).await;
            }
            let _ = socket.write_all(reply.body.as_bytes()).await;
        }
        // A failure on a later batch must not send the following batch.
        assert!(
            tokio::time::timeout(Duration::from_millis(25), listener.accept())
                .await
                .is_err()
        );
        requests
    });
    Fixture {
        origin,
        observed,
        server,
    }
}
