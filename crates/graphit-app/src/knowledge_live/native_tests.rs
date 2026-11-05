//! Live runner composition with a genuine ACP child and the existing Kiro transport.
use super::*;
use crate::knowledge_ask_service::AskAvailability;
use graphit_hub::native::{RenderedContent, ToolGrants};
use std::{ffi::OsString, path::Path};

const AGENT: &[u8] = b"---\nname: documentary-agent\ndescription: Documentary fixture\nread_only: true\nallows_shell: false\n---\nUse selected sources.\n";
const PROMPT: &str = r#"{"question":"Literal Live prompt","pages":[{"slug":"fixture-page","body":"Authorized fixture text"}]}"#;

struct NativeHarness {
    _directory: tempfile::TempDir,
    availability: AskAvailability,
    journal: std::path::PathBuf,
    agent: std::path::PathBuf,
}
impl NativeHarness {
    fn new(mode: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let binary = std::path::PathBuf::from(
            std::env::var_os("GRAPHIT_TEST_KIRO_KNOWLEDGE_FIXTURE")
                .expect("run the App test through cargo xtask cargo test to emit its Kiro fixture"),
        );
        assert!(
            binary.is_absolute() && binary.is_file(),
            "GRAPHIT_TEST_KIRO_KNOWLEDGE_FIXTURE must name the Cargo-emitted fixture executable"
        );
        std::fs::copy(binary, directory.path().join("kiro-cli")).unwrap();
        let journal = directory.path().join("observed.txt");
        let agent = directory.path().join("AGENT.md");
        std::fs::write(&agent, AGENT).unwrap();
        let environment = vec![
            (
                OsString::from("PATH"),
                directory.path().as_os_str().to_owned(),
            ),
            ("KIRO_CAPTURED".into(), "literal-captured".into()),
            ("KIRO_FIXTURE_MODEL".into(), "fixture-model".into()),
            ("KIRO_FIXTURE_MODE".into(), mode.into()),
            (
                "KIRO_FIXTURE_JOURNAL".into(),
                journal.as_os_str().to_owned(),
            ),
        ];
        let availability = AskAvailability::capture(
            graphit_ai::cli_provider::CliCompletionProvider::KiroV3,
            std::ffi::OsStr::new("kiro-cli"),
            environment,
            directory.path(),
            limits().native,
        )
        .unwrap();
        assert!(availability.service().is_some());
        Self {
            _directory: directory,
            availability,
            journal,
            agent,
        }
    }
    fn log(&self) -> String {
        match std::fs::read_to_string(&self.journal) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => panic!("native fixture observation: {error}"),
        }
    }
}
fn rendered_agent(original: &mut RequestContext<'_>) -> RenderedContent {
    let descriptor = original.parse_agent(AGENT).unwrap();
    original
        .render_agent(
            NativeBinding::KiroV3,
            &descriptor,
            None,
            &ToolGrants {
                binding: NativeBinding::KiroV3,
                read_only: true,
                allows_shell: false,
                tools: vec![],
            },
        )
        .unwrap()
}
fn prepare_agent(
    live: &LiveSessionService,
    access: &Access<'_>,
    id: &str,
    source: &Path,
    rendered: &RenderedContent,
) {
    let reader = captured_source(source, graphit_fs::NodeKind::File, access.original());
    let selected = request();
    live.prepare(
        access,
        id,
        Preparation {
            agent_source_entry: Path::new(""),
            agent: CapturedArtifact {
                selection: &selected.agent,
                reader: &reader,
            },
            rendered,
            artifacts: &[],
            allowed: &|| access.read().is_ok(),
        },
    )
    .unwrap();
}
async fn wait_for_text(live: &LiveSessionService, access: &Access<'_>, id: &str) {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let seen = live.observe(access, id, None).unwrap();
            if seen.replay.events.iter().any(|event| matches!(&event.data, EventData::Text { text } if text.contains("fresh fixture answer"))) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("incremental text must arrive before the gated ACP terminal");
}

#[tokio::test]
async fn native_new_load_replay_disconnect_busy_and_lost_ack_preserve_one_observed_turn() {
    let fixture = new_fixture();
    let native = NativeHarness::new("stream_gate");
    let cancel = Cancellation::default();
    let mut original =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let rendered = rendered_agent(&mut original);
    let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
    let live = LiveSessionService::new(limits());
    let created = live.create(&access, request()).unwrap();
    prepare_agent(&live, &access, &created.id, &native.agent, &rendered);
    let service = native.availability.service().unwrap();
    let send = SendRequest {
        turn_id: "first-native-turn".into(),
        text: PROMPT.into(),
    };
    let allowed = || access.read().is_ok();
    let run = live.run_turn(&access, &created.id, &send, service, &allowed, None);
    let observe = async {
        wait_for_text(&live, &access, &created.id).await;
        assert_eq!(
            live.observe(&access, &created.id, None)
                .unwrap()
                .replay
                .session
                .state,
            SessionState::Running
        );
        assert!(!native.log().lines().any(|line| line == "terminal"));
        assert_eq!(
            native.log().lines().filter(|line| *line == "new").count(),
            1
        );
        let duplicate = SendRequest {
            turn_id: "concurrent-turn".into(),
            text: PROMPT.into(),
        };
        assert!(matches!(
            live.run_turn(&access, &created.id, &duplicate, service, &allowed, None)
                .await,
            Err(LiveError::Busy)
        ));

        let (opened, _opening) = tokio::sync::oneshot::channel();
        let (events, receiver) = tokio::sync::mpsc::channel(1);
        drop(receiver);
        let disconnect = live
            .forward_events(
                &access,
                &created.id,
                None,
                TailDelivery { opened, events },
                &allowed,
            )
            .await;
        assert_eq!(disconnect.err().unwrap().kind(), io::ErrorKind::BrokenPipe);
        assert!(!cancel.is_cancelled());
        assert_eq!(
            native
                .log()
                .lines()
                .filter(|line| *line == "prompt")
                .count(),
            1
        );
        std::fs::write(
            native.journal.with_extension("finish"),
            b"release observed terminal",
        )
        .unwrap();
    };
    let (result, ()) = tokio::join!(run, observe);
    let result = result.unwrap();
    let terminal = observed_terminal(&fixture, &live, &created.id);
    assert_eq!(terminal.outcome, TurnOutcome::Completed);
    assert!(terminal.joined && terminal.clean && terminal.persisted);
    assert!(result.persistence_error.is_none());
    let ack = live
        .run_turn(&access, &created.id, &send, service, &allowed, None)
        .await
        .unwrap();
    assert!(ack.persistence_error.is_none());
    assert_eq!(observed_terminal(&fixture, &live, &created.id), terminal);
    assert_eq!(
        native
            .log()
            .lines()
            .filter(|line| *line == "prompt")
            .count(),
        1
    );

    let fresh_cancel = Cancellation::default();
    let fresh = RequestContext::new(
        content(),
        &fresh_cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let fresh_access = Access::service(&fixture.auth, &fixture.raw, &fresh).unwrap();
    prepare_agent(&live, &fresh_access, &created.id, &native.agent, &rendered);
    let continued = live
        .run_turn(
            &fresh_access,
            &created.id,
            &SendRequest {
                turn_id: "second-native-turn".into(),
                text: PROMPT.into(),
            },
            service,
            &|| fresh_access.read().is_ok(),
            None,
        )
        .await
        .unwrap();
    assert!(continued.persistence_error.is_none());
    let continued_terminal = observed_terminal(&fixture, &live, &created.id);
    assert_eq!(continued_terminal.outcome, TurnOutcome::Completed);
    assert!(continued_terminal.joined && continued_terminal.clean && continued_terminal.persisted);
    // Retrying an older turn after a newer terminal is an unchanged rejection.
    let before_retry = live
        .observe(&fresh_access, &created.id, None)
        .unwrap()
        .replay;
    let old_retry = live
        .run_turn(
            &fresh_access,
            &created.id,
            &send,
            service,
            &|| fresh_access.read().is_ok(),
            None,
        )
        .await
        .err()
        .expect("older turn must not execute again");
    assert_eq!(old_retry.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(old_retry.effect(), NativeDocumentJournalEffect::Unchanged);
    assert!(!fresh_cancel.is_cancelled());
    let after_retry = live.observe(&fresh_access, &created.id, None).unwrap();
    assert_eq!(after_retry.replay, before_retry);
    assert_eq!(after_retry.terminal.unwrap().turn_id, "second-native-turn");
    let log = native.log();
    assert_eq!(log.lines().filter(|line| *line == "new").count(), 1);
    assert_eq!(log.lines().filter(|line| *line == "load").count(), 1);
    assert_eq!(log.lines().filter(|line| *line == "prompt").count(), 2);
    assert!(!log.lines().any(|line| line == "delete"));
    let replay = live
        .observe(&fresh_access, &created.id, None)
        .unwrap()
        .replay;
    let public = serde_json::to_string(&replay).unwrap();
    assert!(!public.contains("private-kiro-fixture-session"));
    assert!(!public.contains("old history must not become the fresh answer"));
    assert_eq!(
        replay
            .events
            .iter()
            .filter(|event| matches!(event.data, EventData::Text { .. }))
            .count(),
        2
    );
    for turn in ["first-native-turn", "second-native-turn"] {
        assert_eq!(
            replay
                .events
                .iter()
                .filter(|event| event.turn_id.as_deref() == Some(turn)
                    && matches!(event.data, EventData::ToolUse { .. }))
                .count(),
            1
        );
    }
    live.close_with_effect(&fresh_access, &created.id).unwrap();
    live.delete_with_effect(&fresh_access, &created.id).unwrap();
    assert!(live.shutdown(Instant::now() + Duration::from_secs(3)).await);
}

#[tokio::test]
async fn native_cancel_deadline_and_frame_limit_retain_observed_outcome_until_fresh_admission() {
    for mode in ["cancel", "deadline", "large_frame"] {
        let fixture = new_fixture();
        let native = NativeHarness::new(if mode == "deadline" { "cancel" } else { mode });
        let cancel = Cancellation::default();
        let duration = if mode == "deadline" {
            Duration::from_secs(3)
        } else {
            Duration::from_secs(30)
        };
        let mut original =
            RequestContext::new(content(), &cancel, Instant::now() + duration).unwrap();
        let rendered = rendered_agent(&mut original);
        let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
        let live = LiveSessionService::new(limits());
        let created = live.create(&access, request()).unwrap();
        prepare_agent(&live, &access, &created.id, &native.agent, &rendered);
        let service = native.availability.service().unwrap();
        let send = SendRequest {
            turn_id: format!("native-{mode}"),
            text: PROMPT.into(),
        };
        let allowed = || access.read().is_ok();
        let run = live.run_turn(&access, &created.id, &send, service, &allowed, None);
        let interrupt = async {
            if mode == "cancel" || mode == "deadline" {
                wait_for_text(&live, &access, &created.id).await;
                if mode == "cancel" {
                    cancel.cancel();
                }
            }
        };
        let (result, ()) = tokio::join!(run, interrupt);
        let result = result.unwrap();
        let terminal = observed_terminal(&fixture, &live, &created.id);
        let log = native.log();
        for observed in ["new", "prompt", "authorized_render_checked"] {
            assert!(
                log.lines().any(|line| line == observed),
                "{mode}: missing actual child phase {observed}"
            );
        }
        assert!(terminal.joined && terminal.clean, "{mode}");
        assert_ne!(terminal.outcome, TurnOutcome::Completed, "{mode}");
        if mode == "cancel" {
            assert_eq!(terminal.outcome, TurnOutcome::Cancelled);
            assert_eq!(terminal.error, Some(FailureCode::Cancelled));
        }
        if mode == "deadline" {
            assert_eq!(terminal.error, Some(FailureCode::Deadline));
        }
        if mode == "large_frame" {
            // The existing SDK reports transport framing failure as Native/Sdk.
            // A different error before the fixture prompt cannot satisfy this oracle.
            assert_eq!(terminal.error, Some(FailureCode::Native));
        }
        if mode != "large_frame" {
            assert!(!terminal.persisted);
            assert!(result.persistence_error.is_some());
        }
        let fresh_cancel = Cancellation::default();
        let fresh = RequestContext::new(
            content(),
            &fresh_cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let fresh_access = Access::service(&fixture.auth, &fixture.raw, &fresh).unwrap();
        let guard = live.begin_preparation(&fresh_access, &created.id).unwrap();
        assert!(
            guard.recover_interrupted(&fresh_access).unwrap().is_none(),
            "{mode}: a recovered Ready terminal must permit a new turn"
        );
        assert_eq!(
            guard.recovered_effect(),
            if mode == "large_frame" {
                NativeDocumentJournalEffect::Unchanged
            } else {
                NativeDocumentJournalEffect::Applied
            }
        );
        if mode == "cancel" {
            let control_cancel = Cancellation::default();
            let control = RequestContext::new(
                content(),
                &control_cancel,
                Instant::now() + Duration::from_secs(30),
            )
            .unwrap();
            let control_access = Access::service(&fixture.auth, &fixture.raw, &control).unwrap();
            assert!(
                live.cancel_with_effect(&control_access, &created.id)
                    .unwrap()
                    .0
            );
            let error = LiveError::from(fresh.check().unwrap_err())
                .retain_prior_effect(guard.recovered_effect());
            assert_eq!(error.kind(), io::ErrorKind::Interrupted);
            assert_eq!(error.effect(), NativeDocumentJournalEffect::Applied);
        }
        drop(guard);
        let final_cancel = Cancellation::default();
        let final_original = RequestContext::new(
            content(),
            &final_cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let fresh_access = Access::service(&fixture.auth, &fixture.raw, &final_original).unwrap();
        let observed = live.observe(&fresh_access, &created.id, None).unwrap();
        assert_eq!(observed.replay.session.state, SessionState::Ready, "{mode}");
        assert!(observed.terminal.unwrap().persisted);
        assert_eq!(
            observed
                .replay
                .events
                .iter()
                .filter(|event| matches!(event.data, EventData::TurnDone { .. }))
                .count(),
            1
        );
        assert!(
            !serde_json::to_string(&observed.replay)
                .unwrap()
                .contains("private-kiro-fixture-session")
        );
        live.close_with_effect(&fresh_access, &created.id).unwrap();
        live.delete_with_effect(&fresh_access, &created.id).unwrap();
        assert!(live.shutdown(Instant::now() + Duration::from_secs(3)).await);
    }
}

async fn run_to_retained_child(
    fixture: &crate::source_server::test_fixture::Fixture,
    live: &LiveSessionService,
    access: &Access<'_>,
    native: &mut NativeHarness,
    rendered: &RenderedContent,
) -> (
    Session,
    Arc<crate::knowledge_kiro_process::KiroProcessOwner>,
) {
    let AskAvailability::Available(service) = &mut native.availability else {
        panic!("selected fixture CLI must resolve");
    };
    // Exercise the actual first-join timeout without a fake owner or terminal.
    service.policy.process.cleanup = Duration::ZERO;
    service.policy.process.termination_grace = Duration::from_millis(100);
    let created = live.create(access, request()).unwrap();
    prepare_agent(live, access, &created.id, &native.agent, rendered);
    let result = live
        .run_turn(
            access,
            &created.id,
            &SendRequest {
                turn_id: "bounded-join".into(),
                text: PROMPT.into(),
            },
            native.availability.service().unwrap(),
            &|| access.read().is_ok(),
            None,
        )
        .await
        .unwrap();
    assert!(result.persistence_error.is_none());
    let terminal = observed_terminal(fixture, live, &created.id);
    assert_eq!(terminal.error, Some(FailureCode::CleanupIncomplete));
    assert!(!terminal.joined && !terminal.clean);
    assert_eq!(
        live.observe(access, &created.id, None)
            .unwrap()
            .replay
            .session
            .state,
        SessionState::Failed
    );
    let owner = {
        let slot = live.slot(access, &created.id).unwrap();
        let state = slot.lock().unwrap();
        assert!(state.active.is_some());
        assert!(!state.abandoned);
        Arc::clone(state.process.as_ref().unwrap())
    };
    assert!(owner.spawned());
    (created, owner)
}

pub(crate) async fn retained_native_child_for_shutdown(
    live: &LiveSessionService,
    fixture: &crate::source_server::test_fixture::Fixture,
) -> (
    Arc<crate::knowledge_kiro_process::KiroProcessOwner>,
    AskAvailability,
    tempfile::TempDir,
) {
    let mut native = NativeHarness::new("delay_exit");
    let cancel = Cancellation::default();
    let mut original =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let rendered = rendered_agent(&mut original);
    let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
    let (_, owner) = run_to_retained_child(fixture, live, &access, &mut native, &rendered).await;
    (owner, native.availability, native._directory)
}

#[tokio::test]
async fn shutdown_reobserves_the_real_retained_child_after_the_turn_join_limit() {
    let fixture = new_fixture();
    let mut native = NativeHarness::new("delay_exit");
    let cancel = Cancellation::default();
    let mut original =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let rendered = rendered_agent(&mut original);
    let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
    let live = LiveSessionService::new(limits());
    let (created, owner) =
        run_to_retained_child(&fixture, &live, &access, &mut native, &rendered).await;
    let cold = LiveSessionService::new(limits());
    assert_eq!(
        recover_with_guard(&cold, &access, &created.id)
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(
        cold.close_with_effect(&access, &created.id)
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(
        cold.delete_with_effect(&access, &created.id)
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    let cold_reader = captured_source(&native.agent, graphit_fs::NodeKind::File, &original);
    assert_eq!(
        cold.prepare(
            &access,
            &created.id,
            Preparation {
                agent: CapturedArtifact {
                    selection: &created.agent,
                    reader: &cold_reader
                },
                agent_source_entry: Path::new(""),
                rendered: &rendered,
                artifacts: &[],
                allowed: &|| access.read().is_ok(),
            }
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::WouldBlock
    );
    let retained_workspace = live
        .slot(&access, &created.id)
        .unwrap()
        .lock()
        .unwrap()
        .workspace
        .as_ref()
        .unwrap()
        .lock()
        .unwrap()
        .path()
        .to_owned();
    assert!(retained_workspace.is_dir());
    // The run future is finished. Shutdown must wait for its genuine owner rather
    // than treating the retained cancellation flag as a future that still runs.
    assert!(live.shutdown(Instant::now() + Duration::from_secs(3)).await);
    assert!(owner.observe(Instant::now() + Duration::from_secs(1)).await);
    assert!(owner.clean_joined().await);
    assert!(
        native
            .availability
            .service()
            .unwrap()
            .shutdown()
            .await
            .is_ok()
    );
}

#[path = "queued_cancel_test.rs"]
mod queued_cancel;
