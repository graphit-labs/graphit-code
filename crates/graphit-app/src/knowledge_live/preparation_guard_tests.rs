use super::*;

#[tokio::test]
async fn guard_owns_precreate_cancellation_and_exclusion_without_fabricating_history() {
    let fixture = new_fixture();
    let ca = Cancellation::default();
    let a = RequestContext::new(content(), &ca, Instant::now() + Duration::from_secs(30)).unwrap();
    let aa = Access::service(&fixture.auth, &fixture.raw, &a).unwrap();
    let cb = Cancellation::default();
    let b = RequestContext::new(content(), &cb, Instant::now() + Duration::from_secs(30)).unwrap();
    let ab = Access::service(&fixture.auth, &fixture.raw, &b).unwrap();
    let live = LiveSessionService::new(limits());
    let guard = live.begin_preparation(&aa, "history-request").unwrap();
    assert!(guard.previous_session().is_none());
    assert!(matches!(
        live.begin_preparation(&ab, "history-request"),
        Err(LiveError::Busy)
    ));
    assert!(matches!(live.create(&ab, request()), Err(LiveError::Busy)));
    assert_eq!(
        guard.recover_interrupted(&ab).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(live.list(&ab).unwrap().is_empty());
    let (requested, effect) = live.cancel_with_effect(&ab, "history-request").unwrap();
    assert!(requested);
    assert_eq!(effect, NativeDocumentJournalEffect::Unchanged);
    assert_eq!(a.check().unwrap_err().kind(), io::ErrorKind::Interrupted);
    assert!(matches!(
        live.close_with_effect(&ab, "history-request"),
        Err(LiveError::Busy)
    ));
    assert!(matches!(
        live.delete_with_effect(&ab, "history-request"),
        Err(LiveError::Busy)
    ));
    assert!(live.list(&ab).unwrap().is_empty());
    drop(guard);
    let retry = live.begin_preparation(&ab, "history-request").unwrap();
    assert!(retry.previous_session().is_none());
    let created = live.create(&ab, request()).unwrap();
    assert!(retry.previous_session().is_none());
    assert!(retry.recover_interrupted(&ab).unwrap().is_none());
    let cc = Cancellation::default();
    let c = RequestContext::new(content(), &cc, Instant::now() + Duration::from_secs(30)).unwrap();
    let ac = Access::service(&fixture.auth, &fixture.raw, &c).unwrap();
    // An existing Create retry validates equality even while another job owns preparation.
    assert_eq!(live.create(&ac, request()).unwrap(), created);
    let mut different = request();
    different.prompt = Some("different payload".into());
    assert_eq!(
        live.create(&ac, different).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    let other = live.begin_preparation(&ac, "other-id").unwrap();
    assert!(other.previous_session().is_none());
    drop(other);
    assert!(!live.shutdown(Instant::now() + Duration::from_secs(1)).await);
    assert_eq!(b.check().unwrap_err().kind(), io::ErrorKind::Interrupted);
    drop(retry);
    assert!(live.shutdown(Instant::now() + Duration::from_secs(1)).await);
    live.delete_with_effect(&ac, &created.id).unwrap();
}

enum RecoveryCase {
    RegisteredColdRoot,
    #[cfg(unix)]
    Replacement,
}

#[tokio::test]
async fn guard_recovers_registered_cold_root() {
    assert_guard_recovery(RecoveryCase::RegisteredColdRoot);
}

#[cfg(unix)]
#[tokio::test]
async fn guard_preserves_replacement() {
    assert_guard_recovery(RecoveryCase::Replacement);
}

fn assert_guard_recovery(case: RecoveryCase) {
    let fixture = new_fixture();
    let ca = Cancellation::default();
    let a = RequestContext::new(content(), &ca, Instant::now() + Duration::from_secs(30)).unwrap();
    let aa = Access::service(&fixture.auth, &fixture.raw, &a).unwrap();
    let before = LiveSessionService::new(limits());
    let created = before.create(&aa, request()).unwrap();
    let mut workspace = workspace::Workspace::create(&aa, limits().native).unwrap();
    workspace
        .write(
            std::path::Path::new("owned.txt"),
            b"owned bytes",
            a.snapshot(),
        )
        .unwrap();
    let directory = workspace.path().to_owned();
    let record = workspace.record(limits().native, a.snapshot()).unwrap();
    before
        .change(&aa, &created.id, |stored| {
            stored.workspace = Some(record);
            Ok(())
        })
        .unwrap();
    drop(workspace);
    drop(before);
    ca.cancel();
    let cb = Cancellation::default();
    let b = RequestContext::new(content(), &cb, Instant::now() + Duration::from_secs(30)).unwrap();
    let ab = Access::service(&fixture.auth, &fixture.raw, &b).unwrap();
    let cold = LiveSessionService::new(limits());
    let guard = cold.begin_preparation(&ab, &created.id).unwrap();
    assert_eq!(guard.previous_session().unwrap(), &created);
    let mut different = request();
    different.model = "different-model".into();
    assert_eq!(
        cold.create(&ab, different).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    let unchanged = cold.observe(&ab, &created.id, None).unwrap().replay;
    assert_eq!(unchanged.session.state, SessionState::Preparing);
    cold.create(&ab, request()).unwrap();
    match case {
        #[cfg(unix)]
        RecoveryCase::Replacement => {
            use std::os::unix::fs::PermissionsExt;
            let retained = directory.with_extension("retained");
            std::fs::rename(&directory, &retained).unwrap();
            std::fs::create_dir(&directory).unwrap();
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
            std::fs::write(directory.join("foreign.txt"), b"foreign bytes").unwrap();
            let failure = guard.recover_interrupted(&ab).unwrap_err();
            assert_eq!(failure.kind(), io::ErrorKind::AlreadyExists);
            assert_eq!(failure.effect(), NativeDocumentJournalEffect::Unchanged);
            assert_eq!(
                cold.observe(&ab, &created.id, None).unwrap().replay,
                unchanged
            );
            assert_eq!(
                std::fs::read(retained.join("owned.txt")).unwrap(),
                b"owned bytes"
            );
            assert_eq!(
                std::fs::read(directory.join("foreign.txt")).unwrap(),
                b"foreign bytes"
            );
        }
        RecoveryCase::RegisteredColdRoot => {
            let recovered = guard.recover_interrupted(&ab).unwrap().unwrap();
            assert_eq!(recovered.state, SessionState::Failed);
            assert!(guard.recover_interrupted(&ab).unwrap().is_none());
            let failed = cold.observe(&ab, &created.id, None).unwrap().replay;
            assert_eq!(
                failed
                    .events
                    .iter()
                    .filter(|event| matches!(
                        event.data,
                        EventData::Error {
                            code: FailureCode::Interrupted
                        }
                    ))
                    .count(),
                1
            );
            assert!(
                failed
                    .events
                    .iter()
                    .all(|event| !matches!(event.data, EventData::Prompt { .. }))
            );
            assert_eq!(
                std::fs::read(directory.join("owned.txt")).unwrap(),
                b"owned bytes"
            );
            drop(guard);
            cold.close_with_effect(&ab, &created.id).unwrap();
            assert!(directory.exists());
            cold.delete_with_effect(&ab, &created.id).unwrap();
            assert!(!directory.exists());
        }
    }
}

#[tokio::test]
async fn guard_leaves_existing_preparing_without_workspace_for_normal_preparation() {
    let fixture = new_fixture();
    let cancel = Cancellation::default();
    let original =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
    let before = LiveSessionService::new(limits());
    let created = before.create(&access, request()).unwrap();
    drop(before);
    let cold = LiveSessionService::new(limits());
    let guard = cold.begin_preparation(&access, &created.id).unwrap();
    assert!(guard.previous_session().is_some());
    assert!(guard.recover_interrupted(&access).unwrap().is_none());
    assert_eq!(
        cold.observe(&access, &created.id, None)
            .unwrap()
            .replay
            .session,
        created
    );
    drop(guard);
    cold.delete_with_effect(&access, &created.id).unwrap();
}
