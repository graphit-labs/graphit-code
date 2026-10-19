//! Actual native admission slots exercise cancellation before any transport poll.
use super::*;

#[tokio::test]
async fn queued_native_cancel_is_cancelled_without_spawning_or_leaking_an_active_slot() {
    let fixture = new_fixture();
    let native = NativeHarness::new("cancel");
    let cancel = Cancellation::default();
    let mut original =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let rendered = rendered_agent(&mut original);
    let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
    let live = LiveSessionService::new(limits());
    let created = live.create(&access, request()).unwrap();
    prepare_agent(&live, &access, &created.id, &native.agent, &rendered);
    let cwd = live
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
    let service = native.availability.service().unwrap();
    let allowed = || access.read().is_ok();
    // These real transports retain both actual AskService permits. Neither is polled.
    let (transport_a, owner_a) = service
        .kiro_process(&cwd, original.snapshot(), None, &allowed)
        .await
        .unwrap();
    let (transport_b, owner_b) = service
        .kiro_process(&cwd, original.snapshot(), None, &allowed)
        .await
        .unwrap();
    assert!(!owner_a.spawned() && !owner_b.spawned());
    let control_cancel = Cancellation::default();
    let control = RequestContext::new(
        content(),
        &control_cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let control_access = Access::service(&fixture.auth, &fixture.raw, &control).unwrap();
    let send = SendRequest {
        turn_id: "queued-cancel".into(),
        text: PROMPT.into(),
    };
    let run = live.run_turn(&access, &created.id, &send, service, &allowed, None);
    let interrupt = async {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if live
                    .observe(&control_access, &created.id, None)
                    .unwrap()
                    .replay
                    .session
                    .state
                    == SessionState::Running
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("turn is admitted before waiting for the occupied native permits");
        assert!(
            live.cancel_with_effect(&control_access, &created.id)
                .unwrap()
                .0
        );
    };
    let (result, ()) = tokio::join!(run, interrupt);
    let result = result.unwrap();
    let terminal = live
        .observe(&control_access, &created.id, None)
        .unwrap()
        .terminal
        .unwrap();
    assert_eq!(terminal.outcome, TurnOutcome::Cancelled);
    assert_eq!(terminal.error, Some(FailureCode::Cancelled));
    assert!(terminal.joined && terminal.clean);
    assert!(!terminal.persisted);
    assert!(result.persistence_error.is_some());
    assert!(
        native.log().is_empty(),
        "no child initialized or received a prompt"
    );
    assert!(!owner_a.spawned() && !owner_b.spawned());
    assert!(cancel.is_cancelled());
    assert!(!control_cancel.is_cancelled());
    drop((transport_a, transport_b));
    assert!(
        !live
            .cancel_with_effect(&control_access, &created.id)
            .unwrap()
            .0
    );
    let observed = live.observe(&control_access, &created.id, None).unwrap();
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
    live.close_with_effect(&control_access, &created.id)
        .unwrap();
    live.delete_with_effect(&control_access, &created.id)
        .unwrap();
    assert!(!cwd.exists());
    assert!(live.shutdown(Instant::now() + Duration::from_secs(3)).await);
    assert!(service.shutdown().await.is_ok());
}
