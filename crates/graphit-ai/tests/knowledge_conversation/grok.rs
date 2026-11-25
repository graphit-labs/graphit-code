use super::*;

fn grok_binding() -> ConversationBinding {
    let mut metadata = binding();
    metadata.engine = ConversationEngine::Grok;
    metadata.provider = "grok".into();
    metadata
}
fn grok_prompt_path() -> PathBuf {
    std::env::temp_dir().join("grok-private-prompt.json")
}
fn grok_command() -> Request {
    let mut request = command();
    request.args = vec![
        "--prompt-file".into(),
        grok_prompt_path().into_os_string(),
        "--output-format".into(),
        "streaming-json".into(),
        "--model".into(),
        "literal-model".into(),
        "--verbatim".into(),
        "--deny".into(),
        "*".into(),
    ];
    request.stdin = None;
    request
}
#[test]
fn grok_native_continuity_cold_reopen_keeps_exact_private_identity_and_request() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    let mut turn = store
        .begin_create("grok-selected", grok_binding(), &current)
        .unwrap();
    let mut request = grok_command();
    let args = request.args.clone();
    let environment = request.environment.clone();
    let cwd = request.cwd.clone();
    turn.configure_grok_request(&mut request, &grok_prompt_path(), &current)
        .unwrap();
    assert_eq!(request.args, args);
    assert_eq!(request.environment, environment);
    assert_eq!(request.cwd, cwd);
    assert!(request.stdin.is_none());
    assert_eq!(
        kind(turn.commit(&current)),
        ConversationErrorKind::NativeSession
    );
    turn.observe_grok_session("00000000-0000-4000-8000-000000000001", &current)
        .unwrap();
    let projection = turn.commit(&current).unwrap();
    assert_eq!(serde_json::to_value(projection).unwrap()["engine"], "grok");
    drop(turn);
    drop(store);
    let next = original(Duration::from_secs(20));
    let reopened = f.store("A", &next, limits());
    let public = serde_json::to_string(&reopened.read("grok-selected", &next).unwrap()).unwrap();
    assert!(!public.contains("00000000-0000-4000-8000-000000000001"));
    assert!(!public.contains("native_id"));
    assert!(
        f.store("B", &next, limits())
            .list(&next)
            .unwrap()
            .is_empty()
    );
    let mut turn = reopened
        .begin_turn("grok-selected", &grok_binding(), &next)
        .unwrap();
    let mut resumed = grok_command();
    turn.configure_grok_request(&mut resumed, &grok_prompt_path(), &next)
        .unwrap();
    assert_eq!(
        &resumed.args[9..11],
        [
            OsString::from("--resume"),
            OsString::from("00000000-0000-4000-8000-000000000001")
        ]
    );
    assert_eq!(resumed.args[1], grok_prompt_path().as_os_str());
    assert!(resumed.stdin.is_none());
    assert_eq!(resumed.environment, environment);
    assert_eq!(resumed.cwd, cwd);
    turn.observe_grok_session("00000000-0000-4000-8000-000000000001", &next)
        .unwrap();
    turn.commit(&next).unwrap();
}
#[test]
fn grok_continuity_rejects_invalid_requests_originals_and_sticky_mismatch() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    let mut turn = store
        .begin_create("grok-invalid", grok_binding(), &current)
        .unwrap();
    for change in 0..10 {
        let mut request = grok_command();
        match change {
            0 => request.args.push("--continue".into()),
            1 => request.args.extend(["--resume".into(), "injected".into()]),
            2 => request.args[5] = "other model".into(),
            3 => request.args[3] = "json".into(),
            4 => request.program = PathBuf::from("/another/program"),
            5 => request.stdin = Some(b"injected".to_vec().into()),
            6 => request.cwd = PathBuf::from("relative"),
            7 => request.args[8] = "not-all".into(),
            8 => request.args[1] = std::env::temp_dir().join("foreign-prompt").into_os_string(),
            9 => request.args[6] = "--fork-session".into(),
            _ => unreachable!(),
        }
        let before = request.args.clone();
        assert!(
            turn.configure_grok_request(&mut request, &grok_prompt_path(), &current)
                .is_err()
        );
        assert_eq!(request.args, before);
    }
    let different = original(Duration::from_secs(20));
    assert_eq!(
        kind(turn.configure_grok_request(&mut grok_command(), &grok_prompt_path(), &different)),
        ConversationErrorKind::Request
    );
    assert_eq!(
        kind(turn.observe_grok_session("", &current)),
        ConversationErrorKind::InvalidInput
    );
    assert_eq!(
        kind(turn.observe_grok_session("00000000-0000-4000-8000-000000000001", &current)),
        ConversationErrorKind::NativeSession
    );
    assert_eq!(
        kind(turn.commit(&current)),
        ConversationErrorKind::NativeSession
    );
    drop(turn);
    for (index, invalid) in ["latest", "1", "00000000000040008000000000000001"]
        .into_iter()
        .enumerate()
    {
        let mut invalid_turn = store
            .begin_create(
                &format!("grok-invalid-id-{index}"),
                grok_binding(),
                &current,
            )
            .unwrap();
        assert_eq!(
            kind(invalid_turn.observe_grok_session(invalid, &current)),
            ConversationErrorKind::InvalidInput
        );
        assert_eq!(
            kind(invalid_turn.commit(&current)),
            ConversationErrorKind::NativeSession
        );
        drop(invalid_turn);
    }
    let mut turn = store
        .begin_create("grok-mismatch", grok_binding(), &current)
        .unwrap();
    turn.observe_grok_session("00000000-0000-4000-8000-000000000001", &current)
        .unwrap();
    turn.commit(&current).unwrap();
    drop(turn);
    let mut continued = store
        .begin_turn("grok-mismatch", &grok_binding(), &current)
        .unwrap();
    assert_eq!(
        kind(continued.observe_grok_session("00000000-0000-4000-8000-000000000002", &current)),
        ConversationErrorKind::NativeSession
    );
    assert_eq!(
        kind(continued.observe_grok_session("00000000-0000-4000-8000-000000000001", &current)),
        ConversationErrorKind::NativeSession
    );
    assert_eq!(
        kind(continued.commit(&current)),
        ConversationErrorKind::NativeSession
    );
    drop(continued);
    let mut valid = store
        .begin_turn("grok-mismatch", &grok_binding(), &current)
        .unwrap();
    let mut request = grok_command();
    valid
        .configure_grok_request(&mut request, &grok_prompt_path(), &current)
        .unwrap();
    assert_eq!(request.args[10], "00000000-0000-4000-8000-000000000001");
    current.cancellation().cancel();
    assert_eq!(
        kind(valid.observe_grok_session("00000000-0000-4000-8000-000000000001", &current)),
        ConversationErrorKind::Request
    );
}
#[test]
fn grok_engine_cannot_enter_any_kiro_api_and_other_engines_cannot_enter_grok() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    let metadata = grok_binding();
    assert_eq!(
        kind(store.begin_kiro_delete("absent", kiro_target(&metadata), &current)),
        ConversationErrorKind::BindingChanged
    );
    let mut turn = store
        .begin_create("grok-isolated", metadata.clone(), &current)
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
        (ConversationEngine::Cursor, "cursor"),
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
            kind(turn.configure_grok_request(&mut grok_command(), &grok_prompt_path(), &current)),
            ConversationErrorKind::BindingChanged
        );
        assert_eq!(
            kind(turn.observe_grok_session("grok", &current)),
            ConversationErrorKind::BindingChanged
        );
        drop(turn);
    }
}
