#![cfg(target_os = "linux")]
//! Client framing only. These protocol fixtures do not assert Broker authorization.
use graphit_app::{
    native_install_http::NativeInstallPolicy,
    now_transport::{NowTransportError, connect_now},
    source_server::{BrokerRequest, SourcePolicy, SourceServer},
};
#[path = "support/knowledge_ask_server.rs"]
mod support;
use axum::{
    Router,
    body::{Body, Bytes},
    http::HeaderMap,
    routing::post,
};
use graphit_auth::Permission;
use graphit_core::{
    brand::Brand, file_lock::Cancellation, module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
};
use graphit_hub::{NativeBinding, native::ContentLimits, policy::RequestContext};
use serde_json::{Value, json};
use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio_stream::StreamExt;

fn content() -> ContentLimits {
    ContentLimits {
        max_input_bytes: 65536,
        max_output_bytes: 65536,
        max_depth: 32,
        max_nodes: 4096,
        max_scalar_bytes: 4096,
        max_collection_entries: 64,
    }
}
fn native() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap()
}
fn fixture() -> support::Fixture {
    support::fixture(
        &[Permission::Read(Module::Task)],
        Brand::default(),
        NativeBinding::Codex,
    )
}
fn request(f: &support::Fixture) -> BrokerRequest {
    BrokerRequest {
        project_id: f.projects[0].1,
        provider: "now".into(),
        version: None,
        instance_id: None,
    }
}
fn snapshot(f: &support::Fixture, instances: Value) -> Value {
    json!({"type":"snapshot","project_id":f.projects[0].1,"instances":instances})
}
struct Endpoint {
    address: SocketAddr,
    stop: tokio_util::sync::CancellationToken,
    task: tokio::task::JoinHandle<()>,
}
impl Endpoint {
    async fn start(raw: String, chunks: Vec<Vec<u8>>, keep_open: bool) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let chunks = Arc::new(Mutex::new(Some(chunks)));
        let router = Router::new().route(
            "/api/now/subscribe",
            post(move |headers: HeaderMap| {
                let raw = raw.clone();
                let chunks = chunks.clone();
                async move {
                    assert_eq!(headers.get("x-graphit-auth-origin").unwrap(), "runtime");
                    assert_eq!(
                        headers.get("authorization").unwrap().as_bytes(),
                        format!("Bearer {raw}").as_bytes()
                    );
                    let chunks = chunks.lock().unwrap().take().unwrap();
                    let parts = tokio_stream::iter(
                        chunks
                            .into_iter()
                            .map(|bytes| Ok::<_, Infallible>(Bytes::from(bytes))),
                    );
                    let stream = parts.chain(if keep_open {
                        Box::pin(tokio_stream::pending())
                            as std::pin::Pin<
                                Box<
                                    dyn tokio_stream::Stream<Item = Result<Bytes, Infallible>>
                                        + Send,
                                >,
                            >
                    } else {
                        Box::pin(tokio_stream::empty())
                    });
                    (
                        [
                            ("content-type", "application/x-ndjson"),
                            ("cache-control", "no-store"),
                            ("x-content-type-options", "nosniff"),
                        ],
                        Body::from_stream(stream),
                    )
                }
            }),
        );
        let stop = tokio_util::sync::CancellationToken::new();
        let signal = stop.clone();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(signal.cancelled_owned())
                .await
                .unwrap();
        });
        Self {
            address,
            stop,
            task,
        }
    }
    async fn shutdown(self) {
        self.stop.cancel();
        tokio::time::timeout(Duration::from_secs(5), self.task)
            .await
            .unwrap()
            .unwrap();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fragmented_unicode_full_and_empty_snapshots_preserve_precision_and_replace() {
    let f = fixture();
    let section =
        json!({"availability":"available","items":[],"total":0,"has_more":false,"error":null});
    let now = json!({"project_id":f.projects[0].1,"generated_at":"9007199254740993001","sections":{"tasks":section,"sessions":section,"memories":section,"knowledge":section}});
    let full = snapshot(
        &f,
        json!([{"instance_id":"a","version":"feature/β","now":now}]),
    );
    let mut bytes = serde_json::to_vec(&full).unwrap();
    bytes.push(b'\n');
    bytes.extend(serde_json::to_vec(&snapshot(&f, json!([]))).unwrap());
    bytes.push(b'\n');
    // Single-byte body chunks include splits inside UTF-8 and escaped JSON values.
    let endpoint = Endpoint::start(
        f.raw.clone(),
        bytes.into_iter().map(|byte| vec![byte]).collect(),
        false,
    )
    .await;
    let cancel = Cancellation::default();
    let original = RequestContext::service(content(), &cancel).unwrap();
    let request = request(&f);
    let client = reqwest::Client::new();
    let mut stream = connect_now(
        &f.paths,
        endpoint.address,
        &client,
        (native(), content()),
        &request,
        &original,
    )
    .await
    .unwrap();
    let full = stream.next().await.unwrap().unwrap();
    assert_eq!(full.instances()[0].version(), "feature/β");
    assert_eq!(
        serde_json::to_value(full.instances()[0].now()).unwrap()["generated_at"],
        "9007199254740993001"
    );
    assert!(stream.next().await.unwrap().unwrap().instances().is_empty());
    assert!(stream.next().await.unwrap().is_none());
    drop(stream);
    endpoint.shutdown().await;
    f.stop.cancel();
    f.server.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_utf8_json_envelope_and_oversize_are_terminal_not_skipped() {
    for broken in [
        vec![0xff, b'\n'],
        b"not-json\n".to_vec(),
        b"{\"type\":\"welcome\",\"heartbeat_timeout_ns\":\"1000000000\"}\n".to_vec(),
        vec![b'x'; 129],
    ] {
        let f = fixture();
        let mut cap = content();
        cap.max_output_bytes = 128;
        let good = serde_json::to_vec(&snapshot(&f, json!([]))).unwrap();
        let mut bytes = broken;
        bytes.extend_from_slice(&good);
        bytes.push(b'\n');
        let endpoint = Endpoint::start(f.raw.clone(), vec![bytes], true).await;
        let cancel = Cancellation::default();
        let original = RequestContext::service(content(), &cancel).unwrap();
        let request = request(&f);
        let client = reqwest::Client::new();
        let mut stream = connect_now(
            &f.paths,
            endpoint.address,
            &client,
            (native(), cap),
            &request,
            &original,
        )
        .await
        .unwrap();
        let error = stream.next().await.err().unwrap();
        let diagnostic = format!("{error:?}");
        assert!(!diagnostic.contains(f.temp.path().to_str().unwrap()));
        assert!(
            !diagnostic.contains(
                f.auth
                    .installation()
                    .live
                    .allowed_roots()
                    .next()
                    .unwrap()
                    .to_str()
                    .unwrap()
            )
        );
        assert!(!diagnostic.contains(&f.raw));
        assert!(stream.next().await.unwrap().is_none());
        drop(stream);
        endpoint.shutdown().await;
        f.stop.cancel();
        f.server.shutdown().await.unwrap();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn service_cancel_and_finite_deadline_drop_the_pending_http_body() {
    for service in [false, true] {
        let f = fixture();
        let mut line = serde_json::to_vec(&snapshot(&f, json!([]))).unwrap();
        line.push(b'\n');
        let endpoint = Endpoint::start(f.raw.clone(), vec![line], true).await;
        let cancel = Cancellation::default();
        let original = if service {
            RequestContext::service(content(), &cancel).unwrap()
        } else {
            RequestContext::new(
                content(),
                &cancel,
                Instant::now() + Duration::from_millis(300),
            )
            .unwrap()
        };
        let request = request(&f);
        let client = reqwest::Client::new();
        let mut stream = connect_now(
            &f.paths,
            endpoint.address,
            &client,
            (native(), content()),
            &request,
            &original,
        )
        .await
        .unwrap();
        assert!(stream.next().await.unwrap().unwrap().instances().is_empty());
        if service {
            cancel.cancel();
        }
        let error = stream.next().await.err().unwrap();
        assert!(matches!(error, NowTransportError::Request(_)));
        drop(stream);
        endpoint.shutdown().await;
        f.stop.cancel();
        f.server.shutdown().await.unwrap();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_snapshot_for_another_project_fails_before_delivery() {
    let f = fixture();
    let line = json!({"type":"snapshot","project_id":"01D39ZY06FGSCTVN4T2V9PKHFZ","instances":[]})
        .to_string()
        + "\n";
    let endpoint = Endpoint::start(f.raw.clone(), vec![line.into_bytes()], false).await;
    let cancel = Cancellation::default();
    let original = RequestContext::service(content(), &cancel).unwrap();
    let request = request(&f);
    let client = reqwest::Client::new();
    let mut stream = connect_now(
        &f.paths,
        endpoint.address,
        &client,
        (native(), content()),
        &request,
        &original,
    )
    .await
    .unwrap();
    assert!(matches!(
        stream.next().await,
        Err(NowTransportError::Snapshot(_))
    ));
    assert!(stream.next().await.unwrap().is_none());
    drop(stream);
    endpoint.shutdown().await;
    f.stop.cancel();
    f.server.shutdown().await.unwrap();
}
