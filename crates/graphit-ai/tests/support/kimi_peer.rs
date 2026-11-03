//! Controlled native REST/WS peer shared by AI and Ask integration tests.
use axum::{
    Json, Router,
    extract::{
        Request, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::{io::Write, path::PathBuf, sync::Arc};
use tokio::sync::Notify;

pub const TOKEN: &str = "fixture-kimi-token";
pub const SESSION: &str = "session_fixture";
pub const PROMPT: &str = "prompt_fixture";
pub const ANSWER: &str = "  actual β [[Paris]]\n\t";
struct Peer {
    mode: String,
    prompt: Notify,
    shutdown: Notify,
    release: Notify,
    journal: Option<PathBuf>,
    session_state: Option<PathBuf>,
}
impl Peer {
    fn record(&self, value: Value) {
        if let Some(path) = &self.journal {
            let mut output = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .unwrap();
            output.write_all(format!("{value}\n").as_bytes()).unwrap();
        }
    }
}
fn envelope(data: Value) -> Json<Value> {
    Json(json!({"code":0,"msg":"success","data":data,"request_id":"fixture_request"}))
}
async fn auth(request: Request, next: Next) -> Response {
    if request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        != Some("Bearer fixture-kimi-token")
    {
        return (StatusCode::UNAUTHORIZED, Json(json!({"code":40101,"data":null,"msg":"unauthorized","request_id":"fixture_request"}))).into_response();
    }
    next.run(request).await
}
async fn meta(State(peer): State<Arc<Peer>>) -> Response {
    peer.record(json!({"stage":"meta"}));
    if peer.mode == "http" {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"code":42901,"msg":"private provider detail","data":null})),
        )
            .into_response();
    }
    if peer.mode == "response_limit" {
        return envelope(json!({"server_version":"x".repeat(1024*1024+1)})).into_response();
    }
    if peer.mode == "business" {
        return Json(
            json!({"code":70001,"msg":"private provider detail","data":null,"request_id":"fixture_request"}),
        ).into_response();
    }
    envelope(json!({"server_version":"2.2.0"})).into_response()
}
async fn session(State(peer): State<Arc<Peer>>, Json(body): Json<Value>) -> Json<Value> {
    peer.record(json!({"stage":"session","body":body}));
    assert!(
        body["metadata"]["cwd"]
            .as_str()
            .is_some_and(|path| std::path::Path::new(path).is_absolute())
    );
    let result = json!({"id":SESSION,"metadata":body["metadata"]});
    if let Some(path) = &peer.session_state {
        std::fs::write(path, serde_json::to_vec(&result).unwrap()).unwrap();
    }
    envelope(result)
}
async fn existing_session(
    State(peer): State<Arc<Peer>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<Value> {
    peer.record(json!({"stage":"resume","id":id}));
    let stored = peer
        .session_state
        .as_ref()
        .and_then(|path| std::fs::read(path).ok());
    if id != SESSION || stored.is_none() || peer.mode == "missing_session" {
        return Json(json!({"code":40401,"msg":"session missing","data":null}));
    }
    let mut result: Value = serde_json::from_slice(&stored.unwrap()).unwrap();
    if peer.mode == "wrong_session" {
        result["id"] = json!("session_other");
    }
    if peer.mode == "wrong_cwd" {
        result["metadata"]["cwd"] = json!("/different-project");
    }
    envelope(result)
}
async fn prompt(State(peer): State<Arc<Peer>>, Json(body): Json<Value>) -> Json<Value> {
    assert_eq!(body["agent_id"], "main");
    assert_eq!(body["permission_mode"], "manual");
    peer.record(json!({"stage":"prompt","body":body}));
    peer.prompt.notify_one();
    envelope(
        json!({"prompt_id":PROMPT,"status":"running","content":body["content"],"user_message_id":"message_fixture","created_at":"2026-10-16T00:00:00Z"}),
    )
}
async fn shutdown(State(peer): State<Arc<Peer>>) -> Json<Value> {
    peer.record(json!({"stage":"shutdown"}));
    peer.shutdown.notify_one();
    envelope(json!({"ok":true}))
}
async fn release(State(peer): State<Arc<Peer>>) -> Json<Value> {
    peer.release.notify_one();
    envelope(json!({"ok":true}))
}
async fn upgrade(State(peer): State<Arc<Peer>>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| stream(socket, peer))
}
async fn frame(socket: &mut WebSocket, seq: u64, payload: Value, offset: Option<usize>) {
    let mut value = json!({"type":payload["type"],"seq":seq,"session_id":SESSION,"timestamp":"2026-10-16T00:00:00Z","payload":payload});
    if let Some(offset) = offset {
        value["offset"] = json!(offset);
        value["volatile"] = json!(true);
    }
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
}
async fn stream(mut socket: WebSocket, peer: Arc<Peer>) {
    socket.send(Message::Text(json!({"type":"server_hello","timestamp":"2026-10-16T00:00:00Z","payload":{"protocol_version":2,"ws_connection_id":"connection_fixture","max_event_buffer_size":1000,"capabilities":{"event_batching":false,"compression":false}}}).to_string().into())).await.unwrap();
    let Some(Ok(Message::Text(bytes))) = socket.recv().await else {
        panic!("subscribe missing")
    };
    let subscribe: Value = serde_json::from_str(&bytes).unwrap();
    assert_eq!(subscribe["type"], "subscribe");
    assert_eq!(subscribe["payload"]["session_ids"], json!([SESSION]));
    assert_eq!(
        subscribe["payload"]["agent_filter"][SESSION],
        json!(["main"])
    );
    peer.record(json!({"stage":"subscribed"}));
    if peer.mode == "ack_error" {
        socket.send(Message::Text(json!({"type":"ack","id":subscribe["id"],"code":9001,"msg":"private provider detail","payload":{}}).to_string().into())).await.unwrap();
        let _ = socket.recv().await;
        return;
    }
    if peer.mode == "resync" {
        socket.send(Message::Text(json!({"type":"ack","id":subscribe["id"],"code":0,"payload":{"accepted_subscriptions":[SESSION],"resync_required":[SESSION]}}).to_string().into())).await.unwrap();
        let _ = socket.recv().await;
        return;
    }

    socket.send(Message::Text(json!({"type":"ack","id":subscribe["id"],"code":0,"msg":"success","payload":{"accepted_subscriptions":[SESSION],"resync_required":[]}}).to_string().into())).await.unwrap();
    peer.prompt.notified().await;
    if peer.mode == "frame_limit" {
        frame(&mut socket,1,json!({"type":"assistant.delta","agentId":"main","turnId":7,"delta":"x".repeat(1024*1024+1)}),Some(0)).await;
        let _ = socket.recv().await;
        return;
    }
    let prompt = if peer.mode == "foreign" {
        "foreign_prompt"
    } else {
        PROMPT
    };
    frame(&mut socket, 1, json!({"type":"turn.started","agentId":"main","turnId":7,"promptId":prompt,"origin":{"kind":"user"}}), None).await;
    if peer.mode == "foreign" {
        let _ = socket.recv().await;
        return;
    }
    frame(
        &mut socket,
        2,
        json!({"type":"turn.step.started","agentId":"main","turnId":7,"stepId":1}),
        None,
    )
    .await;
    frame(
        &mut socket,
        2,
        json!({"type":"assistant.delta","agentId":"main","turnId":7,"delta":"interim before tool"}),
        Some(0),
    )
    .await;
    frame(&mut socket, 3, json!({"type":"hook.result","agentId":"main","turnId":7,"hookEvent":"before_turn","content":"excluded hook answer"}), None).await;
    frame(&mut socket, 3, json!({"type":"assistant.delta","agentId":"child","turnId":7,"delta":"excluded child answer"}), Some(0)).await;
    frame(&mut socket, 4, json!({"type":"compaction.completed","agentId":"main","result":{"summary":"excluded summary"}}), None).await;
    // Raw lexical payload must survive the consumer's event serialization.
    socket.send(Message::Text(format!("{{\"type\":\"tool.result\",\"seq\":5,\"session_id\":\"{SESSION}\",\"timestamp\":\"2026-10-16T00:00:00Z\",\"payload\":{{\"type\":\"tool.result\",\"agentId\":\"main\",\"turnId\":7,\"toolCallId\":\"native_tool\",\"output\":{{\"literal\":1.2300e+02}}}}}}").into())).await.unwrap();
    frame(
        &mut socket,
        6,
        json!({"type":"turn.step.started","agentId":"main","turnId":7,"stepId":2}),
        None,
    )
    .await;
    frame(
        &mut socket,
        6,
        json!({"type":"assistant.delta","agentId":"main","turnId":7,"delta":ANSWER}),
        Some(0),
    )
    .await;
    peer.record(json!({"stage":"chunk"}));
    if peer.mode == "gate" {
        peer.release.notified().await;
    }
    match peer.mode.as_str() {
        "hold" => {
            while socket.recv().await.is_some() {}
            return;
        }
        "no_terminal" => {
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
        "gap" => {
            frame(&mut socket, 6, json!({"type":"assistant.delta","agentId":"main","turnId":7,"delta":"excluded gap"}), Some(999)).await;
            let _ = socket.recv().await;
            return;
        }
        "error" => {
            frame(&mut socket,7,json!({"type":"error","agentId":"main","error":{"code":"provider.internal","message":"private provider detail"}}),None).await;
            let _ = socket.recv().await;
            return;
        }
        "permission" => {
            frame(&mut socket, 7, json!({"type":"event.approval.requested","agentId":"main","approval":{"approval_id":"native_approval"}}), None).await;
            let _ = socket.recv().await;
            return;
        }
        _ => (),
    }
    let reason = match peer.mode.as_str() {
        "native_error" => "failed",
        "cancelled" => "cancelled",
        "blocked" => "blocked",
        _ => "completed",
    };
    let mut terminal = json!({"type":"turn.ended","agentId":"main","turnId":7,"reason":reason});
    if reason == "failed" || peer.mode == "completed_error" {
        terminal["error"] = json!({"code":"provider.internal","message":"private provider detail"});
    }
    frame(&mut socket, 7, terminal, None).await;
    peer.record(json!({"stage":"terminal"}));
    while let Some(message) = socket.recv().await {
        if matches!(message, Ok(Message::Close(_)) | Err(_)) {
            break;
        }
    }
}
/// Called only by an explicitly selected ignored test in a real subprocess.
pub fn run() {
    assert_eq!(std::env::var("KIMI_PEER").as_deref(), Ok("1"));
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        let mode = std::env::var_os("KIMI_CONTROL_MODE").and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_else(|| std::env::var("KIMI_MODE").unwrap_or_else(|_| "answer".into()));
        if mode == "startup_eof" { return; }
        let peer = Arc::new(Peer { mode, prompt: Notify::new(), shutdown: Notify::new(), release: Notify::new(), journal: std::env::var_os("KIMI_JOURNAL").map(PathBuf::from), session_state: std::env::var_os("KIMI_SESSION_STATE").map(PathBuf::from) });
        peer.record(json!({"stage":"spawn","pid":std::process::id(),"cwd":std::env::current_dir().unwrap(),"captured":std::env::var("KIMI_CAPTURED").ok()}));
        let router = Router::new().route("/api/v1/meta", get(meta))
            .route("/api/v1/sessions", post(session)).route("/api/v1/sessions/{session_id}", get(existing_session)).route("/api/v1/ws", get(upgrade))
            .route(&format!("/api/v1/sessions/{SESSION}/prompts"), post(prompt))
            .route("/api/v1/shutdown", post(shutdown)).route("/fixture/release", post(release)).layer(middleware::from_fn(auth)).with_state(Arc::clone(&peer));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        if let Some(path) = std::env::var_os("KIMI_PEER_PORT") { std::fs::write(path, port.to_string()).unwrap(); }
        if peer.mode == "endpoint" { println!("Kimi server: http://192.0.2.1:{port}/#token={TOKEN}"); }
        else { println!("Kimi server: http://127.0.0.1:{port}/#token={TOKEN}"); }
        std::io::stdout().flush().unwrap();
        let control = Arc::clone(&peer);
        axum::serve(listener, router).with_graceful_shutdown(async move { control.shutdown.notified().await }).await.unwrap();
        if peer.mode == "nonzero" { std::process::exit(9); }
    });
}
