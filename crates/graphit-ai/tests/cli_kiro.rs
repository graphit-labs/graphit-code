use agent_client_protocol::{
    Agent, Channel, Client, ConnectTo, Error as SdkError, UntypedMessage,
    schema::{ProtocolVersion, v1::*},
};
use graphit_ai::{cli_kiro::*, cli_kiro_wire::*};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use serde_json::json;
use std::{
    future::Future,
    io,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Notify;

fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(5),
        Cancellation::default(),
    ))
}
fn limits() -> KiroLimits {
    KiroLimits {
        prompt_text_bytes: 128,
        response_text_bytes: 128,
        notifications: 32,
        id_bytes: 128,
    }
}
fn text(kind: &str, value: &str) -> UntypedMessage {
    UntypedMessage::new("session/update", json!({"sessionId":"actual-session","update":{"sessionUpdate":kind,"content":{"type":"text","text":value}}})).unwrap()
}
#[derive(Default)]
struct Metrics {
    initialize: AtomicUsize,
    new_session: AtomicUsize,
    load_session: AtomicUsize,
    config: AtomicUsize,
    prompt: AtomicUsize,
    cancel: AtomicUsize,
    delete: AtomicUsize,
    started: Notify,
    oversized_session: bool,
}

fn delete_agent(
    mode: &'static str,
    metrics: Arc<Metrics>,
    denied: Arc<std::sync::atomic::AtomicBool>,
) -> agent_client_protocol::Builder<
    Agent,
    impl agent_client_protocol::HandleDispatchFrom<Client>,
    impl agent_client_protocol::RunWithConnectionTo<Client>,
> {
    let initialized = metrics.clone();
    let deleted = metrics.clone();
    let created = metrics.clone();
    let loaded = metrics.clone();
    Agent
        .builder()
        .on_receive_request(
            async move |_: InitializeRequest, responder, _| {
                initialized.initialize.fetch_add(1, Ordering::SeqCst);
                if mode == "deny_after_init" {
                    denied.store(true, Ordering::SeqCst);
                }
                let capabilities = if mode == "unsupported" {
                    SessionCapabilities::new()
                } else {
                    SessionCapabilities::new().delete(SessionDeleteCapabilities::new())
                };
                responder.respond(
                    InitializeResponse::new(ProtocolVersion::V1).agent_capabilities(
                        AgentCapabilities::new().session_capabilities(capabilities),
                    ),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: DeleteSessionRequest, responder, cx| {
                assert_eq!(request.session_id.0.as_ref(), "actual-session");
                assert_eq!(deleted.delete.fetch_add(1, Ordering::SeqCst), 0);
                deleted.started.notify_one();
                if mode == "error" {
                    return responder.respond_with_internal_error("synthetic-secret-delete-error");
                }
                if mode == "pending" {
                    return cx.spawn(async move {
                        std::future::pending::<()>().await;
                        responder.respond(DeleteSessionResponse::new())
                    });
                }
                cx.send_notification(text(
                    "agent_message_chunk",
                    "deletion update is not an answer",
                ))?;
                responder.respond(
                    DeleteSessionResponse::new().meta(
                        json!({"native":"synthetic-secret-delete-response"})
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_: NewSessionRequest, responder, _| {
                created.new_session.fetch_add(1, Ordering::SeqCst);
                responder.respond(NewSessionResponse::new("unexpected"))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_: LoadSessionRequest, responder, _| {
                loaded.load_session.fetch_add(1, Ordering::SeqCst);
                responder.respond(LoadSessionResponse::new())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_: PromptRequest, responder, _| {
                metrics.prompt.fetch_add(1, Ordering::SeqCst);
                responder.respond(PromptResponse::new(StopReason::EndTurn))
            },
            agent_client_protocol::on_receive_request!(),
        )
}

#[tokio::test]
async fn native_delete_uses_real_capability_response_and_never_a_prompt_answer() {
    {
        let engine = KiroEngine::V3;
        let context = original();
        let metrics = Arc::new(Metrics::default());
        let mut sink = Sink {
            original: Some(context.clone()),
            ..Sink::default()
        };
        let receipt = delete_kiro_session(
            &context,
            delete_agent(
                "answer",
                metrics.clone(),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            ),
            engine,
            DeleteSessionRequest::new("actual-session"),
            limits(),
            &mut sink,
            &|| Ok(()),
        )
        .await
        .unwrap();
        assert!(
            receipt
                .initialize_response()
                .unwrap()
                .agent_capabilities
                .session_capabilities
                .delete
                .is_some()
        );
        assert_eq!(
            receipt.delete_response().unwrap().meta.as_ref().unwrap()["native"],
            "synthetic-secret-delete-response"
        );
        assert!(receipt.driver().unwrap().is_ok());
        assert_eq!(receipt.accepted(), 1);
        assert_eq!(sink.phases, [KiroDeliveryPhase::Deleting]);
        assert!(receipt.candidate_text().is_none() && receipt.answer().is_none());
        assert!(
            receipt.new_response().is_none()
                && receipt.load_response().is_none()
                && receipt.response().is_none()
        );
        assert_eq!(metrics.initialize.load(Ordering::SeqCst), 1);
        assert_eq!(metrics.delete.load(Ordering::SeqCst), 1);
        assert_eq!(
            metrics.new_session.load(Ordering::SeqCst)
                + metrics.load_session.load(Ordering::SeqCst)
                + metrics.prompt.load(Ordering::SeqCst),
            0
        );
        assert!(!format!("{receipt:?}").contains("synthetic-secret"));
        assert!(!format!("{receipt:?}").contains("actual-session"));
    }
}

#[tokio::test]
async fn native_delete_missing_capability_and_real_sdk_error_preserve_first_cause() {
    for mode in ["unsupported", "error"] {
        let metrics = Arc::new(Metrics::default());
        let error = delete_kiro_session(
            &original(),
            delete_agent(
                mode,
                metrics.clone(),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            ),
            KiroEngine::V3,
            DeleteSessionRequest::new("actual-session"),
            limits(),
            &mut Sink::default(),
            &|| Ok(()),
        )
        .await
        .unwrap_err();
        assert!(error.receipt().initialize_response().is_some());
        assert!(error.receipt().delete_response().is_none() && error.receipt().answer().is_none());
        assert_eq!(
            metrics.delete.load(Ordering::SeqCst),
            usize::from(mode == "error")
        );
        if mode == "unsupported" {
            assert_eq!(error.reason(), KiroReason::UnsupportedDelete);
        } else {
            assert_eq!(error.reason(), KiroReason::Sdk);
            assert!(error.sdk_error().is_some());
        }
        assert!(!format!("{error:?}").contains("synthetic-secret"));
    }
}

#[tokio::test]
async fn native_delete_rechecks_borrowed_authority_after_initialize_before_effect() {
    let metrics = Arc::new(Metrics::default());
    let denied = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let check = || {
        if denied.load(Ordering::SeqCst) {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "synthetic-secret-actual-authority-cause",
            ))
        } else {
            Ok(())
        }
    };
    let error = delete_kiro_session(
        &original(),
        delete_agent("deny_after_init", metrics.clone(), denied.clone()),
        KiroEngine::V3,
        DeleteSessionRequest::new("actual-session"),
        limits(),
        &mut Sink::default(),
        &check,
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.reason(),
        KiroReason::Context(io::ErrorKind::PermissionDenied)
    );
    assert_eq!(metrics.initialize.load(Ordering::SeqCst), 1);
    assert_eq!(metrics.delete.load(Ordering::SeqCst), 0);
    assert!(error.receipt().initialize_response().is_some());
    assert!(error.receipt().delete_response().is_none());
    assert!(!format!("{error:?}").contains("synthetic-secret"));
}

#[tokio::test]
async fn native_delete_pending_response_obeys_original_cancel_and_deadline_without_retry() {
    for cancel in [true, false] {
        let context = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_millis(if cancel { 1000 } else { 75 }),
            Cancellation::default(),
        ));
        let metrics = Arc::new(Metrics::default());
        let mut sink = Sink::default();
        let work = delete_kiro_session(
            &context,
            delete_agent(
                "pending",
                metrics.clone(),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            ),
            KiroEngine::V3,
            DeleteSessionRequest::new("actual-session"),
            limits(),
            &mut sink,
            &|| Ok(()),
        );
        let interrupted = async {
            metrics.started.notified().await;
            if cancel {
                context.cancellation().cancel();
            }
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(work, interrupted)
        })
        .await
        .unwrap();
        let error = result.unwrap_err();
        assert_eq!(
            error.reason(),
            KiroReason::Context(if cancel {
                io::ErrorKind::Interrupted
            } else {
                io::ErrorKind::TimedOut
            })
        );
        assert_eq!(metrics.delete.load(Ordering::SeqCst), 1);
        assert!(error.receipt().delete_response().is_none() && error.receipt().answer().is_none());
        assert!(error.receipt().cancel_enqueue().is_none());
    }
}

#[tokio::test]
async fn native_delete_invalid_id_or_original_never_polls_transport() {
    for scenario in ["empty", "nul", "limit", "cancelled", "unbounded"] {
        let context = if scenario == "unbounded" {
            Arc::new(SnapshotContext::service(Cancellation::default()))
        } else {
            original()
        };
        if scenario == "cancelled" {
            context.cancellation().cancel();
        }
        let id = match scenario {
            "empty" => "".to_owned(),
            "nul" => "a\0b".to_owned(),
            "limit" => "x".repeat(129),
            _ => "actual-session".to_owned(),
        };
        let metrics = Arc::new(Metrics::default());
        let error = delete_kiro_session(
            &context,
            delete_agent(
                "answer",
                metrics.clone(),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            ),
            KiroEngine::V3,
            DeleteSessionRequest::new(id),
            limits(),
            &mut Sink::default(),
            &|| Ok(()),
        )
        .await
        .unwrap_err();
        assert_eq!(metrics.initialize.load(Ordering::SeqCst), 0);
        assert_eq!(metrics.delete.load(Ordering::SeqCst), 0);
        assert!(error.receipt().driver().is_none());
    }
}

fn selection() -> KiroConfigSelection {
    KiroConfigSelection {
        config_id: "fixture-select".into(),
        value_id: "fixture-value!".into(),
        meta: Some(
            json!({"private":"synthetic-secret-config-request"})
                .as_object()
                .unwrap()
                .clone(),
        ),
    }
}
fn configured_prompt() -> KiroPrompt {
    KiroPrompt {
        text: "prompt".into(),
        selection: Some(selection()),
    }
}
fn select_offer(grouped: bool, current: &str) -> SessionConfigOption {
    let values = vec![SessionConfigSelectOption::new(
        "fixture-value!",
        "Literal label",
    )];
    let options = if grouped {
        SessionConfigSelectOptions::Grouped(vec![SessionConfigSelectGroup::new(
            "fixture-group",
            "Group",
            values,
        )])
    } else {
        SessionConfigSelectOptions::Ungrouped(values)
    };
    SessionConfigOption::select(
        "fixture-select",
        "Opaque option",
        SessionConfigValueId::new(current),
        options,
    )
}
fn config_meta() -> Meta {
    json!({"private":"synthetic-secret-config-response"})
        .as_object()
        .unwrap()
        .clone()
}
struct ConfigFixture {
    offers: Option<Vec<SessionConfigOption>>,
    returned: Vec<SessionConfigOption>,
    events: Vec<UntypedMessage>,
    fresh: Option<&'static str>,
    ending: LoadEnding,
    expected_set: bool,
}
impl Default for ConfigFixture {
    fn default() -> Self {
        Self {
            offers: Some(vec![select_offer(false, "previous-value")]),
            returned: vec![select_offer(false, "fixture-value!")],
            events: vec![],
            fresh: Some("ok"),
            ending: LoadEnding::Response,
            expected_set: true,
        }
    }
}
fn config_agent(
    mut fixture: ConfigFixture,
    metrics: Arc<Metrics>,
) -> agent_client_protocol::Builder<
    Agent,
    impl agent_client_protocol::HandleDispatchFrom<Client>,
    impl agent_client_protocol::RunWithConnectionTo<Client>,
> {
    let initialized = metrics.clone();
    let created = metrics.clone();
    let loaded = metrics.clone();
    let configured = metrics.clone();
    let prompted = metrics.clone();
    let offers = fixture.offers.clone();
    let close = metrics.clone();
    let ending = fixture.ending;
    Agent
        .builder()
        .on_receive_request(
            async move |request: InitializeRequest, responder, _| {
                assert!(
                    request
                        .client_capabilities
                        .session
                        .as_ref()
                        .and_then(|session| session.config_options.as_ref())
                        .is_none_or(|options| options.boolean.is_none())
                );
                initialized.initialize.fetch_add(1, Ordering::SeqCst);
                responder.respond(
                    InitializeResponse::new(ProtocolVersion::V1)
                        .agent_capabilities(AgentCapabilities::new().load_session(true)),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_: NewSessionRequest, responder, _| {
                created.new_session.fetch_add(1, Ordering::SeqCst);
                responder.respond(
                    NewSessionResponse::new("actual-session")
                        .config_options(offers.clone())
                        .meta(config_meta()),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_: LoadSessionRequest, responder, _| {
                loaded.load_session.fetch_add(1, Ordering::SeqCst);
                responder.respond(
                    LoadSessionResponse::new()
                        .config_options(fixture.offers.clone())
                        .meta(config_meta()),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: SetSessionConfigOptionRequest, responder, cx| {
                assert_eq!(request.session_id.0.as_ref(), "actual-session");
                assert_eq!(request.config_id, selection().config_id);
                assert_eq!(request.value.as_value_id(), Some(&selection().value_id));
                assert_eq!(request.meta, selection().meta);
                assert_eq!(configured.config.fetch_add(1, Ordering::SeqCst), 0);
                assert_eq!(configured.prompt.load(Ordering::SeqCst), 0);
                for event in fixture.events.drain(..) {
                    cx.send_notification(event)?;
                }
                configured.started.notify_one();
                match fixture.ending {
                    LoadEnding::Response => responder.respond(
                        SetSessionConfigOptionResponse::new(fixture.returned.clone())
                            .meta(config_meta()),
                    ),
                    LoadEnding::Error => {
                        responder.respond_with_internal_error("synthetic-secret-config-error")
                    }
                    LoadEnding::Pending | LoadEnding::Close => cx.spawn(async move {
                        std::future::pending::<()>().await;
                        responder.respond(SetSessionConfigOptionResponse::new(vec![]))
                    }),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: PromptRequest, responder, cx| {
                assert_eq!(request.session_id.0.as_ref(), "actual-session");
                assert_eq!(
                    request.prompt,
                    vec![ContentBlock::Text(TextContent::new("prompt"))]
                );
                assert_eq!(
                    prompted.config.load(Ordering::SeqCst),
                    usize::from(fixture.expected_set)
                );
                assert_eq!(prompted.prompt.fetch_add(1, Ordering::SeqCst), 0);
                if let Some(fresh) = fixture.fresh {
                    cx.send_notification(text("agent_message_chunk", fresh))?;
                }
                responder.respond(PromptResponse::new(StopReason::EndTurn))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            async move |request: CancelNotification, _| {
                assert_eq!(request.session_id.0.as_ref(), "actual-session");
                metrics.cancel.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .with_spawned(async move |_| {
            if matches!(ending, LoadEnding::Close) {
                close.started.notified().await;
                Err(SdkError::internal_error())
            } else {
                Ok(())
            }
        })
}
fn session_start(load: bool) -> KiroSessionStart {
    if load {
        KiroSessionStart::Load(load_request())
    } else {
        KiroSessionStart::New(request())
    }
}

async fn run_config(
    context: &Arc<SnapshotContext>,
    fixture: ConfigFixture,
    metrics: Arc<Metrics>,
    engine: KiroEngine,
    load: bool,
    policy: KiroLimits,
    sink: &mut impl KiroSink,
) -> Result<KiroReceipt, KiroFailure> {
    let (channel, peer) = Channel::duplex();
    let (result, peer_result) = tokio::join!(
        run_kiro(
            context,
            channel,
            engine,
            session_start(load),
            configured_prompt(),
            policy,
            sink
        ),
        config_agent(fixture, metrics).connect_with(peer, async |cx| {
            // Keep the fixture peer serving until it observes the drained incoming close.
            cx.incoming_closed().await;
            Ok(())
        })
    );
    assert!(peer_result.is_ok());
    result
}

#[tokio::test]
async fn native_offers_without_explicit_selection_cannot_automatically_send_set() {
    for load in [false, true] {
        let context = original();
        let metrics = Arc::new(Metrics::default());
        let receipt = run_kiro(
            &context,
            config_agent(
                ConfigFixture {
                    expected_set: false,
                    ..ConfigFixture::default()
                },
                metrics.clone(),
            ),
            KiroEngine::V3,
            session_start(load),
            KiroPrompt::new("prompt"),
            limits(),
            &mut Sink::default(),
        )
        .await
        .unwrap();
        assert_eq!(receipt.answer(), Some("ok"));
        assert!(receipt.config_response().is_none());
        assert_eq!(metrics.config.load(Ordering::SeqCst), 0);
        assert_eq!(metrics.prompt.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn config_pending_request_and_sink_preserve_original_cancel_deadline_and_no_replay() {
    for load in [false, true] {
        for pending_sink in [false, true] {
            for cancel in [false, true] {
                let deadline =
                    Instant::now() + Duration::from_millis(if cancel { 1000 } else { 100 });
                let context = Arc::new(SnapshotContext::new(deadline, Cancellation::default()));
                let metrics = Arc::new(Metrics::default());
                let entered = Arc::new(Notify::new());
                let drops = Arc::new(AtomicUsize::new(0));
                let mut sink = PendingSink {
                    entered: entered.clone(),
                    drops: drops.clone(),
                    calls: 0,
                };
                let fixture = ConfigFixture {
                    ending: LoadEnding::Pending,
                    events: if pending_sink {
                        vec![text("agent_message_chunk", "configuration")]
                    } else {
                        vec![]
                    },
                    ..ConfigFixture::default()
                };
                let interrupt = async {
                    tokio::select! {
                        _ = async {
                            if pending_sink {
                                entered.notified().await;
                            } else {
                                metrics.started.notified().await;
                            }
                        } => {},
                        _ = tokio::time::sleep_until(deadline.into()) => return,
                    }
                    if cancel {
                        context.cancellation().cancel();
                    }
                };
                let (result, _) = tokio::join!(
                    run_config(
                        &context,
                        fixture,
                        metrics.clone(),
                        KiroEngine::V3,
                        load,
                        limits(),
                        &mut sink
                    ),
                    interrupt
                );
                let error = result.unwrap_err();
                assert_eq!(
                    error.reason(),
                    KiroReason::Context(if cancel {
                        io::ErrorKind::Interrupted
                    } else {
                        io::ErrorKind::TimedOut
                    })
                );
                assert_eq!(error.receipt().new_response().is_some(), !load);
                assert_eq!(error.receipt().load_response().is_some(), load);
                assert!(error.receipt().config_response().is_none());
                assert!(error.receipt().response().is_none());
                assert!(error.receipt().candidate_text().is_none());
                assert_eq!(error.receipt().accepted(), 0);
                assert_eq!(drops.load(Ordering::SeqCst), usize::from(pending_sink));
                assert_eq!(sink.calls, usize::from(pending_sink));
                assert_eq!(metrics.config.load(Ordering::SeqCst), 1);
                assert_eq!(metrics.prompt.load(Ordering::SeqCst), 0);
                assert_eq!(metrics.cancel.load(Ordering::SeqCst), 1);
                assert!(error.receipt().cancel_enqueue().unwrap().is_ok());
            }
        }
    }
}

#[tokio::test]
async fn config_serial_barrier_precedes_response_and_prompt_then_preserves_native_receipt() {
    let context = original();
    let metrics = Arc::new(Metrics::default());
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let completed = AtomicUsize::new(0);
    let mut sink = BarrierSink {
        entered: entered.clone(),
        release: release.clone(),
        accepted: 0,
    };
    let work = async {
        let result = run_kiro(
            &context,
            config_agent(
                ConfigFixture {
                    events: vec![text("agent_message_chunk", "configuration")],
                    fresh: Some(""),
                    ..ConfigFixture::default()
                },
                metrics.clone(),
            ),
            KiroEngine::V3,
            session_start(true),
            configured_prompt(),
            limits(),
            &mut sink,
        )
        .await;
        completed.store(1, Ordering::SeqCst);
        result
    };
    let barrier = async {
        entered.notified().await;
        assert_eq!(metrics.config.load(Ordering::SeqCst), 1);
        assert_eq!(metrics.prompt.load(Ordering::SeqCst), 0);
        assert_eq!(completed.load(Ordering::SeqCst), 0);
        release.notify_one();
        entered.notified().await;
        release.notify_one();
    };
    let (result, _) = tokio::join!(work, barrier);
    let receipt = result.unwrap();
    assert_eq!(receipt.answer(), Some(""));
    assert!(receipt.load_response().is_some());
    assert!(receipt.config_response().is_some());
    assert_eq!(sink.accepted, 2);
}

#[tokio::test]
async fn observed_config_response_survives_original_cancel_during_fresh_sink() {
    let context = original();
    let metrics = Arc::new(Metrics::default());
    let entered = Arc::new(Notify::new());
    let drops = Arc::new(AtomicUsize::new(0));
    let mut sink = PendingSink {
        entered: entered.clone(),
        drops: drops.clone(),
        calls: 0,
    };
    let interrupt = async {
        entered.notified().await;
        context.cancellation().cancel();
    };
    let (result, _) = tokio::join!(
        run_config(
            &context,
            ConfigFixture::default(),
            metrics.clone(),
            KiroEngine::V3,
            true,
            limits(),
            &mut sink
        ),
        interrupt
    );
    let error = result.unwrap_err();
    assert!(matches!(error.reason(), KiroReason::Context(_)));
    assert!(error.receipt().load_response().is_some());
    assert!(error.receipt().config_response().is_some());
    assert!(error.receipt().response().is_none());
    assert!(error.receipt().answer().is_none());
    assert_eq!(error.receipt().accepted(), 0);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(metrics.cancel.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn config_peer_driver_failure_during_pending_request_never_fabricates_answer() {
    let context = original();
    let metrics = Arc::new(Metrics::default());
    let error = run_kiro(
        &context,
        config_agent(
            ConfigFixture {
                ending: LoadEnding::Close,
                ..ConfigFixture::default()
            },
            metrics.clone(),
        ),
        KiroEngine::V3,
        session_start(true),
        configured_prompt(),
        limits(),
        &mut Sink::default(),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error.reason(),
        KiroReason::Sdk | KiroReason::IncomingClosed
    ));
    assert!(error.receipt().load_response().is_some());
    assert!(error.receipt().config_response().is_none());
    assert!(error.receipt().answer().is_none());
    assert_eq!(metrics.prompt.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn config_payload_tracing_is_private_for_success_error_cancel_and_future_drop() {
    use tracing::instrument::WithSubscriber;
    let positive = Arc::new(TraceCounts::default());
    let capture = tracing::Dispatch::new(CaptureSubscriber(positive.clone()));
    Client
        .builder()
        .on_receive_notification(
            async |_: SessionNotification, _| Ok(()),
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(
            config_agent(ConfigFixture::default(), Arc::new(Metrics::default())),
            async |cx| {
                cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                let session = cx
                    .send_request(NewSessionRequest::new("/fixture/project"))
                    .block_task()
                    .await?;
                let choice = selection();
                cx.send_request(
                    SetSessionConfigOptionRequest::new(
                        session.session_id.clone(),
                        choice.config_id,
                        choice.value_id,
                    )
                    .meta(choice.meta),
                )
                .block_task()
                .await?;
                cx.send_request(PromptRequest::new(
                    session.session_id,
                    vec![ContentBlock::Text(TextContent::new("prompt"))],
                ))
                .block_task()
                .await?;
                Ok(())
            },
        )
        .with_subscriber(capture)
        .await
        .unwrap();
    assert!(positive.sdk_payload.load(Ordering::SeqCst) > 0);
    for mode in 0..4 {
        let counts = Arc::new(TraceCounts::default());
        let capture = tracing::Dispatch::new(CaptureSubscriber(counts.clone()));
        let drops = Arc::new(AtomicUsize::new(0));
        async {
            tracing::trace!(target:"kiro-outside", "outside-before");
            let context=original(); let metrics=Arc::new(Metrics::default());
            let fixture=ConfigFixture { ending: if mode==1 { LoadEnding::Error } else if mode>=2 { LoadEnding::Pending } else { LoadEnding::Response },
                events: if mode==3 { vec![text("agent_message_chunk", "synthetic-secret-config-update")] } else { vec![] }, ..ConfigFixture::default() };
            let transport=TracedTransport { inner:Some(config_agent(fixture,metrics.clone())), _drop:TracedDrop(drops.clone()) };
            if mode<3 {
                let mut sink=Sink::default();
                let interrupt=async { if mode==2 { metrics.started.notified().await; context.cancellation().cancel(); } };
                let (result,_)=tokio::join!(run_kiro(&context,transport,KiroEngine::V3,session_start(true),configured_prompt(),limits(),&mut sink),interrupt);
                if mode==0 { assert_eq!(result.unwrap().answer(),Some("ok")); }
                else if mode==1 { let error=result.unwrap_err(); assert_eq!(error.reason(),KiroReason::Sdk);
                    assert!(!format!("{error:?} {error}").contains("synthetic-secret")); }
                else { assert!(matches!(result.unwrap_err().reason(),KiroReason::Context(_))); }
            } else {
                let entered=Arc::new(Notify::new());let callback_drops=Arc::new(AtomicUsize::new(0));
                let mut sink=PendingSink { entered:entered.clone(),drops:callback_drops.clone(),calls:0 };
                let mut work=Box::pin(run_kiro(&context,transport,KiroEngine::V3,session_start(true),configured_prompt(),limits(),&mut sink));
                tokio::select! { _=entered.notified()=>{}, _=&mut work=>panic!("configuration must be pending before Drop") }
                drop(work); assert_eq!(callback_drops.load(Ordering::SeqCst),1);
            }
            assert_eq!(metrics.config.load(Ordering::SeqCst),1);
            tracing::debug!(target:"kiro-outside", "outside-after");
        }.with_subscriber(capture).await;
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert_eq!(counts.sdk.load(Ordering::SeqCst), 0);
        assert_eq!(counts.sdk_payload.load(Ordering::SeqCst), 0);
        assert_eq!(counts.drop_payload.load(Ordering::SeqCst), 0);
        assert_eq!(counts.outside.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn config_select_new_load_flat_grouped_categories_and_full_native_state() {
    for load in [false, true] {
        for grouped in [false, true] {
            for category in [
                None,
                Some(SessionConfigOptionCategory::Other("_opaque".into())),
                Some(SessionConfigOptionCategory::Model),
            ] {
                let context = original();
                let offer = select_offer(grouped, "previous-value").category(category.clone());
                let selected = select_offer(grouped, "fixture-value!").category(category);
                let dependent = SessionConfigOption::select(
                    "dependent",
                    "Dependent changed",
                    "new-dependent",
                    vec![SessionConfigSelectOption::new(
                        "new-dependent",
                        "New dependent",
                    )],
                );
                let returned = vec![selected, dependent];
                let update = SessionUpdate::ConfigOptionUpdate(
                    ConfigOptionUpdate::new(returned.clone()).meta(config_meta()),
                );
                let fixture = ConfigFixture {
                    offers: Some(vec![offer.clone()]),
                    returned: returned.clone(),
                    events: vec![
                        text(
                            "agent_message_chunk",
                            "configuration larger than response retention",
                        ),
                        UntypedMessage::new(
                            "session/update",
                            SessionNotification::new("actual-session", update.clone()),
                        )
                        .unwrap(),
                    ],
                    ..ConfigFixture::default()
                };
                let metrics = Arc::new(Metrics::default());
                let mut sink = Sink {
                    original: Some(context.clone()),
                    ..Sink::default()
                };
                let mut policy = limits();
                policy.response_text_bytes = 2;
                let receipt = run_kiro(
                    &context,
                    config_agent(fixture, metrics.clone()),
                    KiroEngine::V3,
                    session_start(load),
                    configured_prompt(),
                    policy,
                    &mut sink,
                )
                .await
                .unwrap();
                assert_eq!(receipt.answer(), Some("ok"));
                assert_eq!(receipt.config_response().unwrap().config_options, returned);
                assert_eq!(receipt.config_response().unwrap().meta, Some(config_meta()));
                let actual_offers = if load {
                    receipt
                        .load_response()
                        .unwrap()
                        .config_options
                        .as_ref()
                        .unwrap()
                } else {
                    receipt
                        .new_response()
                        .unwrap()
                        .config_options
                        .as_ref()
                        .unwrap()
                };
                assert_eq!(actual_offers, &vec![offer]);
                assert_eq!(receipt.new_response().is_some(), !load);
                assert_eq!(receipt.load_response().is_some(), load);
                assert_eq!(
                    sink.phases,
                    vec![
                        KiroDeliveryPhase::Configuring,
                        KiroDeliveryPhase::Configuring,
                        KiroDeliveryPhase::Prompting
                    ]
                );
                assert_eq!(sink.seen[1], update);
                assert_eq!(receipt.accepted(), 3);
                assert_eq!(
                    metrics.new_session.load(Ordering::SeqCst),
                    usize::from(!load)
                );
                assert_eq!(
                    metrics.load_session.load(Ordering::SeqCst),
                    usize::from(load)
                );
                assert_eq!(metrics.config.load(Ordering::SeqCst), 1);
                assert_eq!(metrics.prompt.load(Ordering::SeqCst), 1);
                assert_eq!(metrics.cancel.load(Ordering::SeqCst), 0);
                assert!(!format!("{receipt:?}").contains("synthetic-secret"));
            }
        }
    }
}

#[tokio::test]
async fn config_offer_rejections_have_distinct_reasons_and_zero_set_prompt() {
    for load in [false, true] {
        for case in 0..5 {
            let offers = match case {
                0 => None,
                1 => Some(vec![]),
                2 => Some(vec![
                    select_offer(false, "previous"),
                    select_offer(true, "previous"),
                ]),
                3 => Some(vec![SessionConfigOption::boolean(
                    "fixture-select",
                    "Unsupported",
                    true,
                )]),
                _ => Some(vec![SessionConfigOption::select(
                    "fixture-select",
                    "Literal",
                    "previous",
                    vec![SessionConfigSelectOption::new("other-value", "Other")],
                )]),
            };
            let context = original();
            let metrics = Arc::new(Metrics::default());
            let error = run_config(
                &context,
                ConfigFixture {
                    offers,
                    ..ConfigFixture::default()
                },
                metrics.clone(),
                KiroEngine::V3,
                load,
                limits(),
                &mut Sink::default(),
            )
            .await
            .unwrap_err();
            assert_eq!(
                error.reason(),
                [
                    KiroReason::UnsupportedConfiguration,
                    KiroReason::ConfigurationNotFound,
                    KiroReason::AmbiguousConfiguration,
                    KiroReason::UnsupportedConfigurationKind,
                    KiroReason::ConfigurationValueNotOffered
                ][case]
            );
            assert_eq!(error.receipt().new_response().is_some(), !load);
            assert_eq!(error.receipt().load_response().is_some(), load);
            assert!(error.receipt().config_response().is_none());
            assert_eq!(metrics.config.load(Ordering::SeqCst), 0);
            assert_eq!(metrics.prompt.load(Ordering::SeqCst), 0);
            assert_eq!(metrics.cancel.load(Ordering::SeqCst), 1);
            assert!(!format!("{error:?}").contains("synthetic-secret"));
        }
    }
}

#[tokio::test]
async fn config_id_caps_and_original_are_checked_before_transport() {
    for case in 0..4 {
        let context = if case == 3 {
            Arc::new(SnapshotContext::new(
                Instant::now(),
                Cancellation::default(),
            ))
        } else {
            original()
        };
        if case == 2 {
            context.cancellation().cancel();
        }
        let mut prompt = configured_prompt();
        let mut policy = limits();
        policy.id_bytes = 32;
        if case == 0 {
            prompt.selection.as_mut().unwrap().config_id = SessionConfigId::new("x".repeat(33));
        }
        if case == 1 {
            prompt.selection.as_mut().unwrap().value_id = SessionConfigValueId::new("x".repeat(33));
        }
        let (channel, _peer) = Channel::duplex();
        let calls = Arc::new(AtomicUsize::new(0));
        let error = run_kiro(
            &context,
            CountedTransport {
                channel,
                calls: calls.clone(),
            },
            KiroEngine::V3,
            session_start(false),
            prompt,
            policy,
            &mut Sink::default(),
        )
        .await
        .unwrap_err();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(error.receipt().driver().is_none());
        if case < 2 {
            assert_eq!(error.reason(), KiroReason::IdLimit);
        } else {
            assert!(matches!(error.reason(), KiroReason::Context(_)));
        }
    }
    // Exact caps admit the literal identifiers and perform the actual Set.
    let context = original();
    let metrics = Arc::new(Metrics::default());
    let mut policy = limits();
    policy.id_bytes = "fixture-select".len();
    let receipt = run_kiro(
        &context,
        config_agent(ConfigFixture::default(), metrics.clone()),
        KiroEngine::V3,
        session_start(false),
        configured_prompt(),
        policy,
        &mut Sink::default(),
    )
    .await
    .unwrap();
    assert_eq!(receipt.answer(), Some("ok"));
    assert_eq!(metrics.config.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn config_native_error_and_unconfirmed_response_never_send_prompt() {
    for load in [false, true] {
        for case in 0..6 {
            let returned = match case {
                1 => vec![],
                2 => vec![
                    select_offer(false, "fixture-value!"),
                    select_offer(true, "fixture-value!"),
                ],
                3 => vec![SessionConfigOption::boolean(
                    "fixture-select",
                    "Unsupported",
                    true,
                )],
                4 => vec![select_offer(false, "native-other-value")],
                5 => vec![SessionConfigOption::select(
                    "fixture-select",
                    "No longer offered",
                    "fixture-value!",
                    vec![SessionConfigSelectOption::new("other", "Other")],
                )],
                _ => vec![select_offer(false, "fixture-value!")],
            };
            let context = original();
            let metrics = Arc::new(Metrics::default());
            let error = run_config(
                &context,
                ConfigFixture {
                    returned: returned.clone(),
                    ending: if case == 0 {
                        LoadEnding::Error
                    } else {
                        LoadEnding::Response
                    },
                    ..ConfigFixture::default()
                },
                metrics.clone(),
                KiroEngine::V3,
                load,
                limits(),
                &mut Sink::default(),
            )
            .await
            .unwrap_err();
            if case == 0 {
                assert_eq!(error.reason(), KiroReason::Sdk);
                assert!(error.sdk_error().is_some());
                assert!(error.receipt().config_response().is_none());
            } else {
                assert_eq!(error.reason(), KiroReason::UnconfirmedConfiguration);
                assert_eq!(
                    error.receipt().config_response().unwrap().config_options,
                    returned
                );
                assert_eq!(
                    error.receipt().config_response().unwrap().meta,
                    Some(config_meta())
                );
            }
            assert!(error.receipt().answer().is_none());
            assert_eq!(metrics.config.load(Ordering::SeqCst), 1);
            assert_eq!(metrics.prompt.load(Ordering::SeqCst), 0);
            assert_eq!(metrics.cancel.load(Ordering::SeqCst), 1);
            assert_eq!(
                metrics.new_session.load(Ordering::SeqCst),
                usize::from(!load)
            );
            assert_eq!(
                metrics.load_session.load(Ordering::SeqCst),
                usize::from(load)
            );
        }
    }
}

#[tokio::test]
async fn configuring_text_cannot_substitute_absent_fresh_text_or_empty_fresh_text() {
    for fresh in [None, Some("")] {
        let context = original();
        let metrics = Arc::new(Metrics::default());
        let result = run_kiro(
            &context,
            config_agent(
                ConfigFixture {
                    events: vec![text("agent_message_chunk", "configuration")],
                    fresh,
                    ..ConfigFixture::default()
                },
                metrics,
            ),
            KiroEngine::V3,
            session_start(false),
            configured_prompt(),
            limits(),
            &mut Sink::default(),
        )
        .await;
        if fresh.is_some() {
            assert_eq!(result.unwrap().answer(), Some(""));
        } else {
            let error = result.unwrap_err();
            assert_eq!(error.reason(), KiroReason::MissingText);
            assert!(error.receipt().candidate_text().is_none());
            assert!(error.receipt().config_response().is_some());
            assert!(error.receipt().response().is_some());
        }
    }
}
enum Terminal {
    Response(StopReason),
    Error,
    Pending,
    Close,
}
async fn agent(
    peer: Channel,
    mut events: Vec<UntypedMessage>,
    terminal: Terminal,
    metrics: Arc<Metrics>,
) -> Result<(), SdkError> {
    Agent
        .builder()
        .on_receive_request(
            async |request: InitializeRequest, responder, _| {
                assert_eq!(request.protocol_version, ProtocolVersion::V1);
                metrics.initialize.fetch_add(1, Ordering::SeqCst);
                responder.respond(InitializeResponse::new(ProtocolVersion::V1))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async |request: NewSessionRequest, responder, cx| {
                assert_eq!(request.cwd, std::path::PathBuf::from("/fixture/project"));
                assert_eq!(request.mcp_servers, mcp_fixture());
                assert_eq!(
                    request.meta,
                    Some(
                        json!({"kiro":{"modeId":"fixture-mode","modelId":"fixture-model"}})
                            .as_object()
                            .unwrap()
                            .clone()
                    )
                );
                metrics.new_session.fetch_add(1, Ordering::SeqCst);
                if metrics.oversized_session {
                    return responder.respond(NewSessionResponse::new("over-cap-session"));
                }
                // Delivered before new-session response; it cannot become an answer.
                cx.send_notification(text("agent_message_chunk", "before-session"))?;
                responder.respond(NewSessionResponse::new("actual-session"))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async |request: PromptRequest, responder, cx| {
                assert_eq!(request.session_id.0.as_ref(), "actual-session");
                assert_eq!(
                    request.prompt,
                    vec![ContentBlock::Text(TextContent::new("prompt"))]
                );
                metrics.prompt.fetch_add(1, Ordering::SeqCst);
                for event in events.drain(..) {
                    cx.send_notification(event)?;
                }
                metrics.started.notify_one();
                match &terminal {
                    Terminal::Response(reason) => responder.respond(
                        PromptResponse::new(*reason).meta(
                            json!({"private":"synthetic-secret-fixture"})
                                .as_object()
                                .unwrap()
                                .clone(),
                        ),
                    ),
                    Terminal::Error => {
                        responder.respond_with_internal_error("synthetic-secret-fixture")
                    }
                    Terminal::Pending | Terminal::Close => cx.spawn(async move {
                        // A genuinely pending SDK request, owned by this fixture driver.
                        std::future::pending::<()>().await;
                        responder.respond(PromptResponse::new(StopReason::EndTurn))
                    }),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            async |request: CancelNotification, _| {
                assert_eq!(
                    request.session_id.0.as_ref(),
                    if metrics.oversized_session {
                        "over-cap-session"
                    } else {
                        "actual-session"
                    }
                );
                metrics.cancel.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(peer, async |cx| {
            if matches!(terminal, Terminal::Close) {
                metrics.started.notified().await;
            } else {
                cx.incoming_closed().await;
            }
            Ok(())
        })
        .await
}
fn request() -> NewSessionRequest {
    NewSessionRequest::new("/fixture/project")
        .mcp_servers(mcp_fixture())
        .meta(
            json!({"kiro":{"modeId":"fixture-mode","modelId":"fixture-model"}})
                .as_object()
                .unwrap()
                .clone(),
        )
}
fn mcp_fixture() -> Vec<McpServer> {
    vec![McpServer::Stdio(
        McpServerStdio::new("fixture-mcp", "/fixture/server").args(vec!["--literal".into()]),
    )]
}
#[derive(Default)]
struct Sink {
    seen: Vec<SessionUpdate>,
    phases: Vec<KiroDeliveryPhase>,
    original: Option<Arc<SnapshotContext>>,
    error: bool,
}
impl KiroSink for Sink {
    async fn push(
        &mut self,
        phase: KiroDeliveryPhase,
        event: &SessionNotification,
        original: &SnapshotContext,
    ) -> io::Result<()> {
        if let Some(expected) = &self.original {
            assert!(std::ptr::eq(expected.as_ref(), original));
        }
        if self.error {
            return Err(io::Error::other("synthetic-secret-fixture"));
        }
        self.seen.push(event.update.clone());
        self.phases.push(phase);
        Ok(())
    }
}
async fn run(
    events: Vec<UntypedMessage>,
    terminal: Terminal,
    policy: KiroLimits,
    sink: &mut impl KiroSink,
    engine: KiroEngine,
    context: Arc<SnapshotContext>,
) -> (Result<KiroReceipt, KiroFailure>, Arc<Metrics>) {
    let (client, peer) = Channel::duplex();
    let metrics = Arc::new(Metrics::default());
    let (result, _) = tokio::join!(
        run_kiro(
            &context,
            client,
            engine,
            KiroSessionStart::New(request()),
            KiroPrompt::new("prompt"),
            policy,
            sink
        ),
        agent(peer, events, terminal, metrics.clone())
    );
    (result, metrics)
}

#[tokio::test]
async fn engines_stream_order_session_route_metadata_and_native_terminal() {
    {
        let engine = KiroEngine::V3;
        let context = original();
        let mut sink = Sink {
            original: Some(context.clone()),
            ..Sink::default()
        };
        let mut wrong = text("agent_message_chunk", "wrong-session");
        let (_, mut value) = wrong.into_parts();
        value["sessionId"] = json!("foreign");
        wrong = UntypedMessage::new("session/update", value).unwrap();
        let events = vec![
            text("user_message_chunk", "user"), text("agent_thought_chunk", "thought"), wrong,
            text("agent_message_chunk", "a"),
            UntypedMessage::new("session/update", SessionNotification::new("actual-session", SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Image(ImageContent::new("AA==", "image/png")))))).unwrap(),
            UntypedMessage::new("session/update", json!({"sessionId":"actual-session","update":{"sessionUpdate":"tool_call","toolCallId":"tool-1","title":"fixture","kind":"read","status":"pending"}})).unwrap(),
            text("agent_message_chunk", "b"),
        ];
        let (result, metrics) = run(
            events,
            Terminal::Response(StopReason::EndTurn),
            limits(),
            &mut sink,
            engine,
            context,
        )
        .await;
        let receipt = result.unwrap();
        assert_eq!(receipt.answer(), Some("ab"));
        assert_eq!(receipt.engine(), engine);
        assert_eq!(receipt.observed(), 8);
        assert_eq!(receipt.accepted(), 6);
        assert_eq!(sink.seen.len(), 6);
        assert!(receipt.driver().unwrap().is_ok());
        assert_eq!(
            receipt.response().unwrap().meta.as_ref().unwrap()["private"],
            json!("synthetic-secret-fixture")
        );
        assert_eq!(metrics.initialize.load(Ordering::SeqCst), 1);
        assert_eq!(metrics.new_session.load(Ordering::SeqCst), 1);
        assert_eq!(metrics.prompt.load(Ordering::SeqCst), 1);
        assert_eq!(metrics.cancel.load(Ordering::SeqCst), 0);
        assert!(!format!("{receipt:?}").contains("synthetic-secret-fixture"));
    }
}

#[tokio::test]
async fn unknown_valid_extensions_do_not_hide_known_malformed_data() {
    let events = vec![UntypedMessage::new("vendor/progress", json!({"opaque":"synthetic-secret-fixture"})).unwrap(),
        UntypedMessage::new("session/update", json!({"sessionId":"actual-session","update":{"sessionUpdate":"vendor_new_update","opaque":7}})).unwrap(), text("agent_message_chunk", "answer")];
    let (result, _) = run(
        events,
        Terminal::Response(StopReason::EndTurn),
        limits(),
        &mut Sink::default(),
        KiroEngine::V3,
        original(),
    )
    .await;
    let receipt = result.unwrap();
    assert_eq!(receipt.answer(), Some("answer"));
    assert_eq!(receipt.accepted(), 1);
    assert_eq!(receipt.observed(), 4);
    let malformed = UntypedMessage::new("session/update", json!({"sessionId":"actual-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":false}}})).unwrap();
    let (result, metrics) = run(
        vec![malformed],
        Terminal::Pending,
        limits(),
        &mut Sink::default(),
        KiroEngine::V3,
        original(),
    )
    .await;
    let error = result.unwrap_err();
    assert_eq!(error.reason(), KiroReason::Wire);
    assert!(error.decode_error().is_some());
    assert!(error.receipt().answer().is_none());
    assert!(error.receipt().cancel_enqueue().unwrap().is_ok());
    assert_eq!(metrics.cancel.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn empty_text_absence_and_non_end_turn_are_distinct() {
    let (result, _) = run(
        vec![text("agent_message_chunk", "")],
        Terminal::Response(StopReason::EndTurn),
        limits(),
        &mut Sink::default(),
        KiroEngine::V3,
        original(),
    )
    .await;
    assert_eq!(result.unwrap().answer(), Some(""));
    let (result, _) = run(
        vec![],
        Terminal::Response(StopReason::EndTurn),
        limits(),
        &mut Sink::default(),
        KiroEngine::V3,
        original(),
    )
    .await;
    let error = result.unwrap_err();
    assert_eq!(error.reason(), KiroReason::MissingText);
    assert!(error.receipt().candidate_text().is_none());
    assert!(error.receipt().response().is_some());
    for reason in [
        StopReason::Refusal,
        StopReason::MaxTokens,
        StopReason::MaxTurnRequests,
        StopReason::Cancelled,
    ] {
        let (result, _) = run(
            vec![text("agent_message_chunk", "candidate")],
            Terminal::Response(reason),
            limits(),
            &mut Sink::default(),
            KiroEngine::V3,
            original(),
        )
        .await;
        let receipt = result.unwrap();
        assert!(receipt.answer().is_none());
        assert_eq!(receipt.response().unwrap().stop_reason, reason);
        assert_eq!(receipt.candidate_text(), Some("candidate"));
    }
}

#[tokio::test]
async fn actual_sdk_error_and_sink_cause_are_private_and_never_answers() {
    let (result, _) = run(
        vec![text("agent_message_chunk", "candidate")],
        Terminal::Error,
        limits(),
        &mut Sink::default(),
        KiroEngine::V3,
        original(),
    )
    .await;
    let error = result.unwrap_err();
    assert_eq!(error.reason(), KiroReason::Sdk);
    assert!(error.sdk_error().is_some());
    assert!(error.receipt().answer().is_none());
    assert!(error.receipt().response().is_none());
    assert!(!format!("{error:?} {error}").contains("synthetic-secret-fixture"));
    let (result, metrics) = run(
        vec![text("agent_message_chunk", "candidate")],
        Terminal::Pending,
        limits(),
        &mut Sink {
            error: true,
            ..Sink::default()
        },
        KiroEngine::V3,
        original(),
    )
    .await;
    let error = result.unwrap_err();
    assert_eq!(error.reason(), KiroReason::Sink(io::ErrorKind::Other));
    assert_eq!(
        error.io_error().unwrap().to_string(),
        "synthetic-secret-fixture"
    );
    assert_eq!(error.receipt().accepted(), 0);
    assert_eq!(metrics.cancel.load(Ordering::SeqCst), 1);
    assert!(!format!("{error:?} {error}").contains("synthetic-secret-fixture"));
}

#[tokio::test]
async fn actual_peer_close_without_prompt_response_is_not_terminal_success() {
    let (result, _) = run(
        vec![text("agent_message_chunk", "candidate")],
        Terminal::Close,
        limits(),
        &mut Sink::default(),
        KiroEngine::V3,
        original(),
    )
    .await;
    let error = result.unwrap_err();
    assert!(matches!(
        error.reason(),
        KiroReason::Sdk | KiroReason::IncomingClosed
    ));
    assert!(error.receipt().answer().is_none());
    assert!(error.receipt().response().is_none());
}

struct CountedTransport {
    channel: Channel,
    calls: Arc<AtomicUsize>,
}
impl ConnectTo<Client> for CountedTransport {
    fn connect_to(
        self,
        client: impl ConnectTo<Agent>,
    ) -> impl Future<Output = Result<(), SdkError>> + Send {
        self.calls.fetch_add(1, Ordering::SeqCst);
        <Channel as ConnectTo<Client>>::connect_to(self.channel, client)
    }
}
#[tokio::test]
async fn admission_rejects_expired_cancelled_prompt_over_cap_and_zero_policy_without_transport() {
    for case in 0..4 {
        let context = if case == 0 {
            Arc::new(SnapshotContext::new(
                Instant::now(),
                Cancellation::default(),
            ))
        } else {
            original()
        };
        if case == 1 {
            context.cancellation().cancel();
        }
        let mut policy = limits();
        if case == 2 {
            policy.prompt_text_bytes = 5;
        }
        if case == 3 {
            policy.id_bytes = 0;
        }
        let (channel, _peer) = Channel::duplex();
        let calls = Arc::new(AtomicUsize::new(0));
        let error = run_kiro(
            &context,
            CountedTransport {
                channel,
                calls: calls.clone(),
            },
            KiroEngine::V3,
            KiroSessionStart::New(request()),
            KiroPrompt::new("prompt"),
            policy,
            &mut Sink::default(),
        )
        .await
        .unwrap_err();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(error.receipt().driver().is_none());
        match case {
            0 | 1 => assert!(matches!(error.reason(), KiroReason::Context(_))),
            2 => assert_eq!(error.reason(), KiroReason::PromptLimit),
            _ => assert_eq!(error.reason(), KiroReason::Policy),
        }
    }
}

struct PendingSink {
    entered: Arc<Notify>,
    drops: Arc<AtomicUsize>,
    calls: usize,
}
struct DropMarker(Arc<AtomicUsize>);
impl Drop for DropMarker {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl KiroSink for PendingSink {
    async fn push(
        &mut self,
        _: KiroDeliveryPhase,
        _: &SessionNotification,
        _: &SnapshotContext,
    ) -> io::Result<()> {
        self.calls += 1;
        let _guard = DropMarker(self.drops.clone());
        self.entered.notify_one();
        std::future::pending().await
    }
}
#[tokio::test]
async fn pending_sink_original_cancel_and_deadline_drop_once_send_actual_cancel_and_never_replay() {
    for cancel in [true, false] {
        let context = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_millis(if cancel { 1000 } else { 100 }),
            Cancellation::default(),
        ));
        let entered = Arc::new(Notify::new());
        let drops = Arc::new(AtomicUsize::new(0));
        let mut sink = PendingSink {
            entered: entered.clone(),
            drops: drops.clone(),
            calls: 0,
        };
        let work = run(
            vec![
                text("agent_message_chunk", "pending"),
                text("agent_message_chunk", "never-accepted"),
            ],
            Terminal::Pending,
            limits(),
            &mut sink,
            KiroEngine::V3,
            context.clone(),
        );
        let interrupt = async {
            entered.notified().await;
            if cancel {
                context.cancellation().cancel();
            }
        };
        let ((result, metrics), _) = tokio::join!(work, interrupt);
        let error = result.unwrap_err();
        assert!(matches!(error.reason(), KiroReason::Context(_)));
        assert!(error.io_error().is_some());
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert_eq!(sink.calls, 1);
        assert_eq!(error.receipt().accepted(), 0);
        assert_eq!(metrics.prompt.load(Ordering::SeqCst), 1);
        assert_eq!(metrics.cancel.load(Ordering::SeqCst), 1);
        assert!(error.receipt().cancel_enqueue().unwrap().is_ok());
        assert!(error.receipt().response().is_none());
        assert!(error.receipt().answer().is_none());
    }
}

#[tokio::test]
async fn response_notification_and_identifiers_accept_exact_caps_reject_plus_one() {
    for extra in [false, true] {
        let mut policy = limits();
        policy.prompt_text_bytes = 6;
        policy.response_text_bytes = 3;
        let (result, _) = run(
            vec![text(
                "agent_message_chunk",
                if extra { "four" } else { "abc" },
            )],
            Terminal::Response(StopReason::EndTurn),
            policy,
            &mut Sink::default(),
            KiroEngine::V3,
            original(),
        )
        .await;
        if extra {
            assert_eq!(result.unwrap_err().reason(), KiroReason::ResponseLimit);
        } else {
            assert_eq!(result.unwrap().answer(), Some("abc"));
        }
        let mut policy = limits();
        policy.notifications = if extra { 1 } else { 2 };
        let (result, _) = run(
            vec![text("agent_message_chunk", "ok")],
            Terminal::Response(StopReason::EndTurn),
            policy,
            &mut Sink::default(),
            KiroEngine::V3,
            original(),
        )
        .await;
        if extra {
            assert_eq!(result.unwrap_err().reason(), KiroReason::NotificationLimit);
        } else {
            assert_eq!(result.unwrap().observed(), 2);
        }
        let mut policy = limits();
        policy.id_bytes = if extra { 13 } else { 14 };
        let (result, metrics) = run(
            vec![text("agent_message_chunk", "ok")],
            Terminal::Response(StopReason::EndTurn),
            policy,
            &mut Sink::default(),
            KiroEngine::V3,
            original(),
        )
        .await;
        if extra {
            assert_eq!(result.unwrap_err().reason(), KiroReason::IdLimit);
            assert_eq!(metrics.prompt.load(Ordering::SeqCst), 0);
        } else {
            assert_eq!(result.unwrap().answer(), Some("ok"));
        }
    }
    for field in ["messageId", "toolCallId"] {
        for size in [14, 15] {
            let mut policy = limits();
            policy.id_bytes = 14;
            let mut value = json!({"sessionId":"actual-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"ok"}}});
            value["update"][field] = json!("x".repeat(size));
            let (result, _) = run(
                vec![UntypedMessage::new("session/update", value).unwrap()],
                Terminal::Response(StopReason::EndTurn),
                policy,
                &mut Sink::default(),
                KiroEngine::V3,
                original(),
            )
            .await;
            if size == 15 {
                assert_eq!(result.unwrap_err().reason(), KiroReason::IdLimit);
            } else {
                assert_eq!(result.unwrap().answer(), Some("ok"));
            }
        }
    }
}

struct BarrierSink {
    entered: Arc<Notify>,
    release: Arc<Notify>,
    accepted: usize,
}
impl KiroSink for BarrierSink {
    async fn push(
        &mut self,
        _: KiroDeliveryPhase,
        _: &SessionNotification,
        _: &SnapshotContext,
    ) -> io::Result<()> {
        self.entered.notify_one();
        self.release.notified().await;
        self.accepted += 1;
        Ok(())
    }
}
#[tokio::test]
async fn pending_serial_delivery_precedes_native_prompt_response() {
    let context = original();
    let (channel, peer) = Channel::duplex();
    let metrics = Arc::new(Metrics::default());
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let mut sink = BarrierSink {
        entered: entered.clone(),
        release: release.clone(),
        accepted: 0,
    };
    let completed = AtomicUsize::new(0);
    let work = async {
        let result = run_kiro(
            &context,
            channel,
            KiroEngine::V3,
            KiroSessionStart::New(request()),
            KiroPrompt::new("prompt"),
            limits(),
            &mut sink,
        )
        .await;
        completed.store(1, Ordering::SeqCst);
        result
    };
    let barrier = async {
        entered.notified().await;
        assert_eq!(completed.load(Ordering::SeqCst), 0);
        release.notify_one();
    };
    let (result, _, _) = tokio::join!(
        work,
        agent(
            peer,
            vec![text("agent_message_chunk", "answer")],
            Terminal::Response(StopReason::EndTurn),
            metrics
        ),
        barrier
    );
    assert_eq!(result.unwrap().answer(), Some("answer"));
    assert_eq!(sink.accepted, 1);
}

#[tokio::test]
async fn original_interrupts_pending_sdk_response_and_cancel_is_received() {
    for cancel in [true, false] {
        let context = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_millis(if cancel { 1000 } else { 100 }),
            Cancellation::default(),
        ));
        let (channel, peer) = Channel::duplex();
        let metrics = Arc::new(Metrics::default());
        let mut sink = Sink::default();
        let work = run_kiro(
            &context,
            channel,
            KiroEngine::V3,
            KiroSessionStart::New(request()),
            KiroPrompt::new("prompt"),
            limits(),
            &mut sink,
        );
        let interrupt = async {
            metrics.started.notified().await;
            if cancel {
                context.cancellation().cancel();
            }
        };
        let (result, _, _) = tokio::join!(
            work,
            agent(peer, vec![], Terminal::Pending, metrics.clone()),
            interrupt
        );
        let error = result.unwrap_err();
        assert!(matches!(error.reason(), KiroReason::Context(_)));
        assert_eq!(error.receipt().accepted(), 0);
        assert!(error.receipt().response().is_none());
        assert!(error.receipt().answer().is_none());
        assert!(error.receipt().driver().unwrap().is_ok());
        assert!(error.receipt().cancel_enqueue().unwrap().is_ok());
        assert_eq!(metrics.cancel.load(Ordering::SeqCst), 1);
        assert_eq!(metrics.prompt.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn actual_created_session_over_id_cap_is_cancelled_without_retention_or_prompt() {
    let context = original();
    let (channel, peer) = Channel::duplex();
    let metrics = Arc::new(Metrics {
        oversized_session: true,
        ..Metrics::default()
    });
    let mut policy = limits();
    policy.id_bytes = 14;
    let mut sink = Sink::default();
    let (result, _) = tokio::join!(
        run_kiro(
            &context,
            channel,
            KiroEngine::V3,
            KiroSessionStart::New(request()),
            KiroPrompt::new("prompt"),
            policy,
            &mut sink
        ),
        agent(peer, vec![], Terminal::Pending, metrics.clone())
    );
    let error = result.unwrap_err();
    assert_eq!(error.reason(), KiroReason::IdLimit);
    assert!(error.receipt().session().is_none());
    assert!(error.receipt().answer().is_none());
    assert!(error.receipt().cancel_enqueue().unwrap().is_ok());
    assert!(error.receipt().driver().unwrap().is_ok());
    assert_eq!(metrics.new_session.load(Ordering::SeqCst), 1);
    assert_eq!(metrics.prompt.load(Ordering::SeqCst), 0);
    assert_eq!(metrics.cancel.load(Ordering::SeqCst), 1);
    assert!(sink.seen.is_empty());
}

#[test]
fn decoder_rejects_malformed_envelope_and_known_variant_but_ignores_valid_unknown() {
    for value in [
        json!({"update":{"sessionUpdate":"new"}}),
        json!({"sessionId":7,"update":{"sessionUpdate":"new"}}),
        json!({"sessionId":"s","update":false}),
        json!({"sessionId":"s","update":{"sessionUpdate":false}}),
    ] {
        assert!(matches!(
            decode_notification(UntypedMessage::new("session/update", value).unwrap(), 64),
            Err(KiroWireError::Decode(_))
        ));
    }
    assert!(matches!(
        decode_notification(
            UntypedMessage::new(
                "session/update",
                json!({"sessionId":"s","update":{"sessionUpdate":"new"}})
            )
            .unwrap(),
            64
        ),
        Ok(None)
    ));
}

#[derive(Default)]
struct TraceCounts {
    sdk: AtomicUsize,
    sdk_payload: AtomicUsize,
    drop_payload: AtomicUsize,
    outside: AtomicUsize,
}
struct CaptureSubscriber(Arc<TraceCounts>);
impl tracing::Subscriber for CaptureSubscriber {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn max_level_hint(&self) -> Option<tracing::metadata::LevelFilter> {
        Some(tracing::metadata::LevelFilter::TRACE)
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let target = event.metadata().target();
        if target.starts_with("agent_client_protocol") {
            self.0.sdk.fetch_add(1, Ordering::SeqCst);
            let mut payload = SyntheticPayload(false);
            event.record(&mut payload);
            if payload.0 {
                self.0.sdk_payload.fetch_add(1, Ordering::SeqCst);
            }
        }
        if target == "kiro-private-drop" {
            self.0.drop_payload.fetch_add(1, Ordering::SeqCst);
        }
        if target == "kiro-outside" {
            self.0.outside.fetch_add(1, Ordering::SeqCst);
        }
    }
}
struct SyntheticPayload(bool);
impl tracing::field::Visit for SyntheticPayload {
    fn record_debug(&mut self, _: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        // Fixture data only. Retain a boolean, never the formatted payload.
        self.0 |= format!("{value:?}").contains("synthetic-secret");
    }
}
struct TracedDrop(Arc<AtomicUsize>);
impl Drop for TracedDrop {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
        tracing::warn!(target: "kiro-private-drop", "synthetic-secret-drop");
    }
}
struct TracedTransport<T> {
    inner: Option<T>,
    _drop: TracedDrop,
}
impl<T: ConnectTo<Client>> ConnectTo<Client> for TracedTransport<T> {
    async fn connect_to(mut self, client: impl ConnectTo<Agent>) -> Result<(), SdkError> {
        self.inner.take().unwrap().connect_to(client).await
    }
}
fn private_agent(metrics: Arc<Metrics>, mode: usize) -> impl ConnectTo<Client> {
    let initialized = metrics.clone();
    let created = metrics.clone();
    let prompted = metrics.clone();
    let loaded = metrics.clone();
    Agent
        .builder()
        .on_receive_request(
            async move |_: InitializeRequest, responder, _| {
                initialized.initialize.fetch_add(1, Ordering::SeqCst);
                responder.respond(
                    InitializeResponse::new(ProtocolVersion::V1)
                        .agent_capabilities(AgentCapabilities::new().load_session(true)),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: LoadSessionRequest, responder, cx| {
                assert_eq!(request.session_id.0.as_ref(), "synthetic-secret-session");
                loaded.load_session.fetch_add(1, Ordering::SeqCst);
                cx.send_notification(SessionNotification::new(
                    request.session_id,
                    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                        TextContent::new("synthetic-secret-history"),
                    ))),
                ))?;
                loaded.started.notify_one();
                if mode == 1 {
                    responder.respond_with_internal_error("synthetic-secret-load-error")
                } else if mode == 2 {
                    cx.spawn(async move {
                        std::future::pending::<()>().await;
                        responder.respond(LoadSessionResponse::new())
                    })
                } else {
                    responder.respond(
                        LoadSessionResponse::new().meta(
                            json!({"private":"synthetic-secret-load-response"})
                                .as_object()
                                .unwrap()
                                .clone(),
                        ),
                    )
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_: NewSessionRequest, responder, _| {
                created.new_session.fetch_add(1, Ordering::SeqCst);
                responder.respond(NewSessionResponse::new("synthetic-secret-session"))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: PromptRequest, responder, cx| {
                assert_eq!(
                    request.prompt,
                    vec![ContentBlock::Text(TextContent::new(
                        "synthetic-secret-prompt"
                    ))]
                );
                prompted.prompt.fetch_add(1, Ordering::SeqCst);
                cx.send_notification(SessionNotification::new(
                    "synthetic-secret-session",
                    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                        TextContent::new("synthetic-secret-response"),
                    ))),
                ))?;
                prompted.started.notify_one();
                if mode == 0 {
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                } else if mode == 1 {
                    responder.respond_with_internal_error("synthetic-secret-error")
                } else {
                    cx.spawn(async move {
                        std::future::pending::<()>().await;
                        responder.respond(PromptResponse::new(StopReason::EndTurn))
                    })
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            async move |_: CancelNotification, _| {
                metrics.cancel.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
}

#[tokio::test]
async fn sdk_payload_tracing_is_private_for_success_error_cancel_and_future_drop() {
    use tracing::instrument::WithSubscriber;
    let positive = Arc::new(TraceCounts::default());
    let capture = tracing::Dispatch::new(CaptureSubscriber(positive.clone()));
    // Prove the capturer detects the real SDK's synthetic payload without the
    // consumer boundary. This control emits only into this in-memory subscriber.
    Client
        .builder()
        .on_receive_notification(
            async |_: SessionNotification, _| Ok(()),
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(private_agent(Arc::new(Metrics::default()), 0), async |cx| {
            cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;
            let created = cx
                .send_request(NewSessionRequest::new("/synthetic-secret-cwd"))
                .block_task()
                .await?;
            cx.send_request(PromptRequest::new(
                created.session_id,
                vec![ContentBlock::Text(TextContent::new(
                    "synthetic-secret-prompt",
                ))],
            ))
            .block_task()
            .await?;
            Ok(())
        })
        .with_subscriber(capture)
        .await
        .unwrap();
    assert!(positive.sdk.load(Ordering::SeqCst) > 0);
    assert!(positive.sdk_payload.load(Ordering::SeqCst) > 0);
    for load in [false, true] {
        for mode in 0..4 {
            let counts = Arc::new(TraceCounts::default());
            let capture = tracing::Dispatch::new(CaptureSubscriber(counts.clone()));
            let drops = Arc::new(AtomicUsize::new(0));
            let expected_drops = drops.clone();
            async {
                tracing::trace!(target: "kiro-outside", "outside-before");
                let context = original();
                let metrics = Arc::new(Metrics::default());
                let transport = TracedTransport {
                    inner: Some(private_agent(metrics.clone(), mode)),
                    _drop: TracedDrop(drops),
                };
                let session = NewSessionRequest::new("/synthetic-secret-cwd").meta(
                    json!({"kiro":{"modelId":"synthetic-secret-model"}})
                        .as_object()
                        .unwrap()
                        .clone(),
                );
                let session = if load {
                    KiroSessionStart::Load(LoadSessionRequest::new(
                        "synthetic-secret-session",
                        "/synthetic-secret-cwd",
                    ))
                } else {
                    KiroSessionStart::New(session)
                };
                if mode < 3 {
                    let mut sink = Sink::default();
                    let work = run_kiro(
                        &context,
                        transport,
                        KiroEngine::V3,
                        session,
                        KiroPrompt::new("synthetic-secret-prompt"),
                        limits(),
                        &mut sink,
                    );
                    let interrupt = async {
                        if mode == 2 {
                            metrics.started.notified().await;
                            context.cancellation().cancel();
                        }
                    };
                    let (result, _) = tokio::join!(work, interrupt);
                    if mode == 0 {
                        assert_eq!(result.unwrap().answer(), Some("synthetic-secret-response"));
                    } else if mode == 1 {
                        assert_eq!(result.unwrap_err().reason(), KiroReason::Sdk);
                    } else {
                        assert!(matches!(
                            result.unwrap_err().reason(),
                            KiroReason::Context(_)
                        ));
                    }
                } else {
                    let entered = Arc::new(Notify::new());
                    let callback_drops = Arc::new(AtomicUsize::new(0));
                    let mut sink = PendingSink {
                        entered: entered.clone(),
                        drops: callback_drops.clone(),
                        calls: 0,
                    };
                    let mut work = Box::pin(run_kiro(
                        &context,
                        transport,
                        KiroEngine::V3,
                        session,
                        KiroPrompt::new("synthetic-secret-prompt"),
                        limits(),
                        &mut sink,
                    ));
                    tokio::select! {
                        _ = entered.notified() => {},
                        _ = &mut work => panic!("SDK must be pending before explicit future drop"),
                    }
                    // Drop occurs under the caller's active capturing dispatcher.
                    // The consumer must install its private dispatcher for this Drop.
                    drop(work);
                    assert_eq!(callback_drops.load(Ordering::SeqCst), 1);
                }
                assert_eq!(metrics.initialize.load(Ordering::SeqCst), 1);
                assert_eq!(
                    metrics.new_session.load(Ordering::SeqCst),
                    usize::from(!load)
                );
                assert_eq!(
                    metrics.load_session.load(Ordering::SeqCst),
                    usize::from(load)
                );
                assert_eq!(
                    metrics.prompt.load(Ordering::SeqCst),
                    usize::from(!load || mode == 0)
                );
                tracing::debug!(target: "kiro-outside", "outside-after");
            }
            .with_subscriber(capture)
            .await;
            assert_eq!(expected_drops.load(Ordering::SeqCst), 1);
            assert_eq!(counts.sdk.load(Ordering::SeqCst), 0);
            assert_eq!(counts.sdk_payload.load(Ordering::SeqCst), 0);
            assert_eq!(counts.drop_payload.load(Ordering::SeqCst), 0);
            assert_eq!(counts.outside.load(Ordering::SeqCst), 2);
        }
    }
}

#[derive(Clone, Copy)]
enum LoadEnding {
    Response,
    Error,
    Pending,
    Close,
}
fn load_request() -> LoadSessionRequest {
    LoadSessionRequest::new("actual-session", "/fixture/project")
        .mcp_servers(mcp_fixture())
        .meta(
            json!({"kiro":{"modeId":"fixture-mode"},"literal":true})
                .as_object()
                .unwrap()
                .clone(),
        )
}
async fn load_agent(
    peer: Channel,
    mut history: Vec<UntypedMessage>,
    mut fresh: Vec<UntypedMessage>,
    ending: LoadEnding,
    capabilities: AgentCapabilities,
    metrics: Arc<Metrics>,
) -> Result<(), SdkError> {
    Agent
        .builder()
        .on_receive_request(
            async |_: InitializeRequest, responder, _| {
                metrics.initialize.fetch_add(1, Ordering::SeqCst);
                responder.respond(
                    InitializeResponse::new(ProtocolVersion::V1)
                        .agent_capabilities(capabilities.clone()),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async |_: NewSessionRequest, responder, _| {
                metrics.new_session.fetch_add(1, Ordering::SeqCst);
                responder.respond_with_internal_error("unexpected New")
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async |request: LoadSessionRequest, responder, cx| {
                assert_eq!(request.session_id.0.as_ref(), "actual-session");
                assert_eq!(request.cwd, load_request().cwd);
                assert_eq!(request.mcp_servers, load_request().mcp_servers);
                assert_eq!(request.meta, load_request().meta);
                if !request.additional_directories.is_empty() {
                    assert_eq!(
                        request.additional_directories,
                        vec![std::path::PathBuf::from("/fixture/second")]
                    );
                }
                metrics.load_session.fetch_add(1, Ordering::SeqCst);
                for event in history.drain(..) {
                    cx.send_notification(event)?;
                }
                metrics.started.notify_one();
                match ending {
                    LoadEnding::Response => responder.respond(
                        LoadSessionResponse::new().meta(
                            json!({"private":"synthetic-secret-load-response"})
                                .as_object()
                                .unwrap()
                                .clone(),
                        ),
                    ),
                    LoadEnding::Error => {
                        responder.respond_with_internal_error("synthetic-secret-load-error")
                    }
                    LoadEnding::Pending | LoadEnding::Close => cx.spawn(async move {
                        std::future::pending::<()>().await;
                        responder.respond(LoadSessionResponse::new())
                    }),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async |request: PromptRequest, responder, cx| {
                assert_eq!(request.session_id.0.as_ref(), "actual-session");
                metrics.prompt.fetch_add(1, Ordering::SeqCst);
                for event in fresh.drain(..) {
                    cx.send_notification(event)?;
                }
                responder.respond(PromptResponse::new(StopReason::EndTurn))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            async |request: CancelNotification, _| {
                assert_eq!(request.session_id.0.as_ref(), "actual-session");
                metrics.cancel.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(peer, async |cx| {
            if matches!(ending, LoadEnding::Close) {
                metrics.started.notified().await;
            } else {
                cx.incoming_closed().await;
            }
            Ok(())
        })
        .await
}

#[tokio::test]
async fn load_routes_history_by_phase_preserves_literal_request_and_native_response() {
    {
        let engine = KiroEngine::V3;
        let context = original();
        let (channel, peer) = Channel::duplex();
        let metrics = Arc::new(Metrics::default());
        let mut sink = Sink {
            original: Some(context.clone()),
            ..Sink::default()
        };
        let mut request = load_request();
        request
            .additional_directories
            .push("/fixture/second".into());
        let (_, mut foreign) = text("agent_message_chunk", "foreign").into_parts();
        foreign["sessionId"] = json!("foreign-session");
        let mut policy = limits();
        policy.response_text_bytes = 2;
        let (result, _) = tokio::join!(
            run_kiro(
                &context,
                channel,
                engine,
                KiroSessionStart::Load(request),
                KiroPrompt::new("prompt"),
                policy,
                &mut sink
            ),
            load_agent(
                peer,
                vec![
                    text("user_message_chunk", "history user"),
                    text(
                        "agent_message_chunk",
                        "history larger than response retention cap"
                    ),
                    UntypedMessage::new("session/update", foreign).unwrap()
                ],
                vec![text("agent_message_chunk", "ok")],
                LoadEnding::Response,
                AgentCapabilities::new()
                    .load_session(true)
                    .session_capabilities(
                    SessionCapabilities::new()
                        .additional_directories(SessionAdditionalDirectoriesCapabilities::new())
                ),
                metrics.clone()
            )
        );
        let receipt = result.unwrap();
        assert_eq!(receipt.answer(), Some("ok"));
        assert_eq!(receipt.candidate_text(), Some("ok"));
        assert_eq!(
            sink.phases,
            vec![
                KiroDeliveryPhase::Loading,
                KiroDeliveryPhase::Loading,
                KiroDeliveryPhase::Prompting
            ]
        );
        assert_eq!(receipt.accepted(), 3);
        assert_eq!(receipt.observed(), 4);
        assert_eq!(
            receipt.load_response().unwrap().meta.as_ref().unwrap()["private"],
            json!("synthetic-secret-load-response")
        );
        assert!(!format!("{receipt:?}").contains("synthetic-secret"));
        assert_eq!(metrics.new_session.load(Ordering::SeqCst), 0);
        assert_eq!(metrics.load_session.load(Ordering::SeqCst), 1);
        assert_eq!(metrics.prompt.load(Ordering::SeqCst), 1);
        assert_eq!(metrics.cancel.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn load_capabilities_gate_requests_and_id_limit_precedes_transport() {
    for case in 0..3 {
        let context = original();
        let (channel, peer) = Channel::duplex();
        let metrics = Arc::new(Metrics::default());
        let mut request = load_request();
        let caps = if case == 2 {
            request
                .additional_directories
                .push("/fixture/second".into());
            AgentCapabilities::new().load_session(true)
        } else if case == 1 {
            AgentCapabilities::new().load_session(false)
        } else {
            AgentCapabilities::default()
        };
        let mut sink = Sink::default();
        let (result, _) = tokio::join!(
            run_kiro(
                &context,
                channel,
                KiroEngine::V3,
                KiroSessionStart::Load(request),
                KiroPrompt::new("prompt"),
                limits(),
                &mut sink
            ),
            load_agent(
                peer,
                vec![],
                vec![],
                LoadEnding::Response,
                caps,
                metrics.clone()
            )
        );
        let error = result.unwrap_err();
        assert_eq!(
            error.reason(),
            if case == 2 {
                KiroReason::UnsupportedAdditionalDirectories
            } else {
                KiroReason::UnsupportedLoad
            }
        );
        assert!(error.receipt().session().is_none());
        assert!(error.receipt().cancel_enqueue().is_none());
        assert_eq!(metrics.initialize.load(Ordering::SeqCst), 1);
        for count in [
            &metrics.new_session,
            &metrics.load_session,
            &metrics.prompt,
            &metrics.cancel,
        ] {
            assert_eq!(count.load(Ordering::SeqCst), 0);
        }
    }
    for case in 0..3 {
        let context = if case == 1 {
            Arc::new(SnapshotContext::new(
                Instant::now(),
                Cancellation::default(),
            ))
        } else {
            original()
        };
        if case == 2 {
            context.cancellation().cancel();
        }
        let (channel, _peer) = Channel::duplex();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut policy = limits();
        if case == 0 {
            policy.id_bytes = 13;
        }
        let error = run_kiro(
            &context,
            CountedTransport {
                channel,
                calls: calls.clone(),
            },
            KiroEngine::V3,
            KiroSessionStart::Load(load_request()),
            KiroPrompt::new("prompt"),
            policy,
            &mut Sink::default(),
        )
        .await
        .unwrap_err();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(error.receipt().driver().is_none());
        if case == 0 {
            assert_eq!(error.reason(), KiroReason::IdLimit);
        } else {
            assert!(matches!(error.reason(), KiroReason::Context(_)));
        }
    }
}

#[tokio::test]
async fn load_history_pending_blocks_response_and_prompt_until_serial_sink_releases() {
    let context = original();
    let (channel, peer) = Channel::duplex();
    let metrics = Arc::new(Metrics::default());
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let mut sink = BarrierSink {
        entered: entered.clone(),
        release: release.clone(),
        accepted: 0,
    };
    let completed = AtomicUsize::new(0);
    let work = async {
        let result = run_kiro(
            &context,
            channel,
            KiroEngine::V3,
            KiroSessionStart::Load(load_request()),
            KiroPrompt::new("prompt"),
            limits(),
            &mut sink,
        )
        .await;
        completed.store(1, Ordering::SeqCst);
        result
    };
    let barrier = async {
        for stage in 0..2 {
            entered.notified().await;
            assert_eq!(completed.load(Ordering::SeqCst), 0);
            assert_eq!(metrics.prompt.load(Ordering::SeqCst), stage);
            assert_eq!(metrics.load_session.load(Ordering::SeqCst), 1);
            release.notify_one();
        }
    };
    let (result, _, _) = tokio::join!(
        work,
        load_agent(
            peer,
            vec![text("agent_message_chunk", "history")],
            vec![text("agent_message_chunk", "fresh")],
            LoadEnding::Response,
            AgentCapabilities::new().load_session(true),
            metrics.clone()
        ),
        barrier
    );
    assert_eq!(result.unwrap().answer(), Some("fresh"));
    assert_eq!(sink.accepted, 2);
}

#[tokio::test]
async fn load_pending_or_history_pending_original_interrupts_without_prompt_or_replay() {
    for pending_history in [false, true] {
        for cancel in [false, true] {
            let context = Arc::new(SnapshotContext::new(
                Instant::now() + Duration::from_millis(if cancel { 1000 } else { 100 }),
                Cancellation::default(),
            ));
            let (channel, peer) = Channel::duplex();
            let metrics = Arc::new(Metrics::default());
            let entered = Arc::new(Notify::new());
            let drops = Arc::new(AtomicUsize::new(0));
            let mut sink = PendingSink {
                entered: entered.clone(),
                drops: drops.clone(),
                calls: 0,
            };
            let work = run_kiro(
                &context,
                channel,
                KiroEngine::V3,
                KiroSessionStart::Load(load_request()),
                KiroPrompt::new("prompt"),
                limits(),
                &mut sink,
            );
            let interrupt = async {
                if pending_history {
                    entered.notified().await;
                } else {
                    metrics.started.notified().await;
                }
                if cancel {
                    context.cancellation().cancel();
                }
            };
            let (result, _, _) = tokio::join!(
                work,
                load_agent(
                    peer,
                    if pending_history {
                        vec![
                            text("agent_message_chunk", "pending-history"),
                            text("agent_message_chunk", "never-replay"),
                        ]
                    } else {
                        vec![]
                    },
                    vec![],
                    LoadEnding::Pending,
                    AgentCapabilities::new().load_session(true),
                    metrics.clone()
                ),
                interrupt
            );
            let error = result.unwrap_err();
            assert!(matches!(error.reason(), KiroReason::Context(_)));
            assert_eq!(sink.calls, usize::from(pending_history));
            assert_eq!(drops.load(Ordering::SeqCst), usize::from(pending_history));
            assert_eq!(error.receipt().accepted(), 0);
            assert!(error.receipt().candidate_text().is_none());
            assert!(error.receipt().load_response().is_none());
            assert!(error.receipt().response().is_none());
            assert_eq!(metrics.new_session.load(Ordering::SeqCst), 0);
            assert_eq!(metrics.prompt.load(Ordering::SeqCst), 0);
            assert_eq!(metrics.cancel.load(Ordering::SeqCst), 1);
            assert!(error.receipt().cancel_enqueue().unwrap().is_ok());
        }
    }
}

#[tokio::test]
async fn load_errors_limits_and_absent_fresh_text_preserve_real_receipts() {
    for case in 0..7 {
        let context = original();
        let (channel, peer) = Channel::duplex();
        let metrics = Arc::new(Metrics::default());
        let mut sink = Sink {
            error: case == 4,
            ..Sink::default()
        };
        let mut policy = limits();
        if case == 3 {
            policy.notifications = 1;
        }
        let history = if case == 2 {
            vec![UntypedMessage::new("session/update", json!({"sessionId":"actual-session","update":{"sessionUpdate":"agent_message_chunk"}})).unwrap()]
        } else {
            vec![
                text("agent_message_chunk", "history"),
                text("agent_message_chunk", "second"),
            ]
        };
        let (result, _) = tokio::join!(
            run_kiro(
                &context,
                channel,
                KiroEngine::V3,
                KiroSessionStart::Load(load_request()),
                KiroPrompt::new("prompt"),
                policy,
                &mut sink
            ),
            load_agent(
                peer,
                history,
                if case == 6 {
                    vec![text("agent_message_chunk", "")]
                } else {
                    vec![]
                },
                if case == 0 {
                    LoadEnding::Error
                } else if case == 1 {
                    LoadEnding::Close
                } else {
                    LoadEnding::Response
                },
                AgentCapabilities::new().load_session(true),
                metrics.clone()
            )
        );
        if case == 6 {
            assert_eq!(result.unwrap().answer(), Some(""));
            continue;
        }
        let error = result.unwrap_err();
        if case == 0 {
            assert_eq!(error.reason(), KiroReason::Sdk);
            assert!(error.sdk_error().is_some());
        }
        if case == 1 {
            assert!(matches!(
                error.reason(),
                KiroReason::IncomingClosed | KiroReason::Sdk
            ));
        }
        if case == 2 {
            assert_eq!(error.reason(), KiroReason::Wire);
            assert!(error.decode_error().is_some());
        }
        if case == 3 {
            assert_eq!(error.reason(), KiroReason::NotificationLimit);
        }
        if case == 4 {
            assert_eq!(error.reason(), KiroReason::Sink(io::ErrorKind::Other));
            assert!(error.io_error().is_some());
        }
        if case == 5 {
            assert_eq!(error.reason(), KiroReason::MissingText);
            assert!(error.receipt().load_response().is_some());
            assert!(error.receipt().response().is_some());
        }
        assert!(error.receipt().answer().is_none());
        assert_eq!(metrics.new_session.load(Ordering::SeqCst), 0);
        assert_eq!(
            metrics.prompt.load(Ordering::SeqCst),
            usize::from(case == 5)
        );
    }
}

#[tokio::test]
async fn observed_load_response_survives_original_cancel_during_fresh_sink() {
    let context = original();
    let (channel, peer) = Channel::duplex();
    let metrics = Arc::new(Metrics::default());
    let entered = Arc::new(Notify::new());
    let drops = Arc::new(AtomicUsize::new(0));
    let mut sink = PendingSink {
        entered: entered.clone(),
        drops: drops.clone(),
        calls: 0,
    };
    let interrupt = async {
        entered.notified().await;
        context.cancellation().cancel();
    };
    let (result, _, _) = tokio::join!(
        run_kiro(
            &context,
            channel,
            KiroEngine::V3,
            KiroSessionStart::Load(load_request()),
            KiroPrompt::new("prompt"),
            limits(),
            &mut sink
        ),
        load_agent(
            peer,
            vec![],
            vec![text("agent_message_chunk", "fresh")],
            LoadEnding::Response,
            AgentCapabilities::new().load_session(true),
            metrics.clone()
        ),
        interrupt
    );
    let error = result.unwrap_err();
    assert!(matches!(error.reason(), KiroReason::Context(_)));
    assert!(error.receipt().load_response().is_some());
    assert!(error.receipt().response().is_none());
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(error.receipt().accepted(), 0);
    assert_eq!(metrics.cancel.load(Ordering::SeqCst), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn load_serialization_failure_cannot_cancel_a_preexisting_session() {
    use std::os::unix::ffi::OsStringExt;
    let context = original();
    let (channel, peer) = Channel::duplex();
    let metrics = Arc::new(Metrics::default());
    let mut request = load_request();
    request.cwd = std::ffi::OsString::from_vec(vec![b'/', 0xff]).into();
    let mut sink = Sink::default();
    let (result, _) = tokio::join!(
        run_kiro(
            &context,
            channel,
            KiroEngine::V3,
            KiroSessionStart::Load(request),
            KiroPrompt::new("prompt"),
            limits(),
            &mut sink
        ),
        load_agent(
            peer,
            vec![],
            vec![],
            LoadEnding::Response,
            AgentCapabilities::new().load_session(true),
            metrics.clone()
        )
    );
    let error = result.unwrap_err();
    assert_eq!(error.reason(), KiroReason::Sdk);
    assert!(error.sdk_error().is_some());
    assert!(error.receipt().load_response().is_none());
    assert!(error.receipt().cancel_enqueue().is_none());
    assert_eq!(metrics.initialize.load(Ordering::SeqCst), 1);
    for count in [
        &metrics.load_session,
        &metrics.prompt,
        &metrics.new_session,
        &metrics.cancel,
    ] {
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn service_original_rejects_new_load_and_config_before_transport() {
    {
        let engine = KiroEngine::V3;
        for mode in 0..3 {
            let context = Arc::new(SnapshotContext::service(Cancellation::default()));
            let (channel, _peer) = Channel::duplex();
            let calls = Arc::new(AtomicUsize::new(0));
            let failure = run_kiro(
                &context,
                CountedTransport {
                    channel,
                    calls: Arc::clone(&calls),
                },
                engine,
                session_start(mode == 1),
                if mode == 2 {
                    configured_prompt()
                } else {
                    KiroPrompt::new("prompt")
                },
                limits(),
                &mut Sink::default(),
            )
            .await
            .unwrap_err();
            assert_eq!(
                failure.reason(),
                KiroReason::Context(io::ErrorKind::InvalidInput)
            );
            assert_eq!(
                failure.io_error().unwrap().kind(),
                io::ErrorKind::InvalidInput
            );
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            assert!(failure.receipt().driver().is_none());
            assert!(failure.receipt().answer().is_none());
            assert!(failure.receipt().session().is_none());
            assert!(failure.receipt().cancel_enqueue().is_none());
            assert_eq!(failure.receipt().observed(), 0);
        }
    }
}
