use super::*;
use graphit_ai::cli_kimi::KimiSessionStart;
use std::path::Path;

fn kimi_binding() -> ConversationBinding {
    let mut metadata = binding();
    metadata.engine = ConversationEngine::Kimi;
    metadata.provider = "kimi".into();
    metadata
}
#[test]
fn kimi_create_and_cold_reopen_use_exact_private_session_binding() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let metadata = kimi_binding();
    let store = f.store("A", &current, limits());
    let mut turn = store
        .begin_create("kimi-selected", metadata.clone(), &current)
        .unwrap();
    assert!(matches!(
        turn.configure_kimi_session(
            &metadata.program,
            &metadata.model,
            Path::new("/project"),
            &current
        )
        .unwrap(),
        KimiSessionStart::Create
    ));
    assert_eq!(
        kind(turn.commit(&current)),
        ConversationErrorKind::NativeSession
    );
    turn.observe_kimi_session("private-native-kimi", &current)
        .unwrap();
    let record = serde_json::to_string(&turn.commit(&current).unwrap()).unwrap();
    assert!(!record.contains("private-native-kimi"));
    assert!(!record.contains("native_id"));
    assert!(record.contains("kimi"));
    drop(turn);
    drop(store);
    let next = original(Duration::from_secs(20));
    let reopened = f.store("A", &next, limits());
    let mut turn = reopened
        .begin_turn("kimi-selected", &metadata, &next)
        .unwrap();
    match turn
        .configure_kimi_session(
            &metadata.program,
            &metadata.model,
            Path::new("/project"),
            &next,
        )
        .unwrap()
    {
        KimiSessionStart::Resume(id) => assert!(id.as_str() == "private-native-kimi"),
        KimiSessionStart::Create => panic!("cold reopen must resume"),
    }
    assert_eq!(
        kind(turn.commit(&next)),
        ConversationErrorKind::NativeSession
    );
    // The exact native session response and correlated turn confirm continuation.
    turn.observe_kimi_session("private-native-kimi", &next)
        .unwrap();
    turn.commit(&next).unwrap();
    drop(turn);
    assert!(
        f.store("B", &next, limits())
            .list(&next)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn kimi_config_rejects_wrong_original_selection_and_sticky_native_mismatch() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let metadata = kimi_binding();
    let store = f.store("A", &current, limits());
    let mut turn = store
        .begin_create("kimi-selected", metadata.clone(), &current)
        .unwrap();
    let other = original(Duration::from_secs(20));
    assert_eq!(
        kind(turn.configure_kimi_session(
            &metadata.program,
            &metadata.model,
            Path::new("/project"),
            &other
        )),
        ConversationErrorKind::Request
    );
    assert!(
        turn.configure_kimi_session(
            Path::new("/other"),
            &metadata.model,
            Path::new("/project"),
            &current
        )
        .is_err()
    );
    assert!(
        turn.configure_kimi_session(&metadata.program, "other", Path::new("/project"), &current)
            .is_err()
    );
    assert!(
        turn.configure_kimi_session(
            &metadata.program,
            &metadata.model,
            Path::new("relative"),
            &current
        )
        .is_err()
    );
    turn.observe_kimi_session("private-native-kimi", &current)
        .unwrap();
    turn.commit(&current).unwrap();
    drop(turn);
    let mut resumed = store
        .begin_turn("kimi-selected", &metadata, &current)
        .unwrap();
    assert_eq!(
        kind(resumed.observe_kimi_session("different-native", &current)),
        ConversationErrorKind::NativeSession
    );
    assert_eq!(
        kind(resumed.observe_kimi_session("private-native-kimi", &current)),
        ConversationErrorKind::NativeSession
    );
    assert_eq!(
        kind(resumed.commit(&current)),
        ConversationErrorKind::NativeSession
    );
}
#[test]
fn kimi_changed_binding_denies_before_native_selector_is_available() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let metadata = kimi_binding();
    let store = f.store("A", &current, limits());
    let mut turn = store
        .begin_create("kimi-selected", metadata.clone(), &current)
        .unwrap();
    turn.observe_kimi_session("private-native-kimi", &current)
        .unwrap();
    turn.commit(&current).unwrap();
    drop(turn);
    for change in 0..6 {
        let mut changed = metadata.clone();
        match change {
            0 => changed.model.push_str("-changed"),
            1 => changed.program = PathBuf::from("/different/program"),
            2 => changed.project_id.push_str("-changed"),
            3 => changed.project_version.push_str("-changed"),
            4 => changed.provider = "claude".into(),
            5 => changed.engine = ConversationEngine::Claude,
            _ => unreachable!(),
        }
        assert_eq!(
            kind(store.begin_turn("kimi-selected", &changed, &current)),
            ConversationErrorKind::BindingChanged
        );
    }
}
