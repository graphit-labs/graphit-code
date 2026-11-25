use super::*;

fn cursor_binding() -> ConversationBinding {
    let mut metadata = binding();
    metadata.engine = ConversationEngine::Cursor;
    metadata.provider = "cursor".into();
    metadata
}
fn cursor_command() -> Request {
    let mut request = command();
    request.args = [
        "--print",
        "--output-format",
        "stream-json",
        "--model",
        "literal-model",
        "--mode",
        "ask",
        "Authorized documentary prompt β",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    request.stdin = None;
    request
}
#[test]
fn cursor_native_continuity_cold_reopen_keeps_exact_private_identity_and_request() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    let mut turn = store
        .begin_create("cursor-selected", cursor_binding(), &current)
        .unwrap();
    let mut request = cursor_command();
    let args = request.args.clone();
    let environment = request.environment.clone();
    let cwd = request.cwd.clone();
    turn.configure_cursor_request(&mut request, &current)
        .unwrap();
    assert_eq!(request.args, args);
    assert_eq!(request.environment, environment);
    assert_eq!(request.cwd, cwd);
    assert!(request.stdin.is_none());
    assert_eq!(
        kind(turn.commit(&current)),
        ConversationErrorKind::NativeSession
    );
    turn.observe_cursor_session("cursor-native-observed", &current)
        .unwrap();
    let projection = turn.commit(&current).unwrap();
    assert_eq!(
        serde_json::to_value(projection).unwrap()["engine"],
        "cursor"
    );
    drop(turn);
    drop(store);
    let next = original(Duration::from_secs(20));
    let reopened = f.store("A", &next, limits());
    let public = serde_json::to_string(&reopened.read("cursor-selected", &next).unwrap()).unwrap();
    assert!(!public.contains("cursor-native-observed"));
    assert!(!public.contains("native_id"));
    assert!(
        f.store("B", &next, limits())
            .list(&next)
            .unwrap()
            .is_empty()
    );
    let mut turn = reopened
        .begin_turn("cursor-selected", &cursor_binding(), &next)
        .unwrap();
    let mut resumed = cursor_command();
    turn.configure_cursor_request(&mut resumed, &next).unwrap();
    assert_eq!(
        &resumed.args[7..9],
        [
            OsString::from("--resume"),
            OsString::from("cursor-native-observed")
        ]
    );
    assert_eq!(resumed.args[9], "Authorized documentary prompt β");
    assert!(resumed.stdin.is_none());
    assert_eq!(resumed.environment, environment);
    assert_eq!(resumed.cwd, cwd);
    turn.observe_cursor_session("cursor-native-observed", &next)
        .unwrap();
    turn.commit(&next).unwrap();
}
#[test]
fn cursor_continuity_rejects_invalid_requests_originals_and_sticky_mismatch() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    let mut turn = store
        .begin_create("cursor-invalid", cursor_binding(), &current)
        .unwrap();
    for change in 0..8 {
        let mut request = cursor_command();
        match change {
            0 => request.args.push("--continue".into()),
            1 => request.args.extend(["--resume".into(), "injected".into()]),
            2 => request.args[4] = "other model".into(),
            3 => request.args[2] = "json".into(),
            4 => request.program = PathBuf::from("/another/program"),
            5 => request.stdin = Some(b"injected".to_vec().into()),
            6 => request.cwd = PathBuf::from("relative"),
            7 => request.args[6] = "agent".into(),
            _ => unreachable!(),
        }
        let before = request.args.clone();
        assert!(
            turn.configure_cursor_request(&mut request, &current)
                .is_err()
        );
        assert_eq!(request.args, before);
    }
    let different = original(Duration::from_secs(20));
    assert_eq!(
        kind(turn.configure_cursor_request(&mut cursor_command(), &different)),
        ConversationErrorKind::Request
    );
    assert_eq!(
        kind(turn.observe_cursor_session("", &current)),
        ConversationErrorKind::InvalidInput
    );
    assert_eq!(
        kind(turn.observe_cursor_session("later-valid", &current)),
        ConversationErrorKind::NativeSession
    );
    assert_eq!(
        kind(turn.commit(&current)),
        ConversationErrorKind::NativeSession
    );
    drop(turn);
    let mut turn = store
        .begin_create("cursor-mismatch", cursor_binding(), &current)
        .unwrap();
    turn.observe_cursor_session("expected-native", &current)
        .unwrap();
    turn.commit(&current).unwrap();
    drop(turn);
    let mut continued = store
        .begin_turn("cursor-mismatch", &cursor_binding(), &current)
        .unwrap();
    assert_eq!(
        kind(continued.observe_cursor_session("different-native", &current)),
        ConversationErrorKind::NativeSession
    );
    assert_eq!(
        kind(continued.observe_cursor_session("expected-native", &current)),
        ConversationErrorKind::NativeSession
    );
    assert_eq!(
        kind(continued.commit(&current)),
        ConversationErrorKind::NativeSession
    );
    drop(continued);
    let mut valid = store
        .begin_turn("cursor-mismatch", &cursor_binding(), &current)
        .unwrap();
    let mut request = cursor_command();
    valid
        .configure_cursor_request(&mut request, &current)
        .unwrap();
    assert_eq!(request.args[8], "expected-native");
    current.cancellation().cancel();
    assert_eq!(
        kind(valid.observe_cursor_session("expected-native", &current)),
        ConversationErrorKind::Request
    );
}
#[test]
fn cursor_engine_cannot_enter_any_kiro_api_and_other_engines_cannot_enter_cursor() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    let metadata = cursor_binding();
    assert_eq!(
        kind(store.begin_kiro_delete("absent", kiro_target(&metadata), &current)),
        ConversationErrorKind::BindingChanged
    );
    let mut turn = store
        .begin_create("cursor-isolated", metadata.clone(), &current)
        .unwrap();
    assert_eq!(
        kind(turn.configure_kiro_session(&metadata.program, &std::env::temp_dir(), &current)),
        ConversationErrorKind::BindingChanged
    );
    assert_eq!(
        kind(turn.configure_kiro_delete(&metadata.program, &current)),
        ConversationErrorKind::BindingChanged
    );
    assert_eq!(
        kind(turn.observe_kiro_session(
            &agent_client_protocol::schema::v1::SessionId::new("kiro"),
            &current
        )),
        ConversationErrorKind::BindingChanged
    );
    assert_eq!(
        kind(turn.delete_kiro_record(&current)),
        ConversationErrorKind::NativeSession
    );
    assert_eq!(
        kind(turn.configure_claude_request(&mut claude_command(), &current)),
        ConversationErrorKind::BindingChanged
    );
    assert_eq!(
        kind(turn.observe_claude_session("claude", &current)),
        ConversationErrorKind::BindingChanged
    );
    assert_eq!(
        kind(turn.configure_codex_request(&mut command(), &current)),
        ConversationErrorKind::BindingChanged
    );
    assert_eq!(
        kind(turn.observe_thread_started("codex", &current)),
        ConversationErrorKind::BindingChanged
    );
    drop(turn);
    for (engine, provider) in [
        (ConversationEngine::Codex, "codex"),
        (ConversationEngine::Claude, "claude"),
        (ConversationEngine::Qwen, "qwen"),
        (ConversationEngine::Copilot, "copilot"),
        (ConversationEngine::KiroV3, "kiro"),
    ] {
        let mut metadata = binding();
        metadata.engine = engine;
        metadata.provider = provider.into();
        let mut turn = store
            .begin_create("other-engine", metadata, &current)
            .unwrap();
        assert_eq!(
            kind(turn.configure_cursor_request(&mut cursor_command(), &current)),
            ConversationErrorKind::BindingChanged
        );
        assert_eq!(
            kind(turn.observe_cursor_session("cursor", &current)),
            ConversationErrorKind::BindingChanged
        );
        drop(turn);
    }
}
