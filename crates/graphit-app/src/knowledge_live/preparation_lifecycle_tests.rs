use super::*;
use std::{
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

#[tokio::test]
async fn preparation_failure_retains_born_workspace_cause_effect_and_fresh_recovery() {
    for cancelled in [false, true] {
        let fixture = new_fixture();
        let sources = tempfile::tempdir().unwrap();
        let source = sources.path().join("AGENT.md");
        let bytes = b"---\nname: documentary-agent\ndescription: Failure fixture\nread_only: true\nallows_shell: false\n---\nRead selected sources.\n";
        std::fs::write(&source, bytes).unwrap();
        let cancel = Cancellation::default();
        let mut original =
            RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30))
                .unwrap();
        let descriptor = original.parse_agent(bytes).unwrap();
        let rendered = original
            .render_agent(
                NativeBinding::KiroV3,
                &descriptor,
                None,
                &graphit_hub::native::ToolGrants {
                    binding: NativeBinding::KiroV3,
                    read_only: true,
                    allows_shell: false,
                    tools: vec![],
                },
            )
            .unwrap();
        let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
        let reader = captured_source(&source, graphit_fs::NodeKind::File, &original);
        let live = LiveSessionService::new(limits());
        let created = live.create(&access, request()).unwrap();
        // File capture keys are relative to that File, not its parent directory.
        assert!(reader.open_file(Path::new("")).is_ok());
        assert_eq!(
            reader
                .open_file(Path::new("AGENT.md"))
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::NotFound
        );
        let calls = AtomicUsize::new(0);
        let allowed = || {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                return true;
            }
            if cancelled {
                cancel.cancel();
                true
            } else {
                false
            }
        };
        let failure = live
            .prepare(
                &access,
                &created.id,
                Preparation {
                    agent: CapturedArtifact {
                        selection: &created.agent,
                        reader: &reader,
                    },
                    agent_source_entry: Path::new(""),
                    rendered: &rendered,
                    artifacts: &[],
                    allowed: &allowed,
                },
            )
            .unwrap_err();
        assert_eq!(
            failure.kind(),
            if cancelled {
                io::ErrorKind::Interrupted
            } else {
                io::ErrorKind::PermissionDenied
            }
        );
        assert_eq!(failure.effect(), NativeDocumentJournalEffect::Applied);
        if cancelled {
            assert_eq!(
                failure.preparation_persistence_error().unwrap().kind(),
                io::ErrorKind::Interrupted
            );
        } else {
            assert!(failure.preparation_persistence_error().is_none());
        }
        let slot = live
            .slots
            .lock()
            .unwrap()
            .values()
            .find(|slot| slot.lock().unwrap().workspace.is_some())
            .cloned()
            .unwrap();
        let directory = slot
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
        assert_eq!(std::fs::read(&source).unwrap(), bytes);
        let fresh_cancel = Cancellation::default();
        let fresh = RequestContext::new(
            content(),
            &fresh_cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let fresh_access = Access::service(&fixture.auth, &fixture.raw, &fresh).unwrap();
        let recorded = live
            .journal(&fresh_access)
            .unwrap()
            .stored(&created.id, fresh.snapshot(), || fresh_access.read())
            .unwrap();
        assert!(recorded.workspace.is_some());
        assert_eq!(
            recorded.session.state,
            if cancelled {
                SessionState::Preparing
            } else {
                SessionState::Failed
            }
        );
        assert!(
            recorded
                .events
                .iter()
                .all(|event| !matches!(event.data, EventData::Prompt { .. }))
        );
        if cancelled {
            let recovery_cancel = Cancellation::default();
            let recovery = RequestContext::new(
                content(),
                &recovery_cancel,
                Instant::now() + Duration::from_secs(30),
            )
            .unwrap();
            let recovery_access = Access::service(&fixture.auth, &fixture.raw, &recovery).unwrap();
            let guard = live
                .begin_preparation(&recovery_access, &created.id)
                .unwrap();
            // This persists the real pending preparation failure. Some ends the Root job;
            // a later Cancel cannot continue into a resolver and replace Applied by Unchanged.
            let recovered = guard
                .recover_interrupted(&recovery_access)
                .unwrap()
                .expect("known recovery must end the job");
            assert_eq!(recovered.state, SessionState::Failed);
            assert_eq!(
                live.cancel_with_effect(&fresh_access, &created.id).unwrap(),
                (true, NativeDocumentJournalEffect::Unchanged)
            );
            assert_eq!(
                recovery.check().unwrap_err().kind(),
                io::ErrorKind::Interrupted
            );
            drop(guard);
        } else {
            assert_eq!(
                recover_with_guard(&live, &fresh_access, &created.id)
                    .unwrap()
                    .state,
                SessionState::Failed
            );
        }
        let first = live
            .observe(&fresh_access, &created.id, None)
            .unwrap()
            .replay;
        recover_with_guard(&live, &fresh_access, &created.id).unwrap();
        assert_eq!(
            live.observe(&fresh_access, &created.id, None)
                .unwrap()
                .replay,
            first
        );
        assert_eq!(
            first
                .events
                .iter()
                .filter(|event| matches!(event.data, EventData::Error { .. }))
                .count(),
            1
        );
        assert!(slot.lock().unwrap().preparation_failure.is_none());
        live.close_with_effect(&fresh_access, &created.id).unwrap();
        assert!(directory.exists());
        live.delete_with_effect(&fresh_access, &created.id).unwrap();
        assert!(!directory.exists());
        let recreated = live.create(&fresh_access, request()).unwrap();
        assert_ne!(recreated.cursor.generation, created.cursor.generation);
        let before_late = live
            .observe(&fresh_access, &recreated.id, None)
            .unwrap()
            .replay;
        let late = live.fail_preparation(&fresh_access, &created, failure);
        assert_eq!(late.effect(), NativeDocumentJournalEffect::Applied);
        assert_eq!(
            live.observe(&fresh_access, &recreated.id, None)
                .unwrap()
                .replay,
            before_late
        );
        assert!(slot.lock().unwrap().preparation_failure.is_none());
        assert!(slot.lock().unwrap().workspace.is_none());
        live.delete_with_effect(&fresh_access, &recreated.id)
            .unwrap();
    }
}

#[tokio::test]
async fn cold_preparation_without_prompt_recovers_failed_and_preserves_foreign_workspace() {
    for foreign in [false, true] {
        let fixture = new_fixture();
        let cancel = Cancellation::default();
        let original =
            RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30))
                .unwrap();
        let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
        let before = LiveSessionService::new(limits());
        let created = before.create(&access, request()).unwrap();
        let workspace = workspace::Workspace::create(&access, limits().native).unwrap();
        let directory = workspace.path().to_owned();
        let record = workspace
            .record(limits().native, original.snapshot())
            .unwrap();
        before
            .change(&access, &created.id, |stored| {
                stored.workspace = Some(record);
                Ok(())
            })
            .unwrap();
        drop(workspace);
        drop(before);
        let cold = LiveSessionService::new(limits());
        assert_eq!(
            recover_with_guard(&cold, &access, &created.id)
                .unwrap()
                .state,
            SessionState::Failed
        );
        assert_eq!(
            cold.close_with_effect(&access, &created.id)
                .unwrap()
                .0
                .state,
            SessionState::Closed
        );
        assert!(directory.is_dir());
        if foreign {
            let retained = directory.with_extension("retained");
            std::fs::rename(&directory, &retained).unwrap();
            std::fs::create_dir(&directory).unwrap();
            std::fs::write(directory.join("foreign"), b"preserve").unwrap();
            assert_eq!(
                cold.delete_with_effect(&access, &created.id)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::AlreadyExists
            );
            assert_eq!(
                std::fs::read(directory.join("foreign")).unwrap(),
                b"preserve"
            );
            assert!(retained.is_dir());
        } else {
            cold.delete_with_effect(&access, &created.id).unwrap();
            assert!(!directory.exists());
        }
    }
}

#[tokio::test]
async fn cancelled_birth_handle_survives_failed_record_capture_until_fresh_recovery() {
    let fixture = new_fixture();
    let cancel = Cancellation::default();
    let original =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
    let live = LiveSessionService::new(limits());
    let created = live.create(&access, request()).unwrap();
    let slot = live.slot(&access, &created.id).unwrap();
    let workspace = workspace::Workspace::create(&access, limits().native).unwrap();
    let directory = workspace.path().to_owned();
    slot.lock().unwrap().workspace = Some(Arc::new(std::sync::Mutex::new(workspace)));
    cancel.cancel();
    let cause = slot
        .lock()
        .unwrap()
        .workspace
        .as_ref()
        .unwrap()
        .lock()
        .unwrap()
        .record(limits().native, original.snapshot())
        .err()
        .unwrap();
    assert_eq!(cause.kind(), io::ErrorKind::Interrupted);
    let failure = live.fail_preparation_in_slot(
        &access,
        &created.id,
        &created.cursor.generation,
        &slot,
        cause.into(),
    );
    assert_eq!(failure.kind(), io::ErrorKind::Interrupted);
    assert_eq!(failure.effect(), NativeDocumentJournalEffect::Unchanged);
    assert_eq!(
        failure.preparation_persistence_error().unwrap().kind(),
        io::ErrorKind::Interrupted
    );
    assert!(directory.is_dir());
    let fresh_cancel = Cancellation::default();
    let fresh = RequestContext::new(
        content(),
        &fresh_cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let fresh_access = Access::service(&fixture.auth, &fixture.raw, &fresh).unwrap();
    assert!(
        live.journal(&fresh_access)
            .unwrap()
            .stored(&created.id, fresh.snapshot(), || fresh_access.read())
            .unwrap()
            .workspace
            .is_none()
    );
    assert_eq!(
        recover_with_guard(&live, &fresh_access, &created.id)
            .unwrap()
            .state,
        SessionState::Failed
    );
    assert!(
        live.journal(&fresh_access)
            .unwrap()
            .stored(&created.id, fresh.snapshot(), || fresh_access.read())
            .unwrap()
            .workspace
            .is_some()
    );
    live.delete_with_effect(&fresh_access, &created.id).unwrap();
    assert!(!directory.exists());
}
