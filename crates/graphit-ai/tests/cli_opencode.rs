use agent_client_protocol::{
    Agent, Channel, UntypedMessage,
    schema::{ProtocolVersion, v1::*},
};
use graphit_ai::{cli_opencode::*, cli_opencode_wire::OpencodeUpdate};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use serde_json::json;
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

fn context() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(5),
        Cancellation::default(),
    ))
}
fn limits() -> OpencodeLimits {
    OpencodeLimits {
        prompt_text_bytes: 256,
        response_text_bytes: 256,
        notifications: 64,
        id_bytes: 128,
    }
}
fn option(value: &str) -> SessionConfigOption {
    serde_json::from_value(json!({"id":"model","name":"Model","category":"model","type":"select","currentValue":value,"options":[{"value":"provider/exact","name":"Exact"}]})).unwrap()
}
fn agent_option(current: &str) -> SessionConfigOption {
    serde_json::from_value(json!({"id":"mode","name":"Agent","category":"mode","type":"select","currentValue":current,"options":[{"value":"graphit-scout","name":"Published Scout"}]})).unwrap()
}
fn agent_options(mode: &str, current: &str, model: &str) -> Vec<SessionConfigOption> {
    let mut options = Vec::new();
    if mode.starts_with("agent-model") {
        options.push(option(model));
    }
    if mode != "agent-missing" {
        let mut selected = agent_option(current);
        if mode == "agent-category" {
            let mut malformed = serde_json::to_value(&selected).unwrap();
            malformed["category"] = json!("model");
            selected = serde_json::from_value(malformed).unwrap();
        }
        options.push(selected);
    }
    if mode == "agent-ambiguous" {
        options.push(agent_option(current));
    }
    options
}
fn update(session: &str, kind: &str, text: &str) -> UntypedMessage {
    UntypedMessage::new("session/update", json!({"sessionId":session,"update":{"sessionUpdate":kind,"content":{"type":"text","text":text}}})).unwrap()
}
#[derive(Default)]
struct Sink {
    updates: usize,
    fail: bool,
    cancel: bool,
    hold: bool,
    deny_bound: bool,
    bindings: Vec<(String, bool)>,
    phases: Vec<OpencodePhase>,
}
impl OpencodeSink for Sink {
    fn recheck(&self, original: &SnapshotContext) -> io::Result<()> {
        original.check()
    }
    fn session_bound(
        &mut self,
        id: &str,
        loaded: bool,
        original: &SnapshotContext,
    ) -> io::Result<()> {
        original.check()?;
        if self.deny_bound {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        self.bindings.push((id.into(), loaded));
        original.check()
    }
    async fn push<'a>(
        &'a mut self,
        phase: OpencodePhase,
        _: &'a OpencodeUpdate,
        original: &'a SnapshotContext,
    ) -> io::Result<()> {
        self.updates += 1;
        self.phases.push(phase);
        if self.hold {
            std::future::pending::<()>().await;
        }
        if self.cancel {
            original.cancellation().cancel();
        }
        if self.fail {
            Err(io::Error::other("private sink detail"))
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
struct Requests {
    created: AtomicUsize,
    loaded: AtomicUsize,
    configured: AtomicUsize,
    agent_configured: AtomicUsize,
    prompted: AtomicUsize,
}
async fn run_start_with_agent(
    mode: &'static str,
    session: OpencodeSessionStart,
    context: Arc<SnapshotContext>,
    model: Option<&str>,
    selected_agent: Option<&str>,
    bounds: OpencodeLimits,
    sink: &mut Sink,
) -> (Result<OpencodeReceipt, OpencodeFailure>, Arc<Requests>) {
    let directory = tempfile::tempdir().unwrap();
    let cwd = std::path::absolute(directory.path()).unwrap();
    let new_cwd = cwd.clone();
    let load_cwd = cwd.clone();
    let (client, peer) = Channel::duplex();
    let calls = Arc::new(Requests::default());
    let observed = Arc::clone(&calls);
    let created_calls = Arc::clone(&calls);
    let loaded_calls = Arc::clone(&calls);
    let configured_calls = Arc::clone(&calls);
    let agent = tokio::spawn(async move {
        Agent.builder()
            .on_receive_request(async move |request:InitializeRequest,responder,_| {
                assert_eq!(request.protocol_version,ProtocolVersion::V1);
                let value=serde_json::to_value(request).unwrap();
                assert_ne!(value.pointer("/clientCapabilities/terminal"),Some(&json!(true)));
                let mut native=json!({"protocolVersion":if mode=="protocol" {2} else {1},"agentInfo":{"name":"OpenCode","version":if mode=="legacy" { "1.19.0" } else if mode=="future" { "3.0.0" } else if mode=="local" { "local" } else { "2.0.0" }},"agentCapabilities":{"mcpCapabilities":{"http":true,"sse":false}}});
                if mode=="missing-info" { native.as_object_mut().unwrap().remove("agentInfo"); }
                if mode!="load-missing-capability" { native["agentCapabilities"]["loadSession"] = json!(mode!="load-no-capability"); }
                responder.respond(serde_json::from_value::<InitializeResponse>(native).unwrap())
            }, agent_client_protocol::on_receive_request!())
            .on_receive_request(async move |request:NewSessionRequest,responder,_| {
                created_calls.created.fetch_add(1,Ordering::SeqCst);
                assert!(!matches!(mode,"legacy"|"future"|"local"|"missing-info"|"protocol"),"incompatible initialization must not publish session/new");
                assert_eq!(request.cwd,new_cwd);
                assert!(request.mcp_servers.is_empty());
                let response=NewSessionResponse::new("native-main");
                responder.respond(if mode.starts_with("agent-") { response.config_options(agent_options(mode,"build","provider/initial")) } else if mode=="missing-model" { response } else { response.config_options(vec![option("provider/initial")]) })
            }, agent_client_protocol::on_receive_request!())
            .on_receive_request(async move |request:LoadSessionRequest,responder,cx| {
                loaded_calls.loaded.fetch_add(1,Ordering::SeqCst);
                assert_eq!(request.cwd,load_cwd);
                assert!(request.mcp_servers.is_empty());
                assert!(request.additional_directories.is_empty());
                assert!(request.meta.is_none());
                if request.session_id.0.as_ref()!="native-main" || mode=="load-native-error" {
                    return responder.respond_with_internal_error("private native load detail");
                }
                cx.send_notification(update("native-main","user_message_chunk","private historical query"))?;
                cx.send_notification(update(if mode=="load-foreign" {"foreign-replay"} else {"native-main"},"agent_message_chunk",&"historical answer ".repeat(40)))?;
                cx.send_notification(UntypedMessage::new("session/update",json!({"sessionId":"native-main","update":{"sessionUpdate":"native_usage_extension","opaque":{"historical_tokens":999}}})).unwrap())?;
                let response=LoadSessionResponse::new();
                responder.respond(if mode.starts_with("agent-") { response.config_options(agent_options(mode,"build",if mode=="agent-model-loaded-drift" {"provider/other"} else {"provider/exact"})) } else if mode=="load-missing-model" {response} else {response.config_options(vec![option(if mode=="load-drift" {"provider/other"} else {"provider/exact"})])})
            }, agent_client_protocol::on_receive_request!())
            .on_receive_request(async move |request:SetSessionConfigOptionRequest,responder,_| {
                if request.config_id.0.as_ref()=="mode" {
                    configured_calls.agent_configured.fetch_add(1,Ordering::SeqCst);
                    assert!(mode.starts_with("agent-"));
                    assert_eq!(request.session_id.0.as_ref(),"native-main");
                    assert_eq!(serde_json::to_value(&request).unwrap()["value"],json!("graphit-scout"));
                    assert_eq!(configured_calls.configured.load(Ordering::SeqCst),0,"agent selection precedes model selection");
                    if mode=="agent-native-error" {
                        return responder.respond_with_internal_error("private agent configuration detail");
                    }
                    return responder.respond(SetSessionConfigOptionResponse::new(agent_options(mode,if mode=="agent-unconfirmed" {"build"} else {"graphit-scout"},if mode=="agent-model-loaded-drift" {"provider/exact"} else {"provider/initial"})));
                }
                configured_calls.configured.fetch_add(1,Ordering::SeqCst);
                if mode.starts_with("agent-") {
                    assert_eq!(configured_calls.agent_configured.load(Ordering::SeqCst),1);
                }
                assert_eq!(request.session_id.0.as_ref(),"native-main");
                assert_eq!(request.config_id.0.as_ref(),"model");
                assert_eq!(serde_json::to_value(&request).unwrap()["value"],json!("provider/exact"));
                responder.respond(SetSessionConfigOptionResponse::new(if mode.starts_with("agent-") {agent_options(mode,if mode=="agent-model-drift" {"build"} else {"graphit-scout"},"provider/exact")} else {vec![option(if mode=="unconfirmed" { "provider/other" } else { "provider/exact" })]}))
            }, agent_client_protocol::on_receive_request!())
            .on_receive_request(async move |request:PromptRequest,responder,cx| {
                observed.prompted.fetch_add(1,Ordering::SeqCst);
                if mode.starts_with("agent-") {
                    assert_eq!(observed.agent_configured.load(Ordering::SeqCst),1,"prompt requires mode ACK");
                    assert_eq!(observed.configured.load(Ordering::SeqCst),usize::from(mode.starts_with("agent-model")));
                }
                assert_eq!(request.session_id.0.as_ref(),"native-main");
                assert_eq!(serde_json::to_value(request.prompt).unwrap(),json!([{"type":"text","text":"  literal\nquestion\t"}]));
                if mode == "permission" {
                    let permission: RequestPermissionRequest = serde_json::from_value(json!({"sessionId":"native-main","toolCall":{"toolCallId":"native-tool"},"options":[{"optionId":"allow","name":"Allow","kind":"allow_once"}]})).unwrap();
                    let pending = cx.send_request(permission).block_task();
                    return cx.spawn(async move {
                        let response = pending.await?;
                        assert_eq!(response.outcome, RequestPermissionOutcome::Cancelled);
                        responder.respond(PromptResponse::new(StopReason::Cancelled))
                    });
                }
                cx.send_notification(update("native-main","agent_thought_chunk","private thought"))?;
                if matches!(mode,"child-update"|"child-only"|"invalid-child") { cx.send_notification(UntypedMessage::new("session/update",json!({"sessionId":"native-main","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"excluded child"},"_meta":{"opencode/child-session":{"id":"native-child","parentID":"native-main","depth":if mode=="invalid-child" {0} else {1}}}}})).unwrap())?; }
                if mode=="foreign" { cx.send_notification(update("another-session","agent_message_chunk","excluded child"))?; }
                if mode=="unknown" { cx.send_notification(UntypedMessage::new("session/update",json!({"sessionId":"native-main","update":{"sessionUpdate":"native_extension","opaque":{"text":"not an answer"}}})).unwrap())?; }
                if !matches!(mode,"missing-text"|"child-only") { cx.send_notification(update("native-main","agent_message_chunk","  answer\n"))?; cx.send_notification(update("native-main","agent_message_chunk","\t尾  "))?; }
                if mode=="native-error" { return responder.respond_with_internal_error("private native detail"); }
                responder.respond(PromptResponse::new(if mode=="cancelled" { StopReason::Cancelled } else { StopReason::EndTurn }))
            }, agent_client_protocol::on_receive_request!())
            .connect_with(peer, async |cx| { cx.incoming_closed().await; Ok(()) }).await
    });
    let result = run_opencode(
        &context,
        client,
        OpencodeRequest {
            session,
            cwd,
            model: model.map(str::to_owned),
            agent: selected_agent.map(str::to_owned),
            prompt: "  literal\nquestion\t".into(),
        },
        bounds,
        sink,
    )
    .await;
    let _ = agent.await.unwrap();
    (result, calls)
}

async fn run_start(
    mode: &'static str,
    session: OpencodeSessionStart,
    context: Arc<SnapshotContext>,
    model: Option<&str>,
    bounds: OpencodeLimits,
    sink: &mut Sink,
) -> (Result<OpencodeReceipt, OpencodeFailure>, Arc<Requests>) {
    run_start_with_agent(mode, session, context, model, None, bounds, sink).await
}
async fn run(
    mode: &'static str,
    original: Arc<SnapshotContext>,
    model: &str,
    bounds: OpencodeLimits,
    sink: &mut Sink,
) -> (Result<OpencodeReceipt, OpencodeFailure>, usize) {
    let (result, calls) = run_start(
        mode,
        OpencodeSessionStart::Create,
        original,
        Some(model),
        bounds,
        sink,
    )
    .await;
    (result, calls.prompted.load(Ordering::SeqCst))
}

#[tokio::test]
async fn stable_model_literal_progress_and_whitespace_need_end_turn_and_sdk_completion() {
    for mode in ["ok", "unknown", "child-update"] {
        let mut sink = Sink::default();
        let (result, prompted) = run(mode, context(), "provider/exact", limits(), &mut sink).await;
        let receipt = result.unwrap();
        assert_eq!(prompted, 1);
        assert_eq!(receipt.eligible_text(), Some("  answer\n\t尾  "));
        assert!(receipt.driver().unwrap().is_ok());
        assert!(receipt.configured().is_some());
        assert_eq!(receipt.accepted() as usize, sink.updates);
        let debug = format!("{receipt:?}");
        for private in ["native-main", "provider/exact", "answer", "private thought"] {
            assert!(!debug.contains(private));
        }
    }
}
#[tokio::test]
async fn absent_unoffered_variant_and_unconfirmed_models_never_publish_prompt() {
    for (mode, model) in [
        ("missing-model", "provider/exact"),
        ("ok", "provider/exact/effort"),
        ("unconfirmed", "provider/exact"),
    ] {
        let (result, prompted) = run(mode, context(), model, limits(), &mut Sink::default()).await;
        let failure = result.unwrap_err();
        assert_eq!(failure.reason(), OpencodeReason::Configuration);
        assert_eq!(prompted, 0);
        assert!(failure.receipt().eligible_text().is_none());
    }
}
#[tokio::test]
async fn foreign_session_missing_text_native_error_and_nonterminal_never_make_answer() {
    for (mode, reason) in [
        ("foreign", OpencodeReason::Session),
        ("child-only", OpencodeReason::MissingText),
        ("invalid-child", OpencodeReason::Wire),
        ("missing-text", OpencodeReason::MissingText),
        ("native-error", OpencodeReason::Sdk),
        ("cancelled", OpencodeReason::Terminal),
    ] {
        let (result, prompted) = run(
            mode,
            context(),
            "provider/exact",
            limits(),
            &mut Sink::default(),
        )
        .await;
        let failure = result.unwrap_err();
        assert_eq!(prompted, 1);
        assert_eq!(failure.reason(), reason);
        assert!(failure.receipt().eligible_text().is_none());
        assert!(!format!("{failure:?}").contains("private native detail"));
    }
}
#[tokio::test]
async fn actual_original_cancel_sink_failure_and_text_limit_retain_prefix_without_answer() {
    for mode in ["cancel", "sink", "limit"] {
        let mut sink = Sink {
            fail: mode == "sink",
            cancel: mode == "cancel",
            ..Sink::default()
        };
        let mut bounds = limits();
        if mode == "limit" {
            bounds.response_text_bytes = 1;
        }
        let (result, _) = run("ok", context(), "provider/exact", bounds, &mut sink).await;
        let failure = result.unwrap_err();
        match mode {
            "cancel" => assert!(matches!(failure.reason(), OpencodeReason::Context(_))),
            "sink" => assert_eq!(failure.reason(), OpencodeReason::Sink(io::ErrorKind::Other)),
            _ => assert_eq!(failure.reason(), OpencodeReason::ResponseLimit),
        }
        assert!(failure.receipt().eligible_text().is_none());
        assert!(failure.receipt().observed() >= 1);
        assert!(!format!("{failure:?}").contains("private sink detail"));
    }
}

#[tokio::test]
async fn permission_is_actually_cancelled_without_a_grant_or_eligible_text() {
    let (result, prompted) = run(
        "permission",
        context(),
        "provider/exact",
        limits(),
        &mut Sink::default(),
    )
    .await;
    let failure = result.unwrap_err();
    assert_eq!(prompted, 1);
    assert_eq!(failure.reason(), OpencodeReason::Permission);
    assert_eq!(failure.receipt().denied_permissions(), 1);
    assert!(failure.receipt().eligible_text().is_none());
}

#[tokio::test]
async fn pending_progress_obeys_the_same_original_deadline_and_cannot_deliver_answer() {
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_millis(40),
        Cancellation::default(),
    ));
    let (result, _) = run(
        "ok",
        original,
        "provider/exact",
        limits(),
        &mut Sink {
            hold: true,
            ..Sink::default()
        },
    )
    .await;
    let failure = result.unwrap_err();
    assert!(matches!(
        failure.reason(),
        OpencodeReason::Context(io::ErrorKind::TimedOut)
    ));
    assert_eq!(failure.receipt().accepted(), 0);
    assert!(failure.receipt().eligible_text().is_none());
}

#[tokio::test]
async fn legacy_future_local_and_missing_identity_do_not_create_native_session() {
    for (mode, reason) in [
        ("legacy", OpencodeReason::Compatibility),
        ("future", OpencodeReason::Compatibility),
        ("local", OpencodeReason::Compatibility),
        ("missing-info", OpencodeReason::Compatibility),
        ("protocol", OpencodeReason::Protocol),
    ] {
        let (result, prompted) = run(
            mode,
            context(),
            "provider/exact",
            limits(),
            &mut Sink::default(),
        )
        .await;
        let failure = result.unwrap_err();
        assert_eq!(failure.reason(), reason);
        assert!(failure.receipt().initialize().is_some());
        assert!(failure.receipt().created().is_none());
        assert_eq!(prompted, 0);
    }
}

#[tokio::test]
async fn load_ack_without_id_keeps_replay_out_of_new_text_progress_and_usage() {
    let mut created_sink = Sink::default();
    let (created, calls) = run_start(
        "ok",
        OpencodeSessionStart::Create,
        context(),
        Some("provider/exact"),
        limits(),
        &mut created_sink,
    )
    .await;
    let created = created.unwrap();
    assert_eq!(calls.created.load(Ordering::SeqCst), 1);
    let observed = created.created().unwrap().session_id.clone();
    assert_eq!(created_sink.bindings, [(observed.0.to_string(), false)]);
    let mut sink = Sink::default();
    let (result, calls) = run_start(
        "ok",
        OpencodeSessionStart::Load(observed),
        context(),
        Some("provider/exact"),
        limits(),
        &mut sink,
    )
    .await;
    let receipt = result.unwrap();
    assert!(receipt.created().is_none());
    assert!(receipt.loaded().is_some());
    assert_eq!(receipt.eligible_text(), Some("  answer\n\t尾  "));
    assert_eq!(receipt.replayed(), 3);
    assert_eq!(receipt.accepted() as usize, sink.updates);
    assert_eq!(sink.updates, 3);
    assert!(
        sink.phases
            .iter()
            .all(|phase| *phase == OpencodePhase::Prompting)
    );
    assert_eq!(sink.bindings, [("native-main".into(), true)]);
    assert_eq!(calls.created.load(Ordering::SeqCst), 0);
    assert_eq!(calls.loaded.load(Ordering::SeqCst), 1);
    assert_eq!(calls.configured.load(Ordering::SeqCst), 1);
    assert_eq!(calls.prompted.load(Ordering::SeqCst), 1);
    for private in ["native-main", "historical", "provider/exact", "answer"] {
        assert!(!format!("{receipt:?}").contains(private));
    }
}
#[tokio::test]
async fn load_capability_native_session_model_drift_and_foreign_replay_never_create_fallback() {
    for (mode, id, reason, loads) in [
        (
            "load-no-capability",
            "native-main",
            OpencodeReason::Capability,
            0,
        ),
        (
            "load-missing-capability",
            "native-main",
            OpencodeReason::Capability,
            0,
        ),
        ("load-native-error", "native-main", OpencodeReason::Sdk, 1),
        ("ok", "native-missing", OpencodeReason::Sdk, 1),
        (
            "load-drift",
            "native-main",
            OpencodeReason::Configuration,
            1,
        ),
        (
            "load-missing-model",
            "native-main",
            OpencodeReason::Configuration,
            1,
        ),
        ("load-foreign", "native-main", OpencodeReason::Session, 1),
    ] {
        let mut sink = Sink::default();
        let (result, calls) = run_start(
            mode,
            OpencodeSessionStart::Load(SessionId::new(id)),
            context(),
            Some("provider/exact"),
            limits(),
            &mut sink,
        )
        .await;
        let failure = result.unwrap_err();
        assert_eq!(failure.reason(), reason, "{mode}");
        assert!(failure.receipt().eligible_text().is_none());
        assert_eq!(calls.created.load(Ordering::SeqCst), 0, "{mode}");
        assert_eq!(calls.loaded.load(Ordering::SeqCst), loads, "{mode}");
        assert_eq!(calls.configured.load(Ordering::SeqCst), 0, "{mode}");
        assert_eq!(calls.prompted.load(Ordering::SeqCst), 0, "{mode}");
        assert_eq!(sink.updates, 0, "{mode}");
        assert!(!format!("{failure:?}").contains("private native load detail"));
    }
}
#[tokio::test]
async fn replay_uses_total_notification_budget_without_consuming_response_text_budget() {
    let mut bounds = limits();
    bounds.response_text_bytes = 32;
    let (result, _) = run_start(
        "ok",
        OpencodeSessionStart::Load(SessionId::new("native-main")),
        context(),
        Some("provider/exact"),
        bounds,
        &mut Sink::default(),
    )
    .await;
    assert!(result.unwrap().eligible_text().is_some());
    bounds.notifications = 2;
    let (result, calls) = run_start(
        "ok",
        OpencodeSessionStart::Load(SessionId::new("native-main")),
        context(),
        Some("provider/exact"),
        bounds,
        &mut Sink::default(),
    )
    .await;
    assert_eq!(
        result.unwrap_err().reason(),
        OpencodeReason::NotificationLimit
    );
    assert_eq!(calls.created.load(Ordering::SeqCst), 0);
    assert_eq!(calls.prompted.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn load_binding_denial_and_pending_new_progress_keep_real_first_cause() {
    let mut denied = Sink {
        deny_bound: true,
        ..Sink::default()
    };
    let (result, calls) = run_start(
        "ok",
        OpencodeSessionStart::Load(SessionId::new("native-main")),
        context(),
        Some("provider/exact"),
        limits(),
        &mut denied,
    )
    .await;
    assert_eq!(
        result.unwrap_err().reason(),
        OpencodeReason::Sink(io::ErrorKind::PermissionDenied)
    );
    assert_eq!(calls.configured.load(Ordering::SeqCst), 0);
    assert_eq!(calls.prompted.load(Ordering::SeqCst), 0);
    let cancel = Cancellation::default();
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(1),
        cancel,
    ));
    let mut pending = Sink {
        hold: true,
        ..Sink::default()
    };
    let (result, calls) = run_start(
        "ok",
        OpencodeSessionStart::Load(SessionId::new("native-main")),
        original,
        Some("provider/exact"),
        limits(),
        &mut pending,
    )
    .await;
    assert_eq!(
        result.unwrap_err().reason(),
        OpencodeReason::Context(io::ErrorKind::TimedOut)
    );
    assert_eq!(calls.loaded.load(Ordering::SeqCst), 1);
    assert_eq!(calls.prompted.load(Ordering::SeqCst), 1);
    assert_eq!(pending.bindings, [("native-main".into(), true)]);
    assert_eq!(pending.updates, 1);
}

#[tokio::test]
async fn absent_model_preserves_created_or_loaded_native_default_without_configuration() {
    for (mode, session) in [
        ("missing-model", OpencodeSessionStart::Create),
        (
            "load-missing-model",
            OpencodeSessionStart::Load(SessionId::new("native-main")),
        ),
    ] {
        let (result, calls) = run_start(
            mode,
            session,
            context(),
            None,
            limits(),
            &mut Sink::default(),
        )
        .await;
        let receipt = result.unwrap();
        assert_eq!(calls.configured.load(Ordering::SeqCst), 0);
        assert_eq!(calls.prompted.load(Ordering::SeqCst), 1);
        assert!(receipt.configured().is_none());
        assert_eq!(receipt.eligible_text(), Some("  answer\n\t尾  "));
    }
}
#[tokio::test]
async fn explicitly_empty_model_is_invalid_before_session_or_prompt() {
    let (result, calls) = run_start(
        "ok",
        OpencodeSessionStart::Create,
        context(),
        Some(""),
        limits(),
        &mut Sink::default(),
    )
    .await;
    assert_eq!(result.unwrap_err().reason(), OpencodeReason::Policy);
    assert_eq!(calls.created.load(Ordering::SeqCst), 0);
    assert_eq!(calls.loaded.load(Ordering::SeqCst), 0);
    assert_eq!(calls.configured.load(Ordering::SeqCst), 0);
    assert_eq!(calls.prompted.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn explicit_agent_is_acknowledged_with_native_default_model_on_create_and_load() {
    for session in [
        OpencodeSessionStart::Create,
        OpencodeSessionStart::Load(SessionId::new("native-main")),
    ] {
        let (result, calls) = run_start_with_agent(
            "agent-only",
            session,
            context(),
            None,
            Some("graphit-scout"),
            limits(),
            &mut Sink::default(),
        )
        .await;
        let receipt = result.unwrap();
        assert_eq!(calls.agent_configured.load(Ordering::SeqCst), 1);
        assert_eq!(calls.configured.load(Ordering::SeqCst), 0);
        assert_eq!(calls.prompted.load(Ordering::SeqCst), 1);
        assert!(receipt.configured().is_none());
        assert_eq!(receipt.eligible_text(), Some("  answer\n\t尾  "));
        assert!(!format!("{receipt:?}").contains("graphit-scout"));
    }
}
#[tokio::test]
async fn explicit_agent_and_model_are_both_confirmed_before_prompt() {
    for session in [
        OpencodeSessionStart::Create,
        OpencodeSessionStart::Load(SessionId::new("native-main")),
    ] {
        let (result, calls) = run_start_with_agent(
            "agent-model",
            session,
            context(),
            Some("provider/exact"),
            Some("graphit-scout"),
            limits(),
            &mut Sink::default(),
        )
        .await;
        let receipt = result.unwrap();
        assert_eq!(calls.agent_configured.load(Ordering::SeqCst), 1);
        assert_eq!(calls.configured.load(Ordering::SeqCst), 1);
        assert_eq!(calls.prompted.load(Ordering::SeqCst), 1);
        assert!(receipt.configured().is_some());
        assert!(receipt.eligible_text().is_some());
    }
}

#[tokio::test]
async fn unoffered_ambiguous_unconfirmed_or_failed_agent_selection_never_prompts() {
    for (mode, agent, reason, mode_calls) in [
        (
            "agent-missing",
            "graphit-scout",
            OpencodeReason::Configuration,
            0,
        ),
        (
            "agent-ambiguous",
            "graphit-scout",
            OpencodeReason::Configuration,
            0,
        ),
        (
            "agent-category",
            "graphit-scout",
            OpencodeReason::Configuration,
            0,
        ),
        (
            "agent-only",
            "foreign-agent",
            OpencodeReason::Configuration,
            0,
        ),
        (
            "agent-unconfirmed",
            "graphit-scout",
            OpencodeReason::Configuration,
            1,
        ),
        (
            "agent-native-error",
            "graphit-scout",
            OpencodeReason::Sdk,
            1,
        ),
        (
            "agent-model-drift",
            "graphit-scout",
            OpencodeReason::Configuration,
            1,
        ),
    ] {
        let model = if mode == "agent-model-drift" {
            Some("provider/exact")
        } else {
            None
        };
        let (result, calls) = run_start_with_agent(
            mode,
            OpencodeSessionStart::Create,
            context(),
            model,
            Some(agent),
            limits(),
            &mut Sink::default(),
        )
        .await;
        let failure = result.unwrap_err();
        assert_eq!(failure.reason(), reason, "{mode}");
        assert_eq!(
            calls.agent_configured.load(Ordering::SeqCst),
            mode_calls,
            "{mode}"
        );
        assert_eq!(calls.prompted.load(Ordering::SeqCst), 0, "{mode}");
        assert!(failure.receipt().eligible_text().is_none());
        assert!(!format!("{failure:?}").contains("private agent configuration detail"));
    }
}
#[tokio::test]
async fn empty_or_oversized_agent_is_rejected_before_native_session() {
    for (agent, reason) in [
        (String::new(), OpencodeReason::Policy),
        ("x".repeat(limits().id_bytes + 1), OpencodeReason::IdLimit),
    ] {
        let (result, calls) = run_start_with_agent(
            "agent-only",
            OpencodeSessionStart::Create,
            context(),
            None,
            Some(&agent),
            limits(),
            &mut Sink::default(),
        )
        .await;
        assert_eq!(result.unwrap_err().reason(), reason);
        assert_eq!(calls.created.load(Ordering::SeqCst), 0);
        assert_eq!(calls.loaded.load(Ordering::SeqCst), 0);
        assert_eq!(calls.agent_configured.load(Ordering::SeqCst), 0);
        assert_eq!(calls.prompted.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn loaded_model_drift_cannot_be_hidden_by_agent_ack_or_effects() {
    let (result, calls) = run_start_with_agent(
        "agent-model-loaded-drift",
        OpencodeSessionStart::Load(SessionId::new("native-main")),
        context(),
        Some("provider/exact"),
        Some("graphit-scout"),
        limits(),
        &mut Sink::default(),
    )
    .await;
    let failure = result.unwrap_err();
    assert_eq!(failure.reason(), OpencodeReason::Configuration);
    assert!(failure.receipt().loaded().is_some());
    assert!(failure.receipt().eligible_text().is_none());
    assert_eq!(calls.loaded.load(Ordering::SeqCst), 1);
    assert_eq!(calls.agent_configured.load(Ordering::SeqCst), 0);
    assert_eq!(calls.configured.load(Ordering::SeqCst), 0);
    assert_eq!(calls.prompted.load(Ordering::SeqCst), 0);
}
