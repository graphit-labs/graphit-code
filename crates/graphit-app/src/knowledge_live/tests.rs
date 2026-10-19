use super::*;
use graphit_auth::Permission;
use graphit_core::{
    brand::Brand,
    file_lock::Cancellation,
    module_settings::Module,
    native_document_file::{
        NativeDocumentFileLimits, NativeDocumentJournalEffect, NativeDocumentJournalWriteBounds,
        NativeDocumentNamespace,
    },
};
use graphit_hub::{NativeBinding, native::ContentLimits, policy::RequestContext};
use std::{
    io,
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, Instant},
};

fn content() -> ContentLimits {
    ContentLimits {
        max_input_bytes: 1024 * 1024,
        max_output_bytes: 1024 * 1024,
        max_depth: 64,
        max_nodes: 65536,
        max_scalar_bytes: 65536,
        max_collection_entries: 4096,
    }
}
fn limits() -> HistoryLimits {
    HistoryLimits {
        native: NativeDocumentFileLimits::new(2 * 1024 * 1024, 4096, 128, 8192).unwrap(),
        envelope_bytes: NonZeroUsize::new(1024 * 1024).unwrap(),
        id_bytes: NonZeroUsize::new(4096).unwrap(),
        prompt_bytes: NonZeroUsize::new(65536).unwrap(),
        event_bytes: NonZeroUsize::new(65536).unwrap(),
    }
}
fn request() -> CreateRequest {
    CreateRequest {
        request_id: "history-request".into(),
        project: ProjectSelection {
            project_id: "selected-project".into(),
            project_version: "main".into(),
        },
        binding: NativeBinding::KiroV3,
        agent: ArtifactSelection {
            source: "hub".into(),
            project_id: None,
            instance_id: None,
            project_kind: None,
            artifact_type: "agent".into(),
            id: "documentary-agent".into(),
            version: "1.0.0".into(),
        },
        artifacts: vec![],
        title: Some("Documentary investigation".into()),
        prompt: None,
        model: "fixture-model".into(),
    }
}
fn new_fixture() -> crate::source_server::test_fixture::Fixture {
    crate::source_server::test_fixture::fixture(
        &[
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ],
        Brand::default(),
        NativeBinding::KiroV3,
    )
}

fn observed_terminal(
    fixture: &crate::source_server::test_fixture::Fixture,
    live: &LiveSessionService,
    id: &str,
) -> KnownTerminal {
    let cancellation = Cancellation::default();
    let original = RequestContext::new(
        content(),
        &cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
    live.observe(&access, id, None).unwrap().terminal.unwrap()
}

fn recover_with_guard(
    live: &LiveSessionService,
    access: &Access<'_>,
    id: &str,
) -> Result<Session, LiveError> {
    let guard = live.begin_preparation(access, id)?;
    match guard.recover_interrupted(access)? {
        Some(session) => Ok(session),
        None => Ok(live.observe(access, id, None)?.replay.session),
    }
}

/// History-only fixtures declare selected identities; they prove neither SDK reads nor grants.
pub(super) fn prepared_history_fixture(stored: &mut journal::StoredSession) {
    if stored.read_contexts.is_some() {
        return;
    }
    stored.prepared_fingerprint = Some("history-boundary-fixture".into());
    stored.read_contexts = Some(ReadContexts {
        selected: stored
            .session
            .artifacts
            .iter()
            .filter(|selection| matches!(selection.artifact_type.as_str(), "knowledge" | "ast"))
            .map(|selection| ReadContext {
                selection: selection.clone(),
            })
            .collect(),
    });
}

#[tokio::test]
async fn history_real_auth_owner_idempotency_replay_fence_and_closed_are_consistent() {
    let fixture = new_fixture();
    let cancel = Cancellation::default();
    let original =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
    let live = LiveSessionService::new(limits());
    let created = live.create(&access, request()).unwrap();
    assert_eq!(created.state, SessionState::Preparing);
    assert!(!created.created_at.is_empty());
    assert_eq!(live.create(&access, request()).unwrap(), created);
    let mut conflicting = request();
    conflicting.prompt = Some("different original request".into());
    assert_eq!(
        live.create(&access, conflicting).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(live.list(&access).unwrap().len(), 1);
    let slots = live.slots.lock().unwrap().len();
    for id in ["absent-a", "absent-b", "absent-c"] {
        assert_eq!(
            live.observe(&access, id, None).err().unwrap().kind(),
            io::ErrorKind::NotFound
        );
    }
    assert_eq!(live.slots.lock().unwrap().len(), slots);

    let mut observed = live.observe(&access, &created.id, None).unwrap();
    assert_eq!(observed.replay.events.len(), 1);
    live.emit(
        &access,
        &created.id,
        None,
        EventData::Prep {
            message: "Captured real sources".into(),
            artifact: None,
        },
    )
    .unwrap();
    assert!(observed.changed.has_changed().unwrap());
    observed.changed.changed().await.unwrap();
    let next = live
        .observe(&access, &created.id, Some(&created.cursor))
        .unwrap();
    assert_eq!(next.replay.events.len(), 1);
    assert_eq!(next.replay.events[0].sequence, 2);
    let mut future = next.replay.session.cursor.clone();
    future.sequence = u64::MAX;
    assert!(
        live.observe(&access, &created.id, Some(&future))
            .unwrap()
            .replay
            .events
            .is_empty()
    );
    future.generation = "different-log".into();
    assert_eq!(
        live.observe(&access, &created.id, Some(&future))
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(!live.cancel_with_effect(&access, &created.id).unwrap().0);
    let other = new_fixture();
    let other_access = Access::service(&other.auth, &other.raw, &original).unwrap();
    assert!(live.list(&other_access).unwrap().is_empty());
    assert_eq!(
        live.observe(&other_access, &created.id, None)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::NotFound
    );
    let closed = live.close_with_effect(&access, &created.id).unwrap().0;
    assert_eq!(closed.state, SessionState::Closed);
    assert!(
        live.emit(
            &access,
            &created.id,
            None,
            EventData::State {
                state: SessionState::Ready
            }
        )
        .is_err()
    );
    live.delete_with_effect(&access, &created.id).unwrap();
    assert!(live.list(&access).unwrap().is_empty());
}

#[tokio::test]
async fn history_fresh_admission_does_not_restore_cancelled_original_or_hide_corruption_and_limits()
{
    let fixture = new_fixture();
    let cancel = Cancellation::default();
    let original =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
    let live = LiveSessionService::new(limits());
    let created = live.create(&access, request()).unwrap();
    let journal = live.journal(&access).unwrap();
    let fresh_cancel = Cancellation::default();
    let fresh =
        RequestContext::new(content(), &fresh_cancel, original.deadline().unwrap()).unwrap();
    assert_eq!(
        journal
            .list(fresh.snapshot(), || access.read())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    cancel.cancel();
    assert_eq!(
        live.list(&access).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    let fresh_access = Access::service(&fixture.auth, &fixture.raw, &fresh).unwrap();
    assert_eq!(live.list(&fresh_access).unwrap()[0].id, created.id);
    let mut small = limits();
    small.envelope_bytes = NonZeroUsize::new(128).unwrap();
    let bounded = LiveSessionService::new(small);
    let failure = bounded.list(&fresh_access).unwrap_err();
    assert_eq!(failure.kind(), io::ErrorKind::FileTooLarge);
    assert_eq!(failure.effect(), NativeDocumentJournalEffect::Unchanged);
    assert_eq!(live.list(&fresh_access).unwrap().len(), 1);
    let namespace = NativeDocumentNamespace::capture(
        fresh_access.scope(),
        &fresh_access.scope().owner_root().join("knowledge-live"),
        limits().native,
        fresh.snapshot(),
    )
    .unwrap();
    let guard = namespace.lock(fresh.snapshot()).unwrap();
    let previous = guard.read_journal(limits().envelope_bytes.get()).unwrap();
    let invalid =
        serde_json::value::RawValue::from_string("{\"version\":2,\"sessions\":{}}".into()).unwrap();
    guard
        .replace_journal(
            &previous,
            &invalid,
            NativeDocumentJournalWriteBounds::new(1024 * 1024, 1024 * 1024, 1024 * 1024).unwrap(),
        )
        .unwrap();
    drop(guard);
    assert_eq!(
        live.list(&fresh_access).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    let readonly = crate::source_server::test_fixture::fixture(
        &[Permission::Read(Module::Knowledge)],
        Brand::default(),
        NativeBinding::KiroV3,
    );
    let readonly_access = Access::service(&readonly.auth, &readonly.raw, &fresh).unwrap();
    assert_eq!(
        LiveSessionService::new(limits())
            .create(&readonly_access, request())
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
}

#[tokio::test]
async fn mutable_workspace_keeps_birth_custody_and_preserves_a_replaced_root() {
    let fixture = new_fixture();
    let cancel = Cancellation::default();
    let original =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
    let mut workspace = workspace::Workspace::create(&access, limits().native).unwrap();
    workspace
        .write(
            std::path::Path::new("context/body.md"),
            b"original",
            original.snapshot(),
        )
        .unwrap();
    std::fs::write(workspace.path().join("context/body.md"), b"native edit").unwrap();
    std::fs::write(
        workspace.path().join("native-created"),
        b"created during the run",
    )
    .unwrap();
    let record = workspace
        .record(limits().native, original.snapshot())
        .unwrap();
    let root = workspace.path().to_owned();
    drop(workspace);
    let mut restored = workspace::Workspace::restore(&access, record, limits().native).unwrap();
    assert_eq!(
        std::fs::read(root.join("context/body.md")).unwrap(),
        b"native edit"
    );
    restored.cleanup(original.snapshot()).unwrap();
    assert!(!root.exists());
    let mut workspace = workspace::Workspace::create(&access, limits().native).unwrap();
    let root = workspace.path().to_owned();
    let retained = root.with_extension("retained");
    std::fs::rename(&root, &retained).unwrap();
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("foreign"), b"preserve").unwrap();
    assert_eq!(
        workspace.cleanup(original.snapshot()).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(std::fs::read(root.join("foreign")).unwrap(), b"preserve");
    assert!(retained.is_dir());
    drop(workspace);
    assert_eq!(std::fs::read(root.join("foreign")).unwrap(), b"preserve");
}

#[tokio::test]
async fn typed_acp_projection_redacts_split_private_ids_and_records_empty_tool_results_per_turn() {
    use agent_client_protocol::schema::v1::{
        ContentBlock, ContentChunk, SessionNotification, SessionUpdate, TextContent, ToolCall,
        ToolCallStatus,
    };
    use graphit_ai::cli_kiro::{KiroDeliveryPhase, KiroSink};
    let fixture = new_fixture();
    let cancel = Cancellation::default();
    let original =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
    let live = LiveSessionService::new(limits());
    live.create(&access, request()).unwrap();
    let allowed = || access.read().is_ok();
    for turn in ["turn-a", "turn-b"] {
        live.emit_fixture(
            &access,
            "history-request",
            Some(turn),
            EventData::State {
                state: SessionState::Running,
            },
        )
        .unwrap();
        live.emit(
            &access,
            "history-request",
            Some(turn),
            EventData::Prompt {
                text: "Investigate selected context".into(),
            },
        )
        .unwrap();
        let mut sink = sink::LiveSink::new(&live, &access, "history-request", turn, &allowed, &[]);
        for text in ["visible private-ses", "sion-fixture end"] {
            let event = SessionNotification::new(
                "private-session-fixture",
                SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                    TextContent::new(text),
                ))),
            );
            sink.push(KiroDeliveryPhase::Prompting, &event, original.snapshot())
                .await
                .unwrap();
        }
        let mut tool = ToolCall::new("same-native-tool-id", "Reading selected file");
        tool.name = Some("read_file".into());
        tool.status = ToolCallStatus::Completed;
        tool.raw_output = Some(serde_json::json!(""));
        sink.push(
            KiroDeliveryPhase::Prompting,
            &SessionNotification::new("private-session-fixture", SessionUpdate::ToolCall(tool)),
            original.snapshot(),
        )
        .await
        .unwrap();
        sink.finish().unwrap();
        live.emit(
            &access,
            "history-request",
            Some(turn),
            EventData::TurnDone {
                outcome: TurnOutcome::Completed,
                joined: true,
                clean: true,
            },
        )
        .unwrap();
        live.emit_fixture(
            &access,
            "history-request",
            Some(turn),
            EventData::State {
                state: SessionState::Ready,
            },
        )
        .unwrap();
    }
    let replay = live
        .observe(&access, "history-request", None)
        .unwrap()
        .replay;
    let serialized = serde_json::to_string(&replay).unwrap();
    assert!(!serialized.contains("private-session-fixture"));
    for turn in ["turn-a", "turn-b"] {
        let events: Vec<_> = replay
            .events
            .iter()
            .filter(|e| e.turn_id.as_deref() == Some(turn))
            .collect();
        let text: String = events
            .iter()
            .filter_map(|e| {
                if let EventData::Text { text } = &e.data {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(text, "visible [redacted] end");
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(&e.data, EventData::ToolResult { output, .. } if output == ""))
                .count(),
            1
        );
    }
    assert!(
        replay
            .events
            .windows(2)
            .all(|pair| pair[0].sequence + 1 == pair[1].sequence)
    );
    let _request_owner = Arc::clone(original.snapshot());
}

#[tokio::test]
async fn close_preserves_workspace_and_remove_reopens_owned_identity_without_vendor_clear() {
    let fixture = new_fixture();
    let cancellation = Cancellation::default();
    let original = RequestContext::new(
        content(),
        &cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
    let live = LiveSessionService::new(limits());
    let created = live.create(&access, request()).unwrap();
    let mut workspace = workspace::Workspace::create(&access, limits().native).unwrap();
    workspace
        .write(
            std::path::Path::new("native-edit"),
            b"retained on Close",
            original.snapshot(),
        )
        .unwrap();
    let directory = workspace.path().to_owned();
    live.change(&access, &created.id, |stored| {
        stored.workspace = Some(
            workspace
                .record(limits().native, original.snapshot())
                .unwrap(),
        );
        prepared_history_fixture(stored);
        service::append(
            stored,
            None,
            EventData::State {
                state: SessionState::Ready,
            },
        )?;
        Ok(())
    })
    .unwrap();
    drop(workspace);
    live.close_with_effect(&access, &created.id).unwrap();
    assert_eq!(
        std::fs::read(directory.join("native-edit")).unwrap(),
        b"retained on Close"
    );
    assert_eq!(
        live.observe(&access, &created.id, None)
            .unwrap()
            .replay
            .session
            .state,
        SessionState::Closed
    );
    let reopened = LiveSessionService::new(limits());
    reopened.delete_with_effect(&access, &created.id).unwrap();
    assert!(!directory.exists());
    assert!(reopened.list(&access).unwrap().is_empty());
}

#[tokio::test]
async fn replay_tail_fence_bounded_queue_and_unpersisted_terminal_are_independent_of_execution() {
    let fixture = new_fixture();
    let cancellation = Cancellation::default();
    let original = RequestContext::new(
        content(),
        &cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
    let live = LiveSessionService::new(limits());
    let created = live.create(&access, request()).unwrap();
    live.emit(
        &access,
        &created.id,
        None,
        EventData::Prep {
            message: "selected source".into(),
            artifact: None,
        },
    )
    .unwrap();
    let (opened, opening) = tokio::sync::oneshot::channel();
    let (events, mut progress) = tokio::sync::mpsc::channel(1);
    let subscriber = live.forward_events(
        &access,
        &created.id,
        None,
        TailDelivery { opened, events },
        &|| true,
    );
    let consume = async {
        let opening = opening.await.unwrap();
        assert_eq!(opening.after.sequence, 0);
        // The subscriber queue can be full while the independent journal still advances.
        live.emit_fixture(
            &access,
            &created.id,
            None,
            EventData::State {
                state: SessionState::Ready,
            },
        )
        .unwrap();
        let known = KnownTerminal {
            turn_id: "observed-turn".into(),
            outcome: TurnOutcome::Cancelled,
            error: Some(FailureCode::Cancelled),
            joined: true,
            clean: true,
            persisted: false,
        };
        {
            let slot = live.slot(&access, &created.id).unwrap();
            let mut state = slot.lock().unwrap();
            state.terminal = Some(known.clone());
            state.changed.send_replace(());
        }
        let mut sequences = Vec::new();
        let mut terminal = None;
        while let Some(message) = progress.recv().await {
            match message {
                TailMessage::Event(event) => sequences.push(event.sequence),
                TailMessage::Terminal(value) => {
                    terminal = Some(value);
                    break;
                }
            }
        }
        assert_eq!(sequences, vec![1, 2, 3]);
        assert_eq!(terminal, Some(known));
        drop(progress);
    };
    let (result, ()) = tokio::join!(subscriber, consume);
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::BrokenPipe);
    assert!(!cancellation.is_cancelled());
}

#[test]
fn wire_sequences_roundtrip_entire_u64_domain_and_reject_noncanonical_input() {
    for sequence in [0, 1, 9007199254740993, u64::MAX] {
        let cursor = ReplayCursor {
            generation: "abcdef".into(),
            sequence,
        };
        let encoded = serde_json::to_value(&cursor).unwrap();
        assert_eq!(encoded["sequence"], sequence.to_string());
        assert_eq!(
            serde_json::from_value::<ReplayCursor>(encoded).unwrap(),
            cursor
        );
    }
    for sequence in [
        serde_json::json!(1),
        serde_json::json!("01"),
        serde_json::json!("-1"),
        serde_json::json!("18446744073709551616"),
    ] {
        assert!(
            serde_json::from_value::<ReplayCursor>(
                serde_json::json!({"generation":"abcdef","sequence":sequence})
            )
            .is_err()
        );
    }
}

#[tokio::test]
async fn history_delivery_and_partial_remove_preserve_observed_effect_and_foreign_root() {
    let fixture = new_fixture();
    let cancellation = Cancellation::default();
    let original = RequestContext::new(
        content(),
        &cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
    let live = LiveSessionService::new(limits());
    let created = live.create(&access, request()).unwrap();
    let receipt = dispatch(
        &live,
        &access,
        HistoryOperation::Close {
            id: created.id.clone(),
        },
    )
    .unwrap();
    assert_eq!(receipt.effect, NativeDocumentJournalEffect::Applied);
    cancellation.cancel();
    let failure = receipt.recheck(&access).unwrap_err();
    assert_eq!(failure.kind(), io::ErrorKind::Interrupted);
    assert_eq!(failure.effect(), NativeDocumentJournalEffect::Applied);
    let fresh_cancel = Cancellation::default();
    let fresh = RequestContext::new(
        content(),
        &fresh_cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let fresh_access = Access::service(&fixture.auth, &fixture.raw, &fresh).unwrap();
    assert_eq!(
        live.observe(&fresh_access, &created.id, None)
            .unwrap()
            .replay
            .session
            .state,
        SessionState::Closed
    );
    let noop = dispatch(
        &live,
        &fresh_access,
        HistoryOperation::Close { id: created.id },
    )
    .unwrap();
    assert_eq!(noop.effect, NativeDocumentJournalEffect::Unchanged);
    let mut selected = request();
    selected.request_id = "foreign-remove".into();
    let created = live.create(&fresh_access, selected).unwrap();
    let workspace = workspace::Workspace::create(&fresh_access, limits().native).unwrap();
    let directory = workspace.path().to_owned();
    live.change(&fresh_access, &created.id, |stored| {
        stored.workspace = Some(workspace.record(limits().native, fresh.snapshot()).unwrap());
        prepared_history_fixture(stored);
        service::append(
            stored,
            None,
            EventData::State {
                state: SessionState::Ready,
            },
        )?;
        Ok(())
    })
    .unwrap();
    drop(workspace);
    let retained = directory.with_extension("retained");
    std::fs::rename(&directory, &retained).unwrap();
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(directory.join("foreign"), b"keep foreign root").unwrap();
    let failure = live
        .delete_with_effect(&fresh_access, &created.id)
        .unwrap_err();
    assert_eq!(failure.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(failure.effect(), NativeDocumentJournalEffect::Applied);
    assert_eq!(
        live.observe(&fresh_access, &created.id, None)
            .unwrap()
            .replay
            .session
            .state,
        SessionState::Closed
    );
    assert_eq!(
        std::fs::read(directory.join("foreign")).unwrap(),
        b"keep foreign root"
    );
    assert!(retained.is_dir());
}

#[tokio::test]
async fn fresh_mutation_reconciles_known_cancelled_turn_without_reviving_original_or_losing_workspace()
 {
    let fixture = new_fixture();
    let cancellation = Cancellation::default();
    let original = RequestContext::new(
        content(),
        &cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
    let live = LiveSessionService::new(limits());
    let created = live.create(&access, request()).unwrap();
    let mut workspace = workspace::Workspace::create(&access, limits().native).unwrap();
    workspace
        .write(
            std::path::Path::new("native-file"),
            b"keep for next admitted turn",
            original.snapshot(),
        )
        .unwrap();
    let directory = workspace.path().to_owned();
    live.change(&access, &created.id, |stored| {
        stored.workspace = Some(
            workspace
                .record(limits().native, original.snapshot())
                .unwrap(),
        );
        prepared_history_fixture(stored);
        service::append(
            stored,
            None,
            EventData::State {
                state: SessionState::Ready,
            },
        )?;
        service::append(
            stored,
            Some("cancelled-turn"),
            EventData::State {
                state: SessionState::Running,
            },
        )?;
        service::append(
            stored,
            Some("cancelled-turn"),
            EventData::Prompt {
                text: "original prompt".into(),
            },
        )?;
        Ok(())
    })
    .unwrap();
    drop(workspace);
    // This fixture supplies the typed result at the post-join seam; child execution is proved separately.
    {
        let slot = live.slot(&access, &created.id).unwrap();
        slot.lock().unwrap().terminal = Some(KnownTerminal {
            turn_id: "cancelled-turn".into(),
            outcome: TurnOutcome::Cancelled,
            error: Some(FailureCode::Cancelled),
            joined: true,
            clean: true,
            persisted: false,
        });
    }
    cancellation.cancel();
    assert_eq!(
        live.persist_terminal(&access, &created.id)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    let fresh_cancel = Cancellation::default();
    let fresh = RequestContext::new(
        content(),
        &fresh_cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let fresh_access = Access::service(&fixture.auth, &fixture.raw, &fresh).unwrap();
    let (requested, effect) = live.cancel_with_effect(&fresh_access, &created.id).unwrap();
    assert!(!requested);
    assert_eq!(effect, NativeDocumentJournalEffect::Applied);
    let observed = live.observe(&fresh_access, &created.id, None).unwrap();
    assert_eq!(observed.replay.session.state, SessionState::Ready);
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
    assert_eq!(
        std::fs::read(directory.join("native-file")).unwrap(),
        b"keep for next admitted turn"
    );
    let (_, effect) = live.cancel_with_effect(&fresh_access, &created.id).unwrap();
    assert_eq!(effect, NativeDocumentJournalEffect::Unchanged);
    assert_eq!(
        access.read().unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    let old_generation = live
        .observe(&fresh_access, &created.id, None)
        .unwrap()
        .replay
        .session
        .cursor
        .generation;
    live.close_with_effect(&fresh_access, &created.id).unwrap();
    live.delete_with_effect(&fresh_access, &created.id).unwrap();
    assert!(!directory.exists());
    let replacement = live.create(&fresh_access, request()).unwrap();
    assert_ne!(replacement.cursor.generation, old_generation);
    let observed = live.observe(&fresh_access, &replacement.id, None).unwrap();
    assert!(observed.terminal.is_none());
    assert_eq!(observed.replay.events.len(), 1);
    assert_eq!(observed.replay.session.state, SessionState::Preparing);
}

fn captured_source(
    path: &std::path::Path,
    kind: graphit_fs::NodeKind,
    original: &RequestContext<'_>,
) -> graphit_core::artifact_store::CapturedSourceReader {
    use graphit_core::artifact_store::{
        CapturedSourceReader, CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet,
        SnapshotPolicy, SourceReadLimits,
    };
    let retained = RetainedBytesLimits::new(1024 * 1024, 1024, 1024 * 1024).unwrap();
    let selected = RetainedFileSet::new(Vec::<std::path::PathBuf>::new(), retained).unwrap();
    let policy = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        CopyLimits::new(1024 * 1024, 1024, 1024 * 1024, 64).unwrap(),
        selected,
        retained,
    )
    .unwrap();
    CapturedSourceReader::capture(
        path,
        kind,
        &policy,
        SourceReadLimits::new(65536, 65536, 1024 * 1024).unwrap(),
        Arc::clone(original.snapshot()),
    )
    .unwrap()
}

#[tokio::test]
async fn preparation_renders_native_skill_assets_and_inline_rules_without_copying_contexts() {
    use graphit_hub::native::ToolGrants;
    use std::path::Path;
    let fixture = new_fixture();
    let sources = tempfile::tempdir().unwrap();
    let agent_bytes=b"---\nname: documentary-agent\ndescription: Documentary fixture\nread_only: true\nallows_shell: false\n---\nUse only selected sources and [principal asset](assets/primary.bin).\n";
    let rule_bytes = b"SELECTED-RULE-LITERAL\n";
    let agent_root = sources.path().join("agent");
    std::fs::create_dir_all(agent_root.join("assets")).unwrap();
    let primary_asset = b"principal asset\0\xff";
    std::fs::write(agent_root.join("assets/primary.bin"), primary_asset).unwrap();
    let agent_path = agent_root.join("AGENT.md");
    std::fs::write(&agent_path, agent_bytes).unwrap();
    let skill_root = sources.path().join("skill");
    std::fs::create_dir_all(skill_root.join("assets")).unwrap();
    let skill_bytes=b"---\nname: selected-skill\ndescription: Native skill fixture\n---\nUse [fixture asset](assets/literal.bin).\n";
    let asset_bytes: Vec<u8> = (0_u8..=255).cycle().take(2 * 64 * 1024 + 17).collect();
    std::fs::write(skill_root.join("SKILL.md"), skill_bytes).unwrap();
    std::fs::write(skill_root.join("assets/literal.bin"), &asset_bytes).unwrap();
    let rule_path = sources.path().join("RULE.md");
    std::fs::write(&rule_path, rule_bytes).unwrap();
    let context_path = sources.path().join("source.md");
    std::fs::write(&context_path, b"Context remains at the selected source").unwrap();
    let cancel = Cancellation::default();
    let mut original =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    // The caller supplies the already composed agent; Root's template pipeline is qualified separately.
    let composed = [agent_bytes.as_slice(), rule_bytes.as_slice()].concat();
    let descriptor = original.parse_agent(&composed).unwrap();
    let rendered = original
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
        .unwrap();
    let skill_descriptor = original.parse_skill(skill_bytes).unwrap();
    let skill_rendered = original
        .render_skill(
            NativeBinding::KiroV3,
            &skill_descriptor,
            Some("native-selected-skill"),
        )
        .unwrap();
    let auxiliary_rendered = original
        .render_agent(
            NativeBinding::KiroV3,
            &descriptor,
            Some("auxiliary-agent"),
            &ToolGrants {
                binding: NativeBinding::KiroV3,
                read_only: true,
                allows_shell: false,
                tools: vec![],
            },
        )
        .unwrap();
    let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
    let agent = captured_source(&agent_root, graphit_fs::NodeKind::Directory, &original);
    let skill_reader = captured_source(&skill_root, graphit_fs::NodeKind::Directory, &original);
    let rule_reader = captured_source(&rule_path, graphit_fs::NodeKind::File, &original);
    let mut selected = request();
    let mut skill = selected.agent.clone();
    skill.artifact_type = "skill".into();
    skill.id = "selected-skill".into();
    let mut rule = selected.agent.clone();
    rule.artifact_type = "rule".into();
    rule.id = "selected-rule".into();
    let mut context = selected.agent.clone();
    context.source = "project".into();
    context.artifact_type = "knowledge".into();
    context.id = "selected-context".into();
    context.project_id = Some(selected.project.project_id.clone());
    context.instance_id = Some("actual-context-instance".into());
    context.project_kind = Some("project".into());
    selected.artifacts = vec![skill.clone(), rule.clone(), context.clone()];
    let live = LiveSessionService::new(limits());
    let created = live.create(&access, selected).unwrap();
    let allowed = || access.read().is_ok();
    macro_rules! entries {
        ($entry:expr,$skill_rendered:expr,$context_original:expr) => {
            [
                PreparedSelection::NativeText {
                    artifact: CapturedArtifact {
                        selection: &skill,
                        reader: &skill_reader,
                    },
                    source_entry: $entry,
                    rendered: $skill_rendered,
                },
                PreparedSelection::InlineRule(CapturedArtifact {
                    selection: &rule,
                    reader: &rule_reader,
                }),
                PreparedSelection::ReadContext {
                    selection: &context,
                    original: $context_original,
                },
            ]
        };
    }
    let entries = entries!(Path::new("SKILL.md"), &skill_rendered, original.snapshot());
    macro_rules! make {
        ($entries:expr) => {
            Preparation {
                agent: CapturedArtifact {
                    selection: &created.agent,
                    reader: &agent,
                },
                agent_source_entry: Path::new("AGENT.md"),
                rendered: &rendered,
                artifacts: $entries,
                allowed: &allowed,
            }
        };
    }
    assert!(matches!(
        live.prepare(&access, &created.id, make!(&entries[..1])),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists
    ));
    let wrong_kind = [
        PreparedSelection::InlineRule(CapturedArtifact {
            selection: &skill,
            reader: &skill_reader,
        }),
        PreparedSelection::InlineRule(CapturedArtifact {
            selection: &rule,
            reader: &rule_reader,
        }),
        PreparedSelection::ReadContext {
            selection: &context,
            original: original.snapshot(),
        },
    ];
    assert_eq!(
        live.prepare(&access, &created.id, make!(&wrong_kind))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    let unsafe_entry = entries!(
        Path::new("../SKILL.md"),
        &skill_rendered,
        original.snapshot()
    );
    assert_eq!(
        live.prepare(&access, &created.id, make!(&unsafe_entry))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    let mut unsafe_native = skill_rendered.clone();
    unsafe_native.relative_entry = "../escape/SKILL.md".into();
    let unsafe_native_entries =
        entries!(Path::new("SKILL.md"), &unsafe_native, original.snapshot());
    assert_eq!(
        live.prepare(&access, &created.id, make!(&unsafe_native_entries))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    let mut colliding_native = skill_rendered.clone();
    colliding_native.relative_entry = rendered.relative_entry.clone();
    let collision_entries = entries!(
        Path::new("SKILL.md"),
        &colliding_native,
        original.snapshot()
    );
    assert!(matches!(
        live.prepare(&access, &created.id, make!(&collision_entries)),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists
    ));
    let absent_entry = entries!(Path::new("absent.md"), &skill_rendered, original.snapshot());
    assert!(
        live.prepare(&access, &created.id, make!(&absent_entry))
            .is_err()
    );
    let mut wrong_binding = skill_rendered.clone();
    wrong_binding.provenance.binding = NativeBinding::Codex;
    let wrong_binding_entries =
        entries!(Path::new("SKILL.md"), &wrong_binding, original.snapshot());
    assert_eq!(
        live.prepare(&access, &created.id, make!(&wrong_binding_entries))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    let other_cancel = Cancellation::default();
    let other = RequestContext::new(
        content(),
        &other_cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let other_entries = entries!(Path::new("SKILL.md"), &skill_rendered, other.snapshot());
    assert_eq!(
        live.prepare(&access, &created.id, make!(&other_entries))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(
        live.slot(&access, &created.id)
            .unwrap()
            .lock()
            .unwrap()
            .workspace
            .is_none()
    );
    // A real SDK entry colliding with either an asset file or directory must fail before birth.
    for directory_collision in [false, true] {
        let collision_root = tempfile::tempdir().unwrap();
        std::fs::write(collision_root.path().join("AGENT.md"), agent_bytes).unwrap();
        let basename = Path::new(&auxiliary_rendered.relative_entry)
            .file_name()
            .unwrap();
        let asset = collision_root.path().join(basename);
        if directory_collision {
            std::fs::create_dir(&asset).unwrap();
            std::fs::write(asset.join("nested.bin"), b"directory asset").unwrap();
        } else {
            std::fs::write(&asset, b"file asset").unwrap();
        }
        let collision_reader = captured_source(
            collision_root.path(),
            graphit_fs::NodeKind::Directory,
            &original,
        );
        let mut collision_request = request();
        collision_request.request_id = format!("asset-collision-{directory_collision}");
        let mut auxiliary = collision_request.agent.clone();
        auxiliary.id = "auxiliary-agent".into();
        collision_request.artifacts = vec![auxiliary.clone()];
        let collision_session = live.create(&access, collision_request).unwrap();
        let collision_artifacts = [PreparedSelection::NativeText {
            artifact: CapturedArtifact {
                selection: &auxiliary,
                reader: &collision_reader,
            },
            source_entry: Path::new("AGENT.md"),
            rendered: &auxiliary_rendered,
        }];
        assert!(matches!(
            live.prepare(
                &access,
                &collision_session.id,
                Preparation {
                    agent: CapturedArtifact {
                        selection: &collision_session.agent,
                        reader: &agent
                    },
                    agent_source_entry: Path::new("AGENT.md"),
                    rendered: &rendered,
                    artifacts: &collision_artifacts,
                    allowed: &allowed,
                }
            ),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists
        ));
        assert!(
            live.slot(&access, &collision_session.id)
                .unwrap()
                .lock()
                .unwrap()
                .workspace
                .is_none()
        );
        live.close_with_effect(&access, &collision_session.id)
            .unwrap();
        live.delete_with_effect(&access, &collision_session.id)
            .unwrap();
    }
    let mut colliding_asset = skill_rendered.clone();
    colliding_asset.relative_entry = Path::new(&rendered.relative_entry)
        .parent()
        .unwrap()
        .join("assets/primary.bin")
        .to_str()
        .unwrap()
        .to_owned();
    let cross_entries = entries!(Path::new("SKILL.md"), &colliding_asset, original.snapshot());
    assert_eq!(
        live.prepare(&access, &created.id, make!(&cross_entries))
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert!(
        live.slot(&access, &created.id)
            .unwrap()
            .lock()
            .unwrap()
            .workspace
            .is_none()
    );
    assert_eq!(
        live.prepare(&access, &created.id, make!(&entries))
            .unwrap()
            .state,
        SessionState::Ready
    );
    let directory = live
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
    assert_eq!(
        std::fs::read(directory.join(&rendered.relative_entry)).unwrap(),
        rendered.bytes
    );
    assert!(String::from_utf8_lossy(&rendered.bytes).contains("SELECTED-RULE-LITERAL"));
    let native_primary_asset = directory
        .join(&rendered.relative_entry)
        .parent()
        .unwrap()
        .join("assets/primary.bin");
    assert_eq!(std::fs::read(&native_primary_asset).unwrap(), primary_asset);
    assert_eq!(
        std::fs::read(agent_root.join("assets/primary.bin")).unwrap(),
        primary_asset
    );
    let native_skill = directory.join(&skill_rendered.relative_entry);
    assert_eq!(std::fs::read(&native_skill).unwrap(), skill_rendered.bytes);
    assert_ne!(skill_rendered.bytes.as_slice(), skill_bytes);
    let native_asset = native_skill.parent().unwrap().join("assets/literal.bin");
    assert_eq!(std::fs::read(&native_asset).unwrap(), asset_bytes);
    assert!(!directory.join("artifacts").exists());
    assert!(!directory.join("RULE.md").exists());
    assert!(!directory.join("source.md").exists());
    assert_eq!(
        std::fs::read(skill_root.join("SKILL.md")).unwrap(),
        skill_bytes
    );
    assert_eq!(
        std::fs::read(skill_root.join("assets/literal.bin")).unwrap(),
        asset_bytes
    );
    assert_eq!(std::fs::read(&rule_path).unwrap(), rule_bytes);
    live.prepare(&access, &created.id, make!(&entries)).unwrap();
    std::fs::write(&native_asset, b"changed materialized asset").unwrap();
    assert!(matches!(
        live.prepare(&access, &created.id, make!(&entries)),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists
    ));
    std::fs::write(&native_asset, &asset_bytes).unwrap();
    live.prepare(&access, &created.id, make!(&entries)).unwrap();

    for changed in [
        [asset_bytes.as_slice(), &[0x7f]].concat(),
        asset_bytes[..asset_bytes.len() - 1].to_vec(),
    ] {
        std::fs::write(&native_asset, changed).unwrap();
        assert!(matches!(
            live.prepare(&access, &created.id, make!(&entries)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists
        ));
    }
    std::fs::write(&native_asset, &asset_bytes).unwrap();
    live.prepare(&access, &created.id, make!(&entries)).unwrap();

    std::fs::write(
        &native_primary_asset,
        [primary_asset.as_slice(), b"extra"].concat(),
    )
    .unwrap();
    assert_eq!(
        live.prepare(&access, &created.id, make!(&entries))
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    std::fs::write(&native_primary_asset, primary_asset).unwrap();
    live.prepare(&access, &created.id, make!(&entries)).unwrap();

    let fresh_cancel = Cancellation::default();
    let fresh = RequestContext::new(
        content(),
        &fresh_cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let fresh_access = Access::service(&fixture.auth, &fixture.raw, &fresh).unwrap();
    let fresh_agent = captured_source(&agent_root, graphit_fs::NodeKind::Directory, &fresh);
    let fresh_skill = captured_source(&skill_root, graphit_fs::NodeKind::Directory, &fresh);
    let fresh_rule = captured_source(&rule_path, graphit_fs::NodeKind::File, &fresh);
    let fresh_entries = [
        PreparedSelection::NativeText {
            artifact: CapturedArtifact {
                selection: &skill,
                reader: &fresh_skill,
            },
            source_entry: Path::new("SKILL.md"),
            rendered: &skill_rendered,
        },
        PreparedSelection::InlineRule(CapturedArtifact {
            selection: &rule,
            reader: &fresh_rule,
        }),
        PreparedSelection::ReadContext {
            selection: &context,
            original: fresh.snapshot(),
        },
    ];
    let fresh_allowed = || fresh_access.read().is_ok();
    macro_rules! fresh_input {
        ($entries:expr) => {
            Preparation {
                agent: CapturedArtifact {
                    selection: &created.agent,
                    reader: &fresh_agent,
                },
                agent_source_entry: Path::new("AGENT.md"),
                rendered: &rendered,
                artifacts: $entries,
                allowed: &fresh_allowed,
            }
        };
    }
    live.prepare(&fresh_access, &created.id, fresh_input!(&fresh_entries))
        .unwrap();
    let mut relocated = skill_rendered.clone();
    relocated.relative_entry = ".kiro/skills/another-name/SKILL.md".into();
    let relocated_entries = [
        PreparedSelection::NativeText {
            artifact: CapturedArtifact {
                selection: &skill,
                reader: &fresh_skill,
            },
            source_entry: Path::new("SKILL.md"),
            rendered: &relocated,
        },
        PreparedSelection::InlineRule(CapturedArtifact {
            selection: &rule,
            reader: &fresh_rule,
        }),
        PreparedSelection::ReadContext {
            selection: &context,
            original: fresh.snapshot(),
        },
    ];
    assert!(matches!(
        live.prepare(&fresh_access, &created.id, fresh_input!(&relocated_entries)),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists
    ));
    // Context content changes do not alter the installed agent/skill/rule payloads.
    std::fs::write(&context_path, b"Source changed after capture").unwrap();
    live.prepare(&access, &created.id, make!(&entries)).unwrap();
    live.prepare(&fresh_access, &created.id, fresh_input!(&fresh_entries))
        .unwrap();
    live.close_with_effect(&fresh_access, &created.id).unwrap();
    // A closed session remains terminal even after a fresh authorized preparation.
    assert!(matches!(
        live.prepare(&fresh_access, &created.id, fresh_input!(&fresh_entries)),
        Err(LiveError::Closed)
    ));
    let resolver_failure =
        live.fail_preparation(&fresh_access, &created, io::ErrorKind::NotFound.into());
    assert_eq!(resolver_failure.kind(), io::ErrorKind::NotFound);
    assert_eq!(
        resolver_failure.effect(),
        NativeDocumentJournalEffect::Unchanged
    );
    assert!(
        live.slot(&fresh_access, &created.id)
            .unwrap()
            .lock()
            .unwrap()
            .preparation_failure
            .is_none()
    );
    assert_eq!(
        live.observe(&fresh_access, &created.id, None)
            .unwrap()
            .replay
            .session
            .state,
        SessionState::Closed
    );
    live.delete_with_effect(&fresh_access, &created.id).unwrap();
    assert!(!directory.exists());
}

#[path = "native_tests.rs"]
#[cfg(unix)]
pub(crate) mod native;

#[path = "preparation_lifecycle_tests.rs"]
mod preparation_lifecycle;

#[path = "read_contexts_tests.rs"]
mod read_contexts;

#[path = "preparation_guard_tests.rs"]
mod preparation_guard;

#[test]
fn diagnostics_preserve_secondary_and_persistence_through_effect_wrappers_without_native_text() {
    let cause = LiveError::History(journal::Failure {
        cause: io::Error::new(io::ErrorKind::AlreadyExists, LiveError::Conflict),
        effect: NativeDocumentJournalEffect::Unchanged,
        secondary: Some(io::ErrorKind::PermissionDenied),
    });
    let failure = LiveError::Preparation {
        cause: Box::new(cause),
        persistence: Some(Box::new(LiveError::after_effect(
            io::Error::new(io::ErrorKind::Interrupted, "private-native-path-and-token"),
            NativeDocumentJournalEffect::Unknown,
        ))),
        effect: NativeDocumentJournalEffect::Unchanged,
    }
    .retain_prior_effect(NativeDocumentJournalEffect::Applied);
    assert!(matches!(failure.primary_cause(), LiveError::Conflict));
    let diagnostic = serde_json::to_value(failure.diagnostic()).unwrap();
    assert_eq!(diagnostic["journal_effect"], "Applied");
    assert_eq!(diagnostic["secondary_kind"], "PermissionDenied");
    assert_eq!(diagnostic["persistence"]["kind"], "Interrupted");
    assert_eq!(diagnostic["persistence"]["journal_effect"], "Unknown");
    assert!(
        !diagnostic
            .to_string()
            .contains("private-native-path-and-token")
    );
}
