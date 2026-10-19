use super::*;
use std::path::Path;

#[tokio::test]
async fn prepared_context_selections_are_atomic_and_readable_without_a_run() {
    let fixture = new_fixture();
    let sources = tempfile::tempdir().unwrap();
    let source = sources.path().join("AGENT.md");
    let bytes = b"---\nname: documentary-agent\ndescription: Knowledge context snapshot fixture\nread_only: true\nallows_shell: false\n---\nRead selected sources.\n";
    std::fs::write(&source, bytes).unwrap();
    let cancel = Cancellation::default();
    let mut original =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
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
    let mut selected = request();
    let mut first = selected.agent.clone();
    first.artifact_type = "knowledge".into();
    first.source = "project".into();
    first.project_id = Some(selected.project.project_id.clone());
    first.instance_id = Some("captured-context-instance".into());
    first.project_kind = Some("project".into());
    first.id = "first-context".into();
    let mut second = first.clone();
    second.id = "second-context".into();
    let mut ast = first.clone();
    ast.artifact_type = "ast".into();
    ast.id = "ast-context".into();
    selected.artifacts = vec![first.clone(), ast.clone(), second.clone()];
    let live = LiveSessionService::new(limits());
    let created = live.create(&access, selected).unwrap();
    let generation = &created.cursor.generation;
    assert!(matches!(
        live.read_contexts(&access, &created.id, generation),
        Err(LiveError::NotReady)
    ));
    assert_eq!(
        live.read_contexts(&access, "absent", generation)
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
    let entries = [
        PreparedSelection::ReadContext {
            selection: &first,
            original: original.snapshot(),
        },
        PreparedSelection::ReadContext {
            selection: &ast,
            original: original.snapshot(),
        },
        PreparedSelection::ReadContext {
            selection: &second,
            original: original.snapshot(),
        },
    ];
    macro_rules! input {
        () => {
            input!(&entries)
        };
        ($selected:expr) => {
            Preparation {
                agent: CapturedArtifact {
                    selection: &created.agent,
                    reader: &reader,
                },
                agent_source_entry: Path::new(""),
                rendered: &rendered,
                artifacts: $selected,
                allowed: &|| access.read().is_ok(),
            }
        };
    }
    let other_cancel = Cancellation::default();
    let other =
        RequestContext::new(content(), &other_cancel, original.deadline().unwrap()).unwrap();
    let other_entries = [
        PreparedSelection::ReadContext {
            selection: &first,
            original: other.snapshot(),
        },
        PreparedSelection::ReadContext {
            selection: &ast,
            original: original.snapshot(),
        },
        PreparedSelection::ReadContext {
            selection: &second,
            original: original.snapshot(),
        },
    ];
    assert_eq!(
        live.prepare(&access, &created.id, input!(&other_entries))
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
    let ready = live.prepare(&access, &created.id, input!()).unwrap();
    let snapshot = live
        .read_contexts(&access, &created.id, generation)
        .unwrap();
    assert_eq!(snapshot.session, ready);
    assert_eq!(
        snapshot.contexts,
        ReadContexts {
            selected: vec![
                ReadContext {
                    selection: first.clone(),
                },
                ReadContext {
                    selection: ast.clone(),
                },
                ReadContext {
                    selection: second.clone(),
                },
            ]
        }
    );
    assert_eq!(snapshot.last_prompt, None);
    assert!(
        live.slot(&access, &created.id)
            .unwrap()
            .lock()
            .unwrap()
            .process
            .is_none()
    );
    assert!(matches!(
        live.read_contexts(&access, &created.id, "other-generation"),
        Err(LiveError::Conflict)
    ));
    assert_eq!(
        live.observe(&access, &created.id, None)
            .unwrap()
            .replay
            .session,
        ready
    );
    let stored = live
        .journal(&access)
        .unwrap()
        .stored(&created.id, original.snapshot(), || access.read())
        .unwrap();
    let mut encoded = serde_json::to_value(&stored).unwrap();
    encoded.as_object_mut().unwrap().remove("read_contexts");
    assert!(serde_json::from_value::<journal::StoredSession>(encoded).is_err());
    for mutation in 0..6 {
        let failure = live
            .journal(&access)
            .unwrap()
            .change(
                original.snapshot(),
                || access.write(),
                |sessions| {
                    let stored = sessions.get_mut(&created.id).unwrap();
                    let contexts = stored.read_contexts.as_mut().unwrap();
                    match mutation {
                        0 => contexts.selected.reverse(),
                        1 => contexts.selected[1].selection.version = "different-version".into(),
                        2 => {
                            contexts.selected.remove(1);
                        }
                        3 => contexts.selected.push(contexts.selected[1].clone()),
                        4 => contexts.selected.swap(0, 1),
                        _ => contexts.selected[1].selection.artifact_type = "knowledge".into(),
                    }
                    Ok(())
                },
            )
            .unwrap_err();
        assert_eq!(failure.cause.kind(), io::ErrorKind::InvalidData);
        assert_eq!(failure.effect, NativeDocumentJournalEffect::Unchanged);
        assert_eq!(
            live.read_contexts(&access, &created.id, generation)
                .unwrap(),
            snapshot
        );
    }
    live.emit_fixture(
        &access,
        &created.id,
        Some("first-turn"),
        EventData::Prompt {
            text: "first".into(),
        },
    )
    .unwrap();
    live.emit_fixture(
        &access,
        &created.id,
        Some("last-turn"),
        EventData::Prompt {
            text: "last".into(),
        },
    )
    .unwrap();
    let after = live
        .read_contexts(&access, &created.id, generation)
        .unwrap();
    assert_eq!(after.last_prompt.as_deref(), Some("last-turn"));
    assert_eq!(after.contexts, snapshot.contexts);
    assert_eq!(after.session.state, SessionState::Ready);
    live.emit_fixture(
        &access,
        &created.id,
        Some("last-turn"),
        EventData::TurnDone {
            outcome: TurnOutcome::Completed,
            joined: true,
            clean: true,
        },
    )
    .unwrap();
    // Re-preparing the same selections does not depend on a corpus stamp.
    let repeated = live.prepare(&access, &created.id, input!()).unwrap();
    assert_eq!(repeated.id, created.id);
    assert_eq!(
        live.read_contexts(&access, &created.id, generation)
            .unwrap()
            .contexts,
        snapshot.contexts
    );
    live.close_with_effect(&access, &created.id).unwrap();
    assert_eq!(
        live.read_contexts(&access, &created.id, generation)
            .unwrap()
            .session
            .state,
        SessionState::Closed
    );
    live.delete_with_effect(&access, &created.id).unwrap();
    let recreated = live.create(&access, request()).unwrap();
    assert_ne!(recreated.cursor.generation, *generation);
    assert!(matches!(
        live.read_contexts(&access, &created.id, generation),
        Err(LiveError::Conflict)
    ));
    cancel.cancel();
    assert_eq!(
        live.read_contexts(&access, &created.id, &recreated.cursor.generation)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
}

#[tokio::test]
async fn knowledge_snapshot_rejects_oversized_and_corrupt_journals_without_repair() {
    let fixture = new_fixture();
    let cancel = Cancellation::default();
    let original =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let access = Access::service(&fixture.auth, &fixture.raw, &original).unwrap();
    let live = LiveSessionService::new(limits());
    let created = live.create(&access, request()).unwrap();
    let mut small = limits();
    small.envelope_bytes = NonZeroUsize::new(128).unwrap();
    let bounded = LiveSessionService::new(small);
    let failure = bounded
        .read_contexts(&access, &created.id, &created.cursor.generation)
        .unwrap_err();
    assert_eq!(failure.kind(), io::ErrorKind::FileTooLarge);
    assert_eq!(failure.effect(), NativeDocumentJournalEffect::Unchanged);
    let namespace = NativeDocumentNamespace::capture(
        access.scope(),
        &access.scope().owner_root().join("knowledge-live"),
        limits().native,
        original.snapshot(),
    )
    .unwrap();
    let guard = namespace.lock(original.snapshot()).unwrap();
    let before = guard.read_journal(limits().envelope_bytes.get()).unwrap();
    let mut body: serde_json::Value =
        serde_json::from_str(before.body().unwrap().unwrap().get()).unwrap();
    body["sessions"][&created.id]
        .as_object_mut()
        .unwrap()
        .remove("read_contexts");
    let invalid =
        serde_json::value::RawValue::from_string(serde_json::to_string(&body).unwrap()).unwrap();
    guard
        .replace_journal(
            &before,
            &invalid,
            NativeDocumentJournalWriteBounds::new(1024 * 1024, 1024 * 1024, 1024 * 1024).unwrap(),
        )
        .unwrap();
    drop(guard);
    let failure = live
        .read_contexts(&access, &created.id, &created.cursor.generation)
        .unwrap_err();
    assert_eq!(failure.kind(), io::ErrorKind::InvalidData);
    assert_eq!(failure.effect(), NativeDocumentJournalEffect::Unchanged);
    assert_eq!(
        namespace
            .lock(original.snapshot())
            .unwrap()
            .read_journal(limits().envelope_bytes.get())
            .unwrap()
            .body()
            .unwrap()
            .unwrap()
            .get(),
        invalid.get()
    );
}
