#![cfg(target_os = "linux")]
use graphit_ai::knowledge_conversation::*;
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CapturedPublicationScope, CopyLimits, ScopeLimits, UserIdentity,
    },
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::{Cancellation, FileLock},
    installation_identity::InstallationId,
    native_document_file::NativeDocumentFileLimits,
    project_paths::InstallationPaths,
};
use graphit_process::{Limits, Request};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    num::NonZeroUsize,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
fn positive(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}
fn original(duration: Duration) -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + duration,
        Cancellation::default(),
    ))
}
fn limits() -> ConversationLimits {
    let copy = CopyLimits::new(128 * 1024, 128, 16 * 1024, 8).unwrap();
    ConversationLimits {
        artifacts: ArtifactLimits::new(256 * 1024, 4096, copy).unwrap(),
        native: NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap(),
        record_bytes: positive(65536),
        field_bytes: positive(4096),
        sources: positive(128),
        list_records: positive(128),
    }
}
struct Fixture {
    _temp: tempfile::TempDir,
    installation: InstallationPaths,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let installation = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("global")),
        )
        .unwrap();
        std::fs::create_dir_all(installation.root()).unwrap();
        std::fs::set_permissions(installation.root(), std::fs::Permissions::from_mode(0o700))
            .unwrap();
        Self {
            _temp: temp,
            installation,
        }
    }
    fn scope(&self, subject: &str, current: &Arc<SnapshotContext>) -> CapturedPublicationScope {
        CapturedPublicationScope::user(
            &self.installation,
            InstallationId::new("conversation-fixture").unwrap(),
            UserIdentity::new("https://issuer.example".into(), subject.into()).unwrap(),
            ScopeLimits::new(1, 16384, 1024).unwrap(),
            current,
        )
        .unwrap()
    }
    fn store(
        &self,
        subject: &str,
        current: &Arc<SnapshotContext>,
        bounds: ConversationLimits,
    ) -> KnowledgeConversationStore {
        KnowledgeConversationStore::capture(
            self.scope(subject, current),
            Arc::clone(current),
            bounds,
        )
        .unwrap()
    }
    fn record_path(&self, subject: &str, id: &str, current: &Arc<SnapshotContext>) -> PathBuf {
        let digest = Sha256::digest(id.as_bytes());
        self.scope(subject, current)
            .user_root()
            .unwrap()
            .join(format!(
                "graphit-ai-conversation-{}.json",
                digest
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            ))
    }
}
fn kiro_target(metadata: &ConversationBinding) -> KiroConversationTarget<'_> {
    KiroConversationTarget {
        engine: metadata.engine,
        model: &metadata.model,
        program: &metadata.program,
        project_id: &metadata.project_id,
        project_version: &metadata.project_version,
    }
}
fn seed_kiro(
    store: &KnowledgeConversationStore,
    current: &Arc<SnapshotContext>,
) -> ConversationBinding {
    let mut metadata = binding();
    metadata.engine = ConversationEngine::KiroV3;
    metadata.provider = "kiro".into();
    let mut turn = store
        .begin_create("clear-selected", metadata.clone(), current)
        .unwrap();
    turn.observe_kiro_session(
        &agent_client_protocol::schema::v1::SessionId::new("private-clear-actual-id"),
        current,
    )
    .unwrap();
    turn.commit(current).unwrap();
    metadata
}
#[test]
fn kiro_delete_locator_reopens_private_id_and_removes_under_same_real_writer() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    let metadata = seed_kiro(&store, &current);
    drop(store);
    let reopened = f.store("A", &current, limits());
    let mut turn = reopened
        .begin_kiro_delete("clear-selected", kiro_target(&metadata), &current)
        .unwrap();
    let writer_path = f
        .scope("A", &current)
        .runtime_root()
        .join("artifact-store.lock");
    assert!(FileLock::is_locked(&writer_path).unwrap());
    let request = turn
        .configure_kiro_delete(&metadata.program, &current)
        .unwrap();
    assert_eq!(request.session_id.0.as_ref(), "private-clear-actual-id");
    assert!(request.meta.is_none());
    let projection =
        serde_json::to_string(&reopened.read("clear-selected", &current).unwrap()).unwrap();
    assert!(!projection.contains("private-clear-actual-id"));
    turn.delete_kiro_record(&current).unwrap();
    assert!(FileLock::is_locked(&writer_path).unwrap());
    assert!(!f.record_path("A", "clear-selected", &current).exists());
    drop(turn);
    assert!(!FileLock::is_locked(&writer_path).unwrap());
    assert_eq!(
        kind(reopened.read("clear-selected", &current)),
        ConversationErrorKind::Missing
    );
}
#[test]
fn kiro_delete_target_other_user_and_original_negatives_preserve_private_binding() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    let metadata = seed_kiro(&store, &current);
    for change in ["engine", "model", "program", "project", "version"] {
        let mut changed = metadata.clone();
        match change {
            "engine" => changed.engine = ConversationEngine::Claude,
            "model" => changed.model = "other".into(),
            "program" => changed.program = PathBuf::from("/other/program"),
            "project" => changed.project_id = "other".into(),
            "version" => changed.project_version = "other".into(),
            _ => unreachable!(),
        }
        assert_eq!(
            kind(store.begin_kiro_delete("clear-selected", kiro_target(&changed), &current)),
            ConversationErrorKind::BindingChanged
        );
    }
    assert_eq!(
        kind(f.store("B", &current, limits()).begin_kiro_delete(
            "clear-selected",
            kiro_target(&metadata),
            &current
        )),
        ConversationErrorKind::Missing
    );
    let foreign = original(Duration::from_secs(20));
    assert_eq!(
        kind(store.begin_kiro_delete("clear-selected", kiro_target(&metadata), &foreign)),
        ConversationErrorKind::Request
    );
    let unbounded = Arc::new(SnapshotContext::service(Cancellation::default()));
    let service = f.store("A", &unbounded, limits());
    assert_eq!(
        kind(service.begin_kiro_delete("clear-selected", kiro_target(&metadata), &unbounded)),
        ConversationErrorKind::Request
    );
    let held = store
        .begin_kiro_delete("clear-selected", kiro_target(&metadata), &current)
        .unwrap();
    assert_eq!(
        kind(held.configure_kiro_delete(&metadata.program, &foreign)),
        ConversationErrorKind::Request
    );
    assert_eq!(
        kind(held.configure_kiro_delete(&PathBuf::from("/other/program"), &current)),
        ConversationErrorKind::BindingChanged
    );
    drop(held);
    assert!(store.read("clear-selected", &current).is_ok());
}
#[test]
fn kiro_delete_local_node_failure_preserves_actual_cause_and_owned_writer() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    let metadata = seed_kiro(&store, &current);
    let mut turn = store
        .begin_kiro_delete("clear-selected", kiro_target(&metadata), &current)
        .unwrap();
    let path = f.record_path("A", "clear-selected", &current);
    let saved = path.with_extension("saved");
    std::fs::rename(&path, &saved).unwrap();
    std::fs::create_dir(&path).unwrap();
    let error = turn.delete_kiro_record(&current).unwrap_err();
    assert!(error.cause().is_some());
    assert!(!format!("{error:?}").contains("private-clear-actual-id"));
    assert!(
        FileLock::is_locked(
            f.scope("A", &current)
                .runtime_root()
                .join("artifact-store.lock")
        )
        .unwrap()
    );
    std::fs::remove_dir(&path).unwrap();
    std::fs::rename(saved, path).unwrap();
    drop(turn);
    assert!(store.read("clear-selected", &current).is_ok());
}
fn binding() -> ConversationBinding {
    ConversationBinding {
        provider: "configured-provider".into(),
        engine: ConversationEngine::Codex,
        model: "literal-model".into(),
        program: std::env::current_exe().unwrap(),
        project_id: "selected-project".into(),
        project_version: "selected-version".into(),
        sources: vec![ConversationSource {
            slug: "Paris".into(),
            source_path: "docs/paris.md".into(),
            physical_digest: Some([1; 32]),
            compilation_digest: [2; 32],
            recipe: "markdown".into(),
            fallback_title: "Paris".into(),
            kind_on_absent: "reference".into(),
            max_summary_bytes: positive(4096),
        }],
    }
}
fn create(store: &KnowledgeConversationStore, id: &str, current: &Arc<SnapshotContext>) {
    let mut turn = store
        .begin_create(id, binding(), current)
        .unwrap_or_else(|e| {
            let cause = e
                .cause()
                .and_then(|c| c.downcast_ref::<std::io::Error>())
                .map(|e| (e.kind(), e.raw_os_error()));
            panic!("begin_create {:?} IO kind/code {:?}", e.kind(), cause);
        });
    turn.observe_thread_started("fixture-native-session", current)
        .unwrap();
    turn.commit(current).unwrap();
}
fn kind<T>(result: Result<T, ConversationError>) -> ConversationErrorKind {
    match result {
        Err(e) => e.kind(),
        Ok(_) => panic!("expected failure"),
    }
}
fn command() -> Request {
    Request {
        program: binding().program,
        args: ["exec", "--json", "--model", "literal-model", "-"]
            .into_iter()
            .map(OsString::from)
            .collect(),
        cwd: std::env::temp_dir(),
        environment: vec![(OsString::from("CAPTURED"), OsString::from("fixture"))],
        stdin: None,
        limits: Limits {
            stdin_bytes: 4096,
            stdout_bytes: 4096,
            stderr_bytes: 4096,
            termination_grace: Duration::from_millis(10),
            cleanup: Duration::from_secs(1),
        },
    }
}
#[test]
fn cold_reads_do_not_birth_and_reconstructed_original_is_rejected() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(10));
    let store = f.store("A", &current, limits());
    assert!(store.list(&current).unwrap().is_empty());
    assert_eq!(
        kind(store.read("absent", &current)),
        ConversationErrorKind::Missing
    );
    let different = Arc::new(SnapshotContext::new(
        current.required_deadline().unwrap(),
        current.cancellation().clone(),
    ));
    assert_eq!(kind(store.list(&different)), ConversationErrorKind::Request);
    assert!(!f.scope("A", &current).user_root().unwrap().exists());
}
#[test]
fn real_sdk_create_reopen_projection_and_owned_delete() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    create(&store, "../Graphit selector", &current);
    let record = f.record_path("A", "../Graphit selector", &current);
    assert_eq!(
        std::fs::metadata(&record).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let public =
        serde_json::to_string(&store.read("../Graphit selector", &current).unwrap()).unwrap();
    assert!(!public.contains("fixture-native-session"));
    assert!(!public.contains("source_path"));
    assert!(!public.contains("program"));
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    assert_eq!(saved["native_id"], "fixture-native-session");
    assert!(
        saved.get("query").is_none()
            && saved.get("answer").is_none()
            && saved.get("history").is_none()
    );
    let next = original(Duration::from_secs(20));
    let reopened = f.store("A", &next, limits());
    assert_eq!(reopened.list(&next).unwrap().len(), 1);
    assert!(
        f.store("B", &next, limits())
            .list(&next)
            .unwrap()
            .is_empty()
    );
    reopened.delete("../Graphit selector", &next).unwrap();
    assert!(!record.exists());
    assert!(reopened.list(&next).unwrap().is_empty());
}
#[test]
fn create_reserves_absence_before_child_and_guard_is_owned_send() {
    fn assert_send<T: Send>() {}
    assert_send::<ConversationTurn>();
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    let mut turn = store.begin_create("new", binding(), &current).unwrap();
    let lock = f
        .scope("A", &current)
        .runtime_root()
        .join("artifact-store.lock");
    assert!(FileLock::is_locked(&lock).unwrap());
    assert_eq!(
        kind(turn.commit(&current)),
        ConversationErrorKind::NativeSession
    );
    assert!(!f.record_path("A", "new", &current).exists());
    turn.observe_thread_started("fixture-native-session", &current)
        .unwrap();
    turn.commit(&current).unwrap();
    drop(turn);
    assert!(!FileLock::is_locked(lock).unwrap());
    assert_eq!(
        kind(store.begin_create("new", binding(), &current)),
        ConversationErrorKind::Conflict
    );
    let foreign = f.record_path("A", "unowned", &current);
    std::fs::write(&foreign, b"unowned").unwrap();
    assert_eq!(
        kind(store.begin_create("unowned", binding(), &current)),
        ConversationErrorKind::Conflict
    );
    assert_eq!(std::fs::read(foreign).unwrap(), b"unowned");
}
#[test]
fn explicit_binding_changes_fail_before_resume() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    create(&store, "selected", &current);
    let same = store.begin_turn("selected", &binding(), &current).unwrap();
    drop(same);
    let mut cases = Vec::new();
    let mut changed = binding();
    changed.engine = ConversationEngine::KiroV3;
    cases.push(changed);
    let mut changed = binding();
    changed.provider = "other".into();
    cases.push(changed);
    let mut changed = binding();
    changed.model = "other".into();
    cases.push(changed);
    let mut changed = binding();
    changed.program = std::env::temp_dir().join("other-program");
    cases.push(changed);
    let mut changed = binding();
    changed.project_version = "other".into();
    cases.push(changed);
    let mut changed = binding();
    changed.sources[0].compilation_digest = [3; 32];
    cases.push(changed);
    let mut changed = binding();
    changed.sources[0].physical_digest = None;
    cases.push(changed);
    let mut changed = binding();
    changed.sources[0].source_path = "other.md".into();
    cases.push(changed);
    let mut changed = binding();
    changed.sources[0].recipe = "other".into();
    cases.push(changed);
    let mut changed = binding();
    changed.sources[0].max_summary_bytes = positive(1);
    cases.push(changed);
    let mut changed = binding();
    changed.sources[0].slug = "Other".into();
    cases.push(changed);
    for metadata in cases {
        assert_eq!(
            kind(store.begin_turn("selected", &metadata, &current)),
            ConversationErrorKind::BindingChanged
        );
    }
}

#[test]
fn kiro_new_commit_reopen_load_preserves_private_binding_and_initial_model() {
    use graphit_ai::cli_kiro::KiroSessionStart;
    {
        let engine = ConversationEngine::KiroV3;
        let f = Fixture::new();
        let current = original(Duration::from_secs(20));
        let store = f.store("A", &current, limits());
        let mut metadata = binding();
        metadata.engine = engine;
        metadata.provider = "kiro".into();
        let cwd = f._temp.path().join("project");
        let mut turn = store
            .begin_create("selected", metadata.clone(), &current)
            .unwrap();
        let expected_meta = Some({
            serde_json::json!({"kiro":{"modelId":"literal-model"}})
                .as_object()
                .unwrap()
                .clone()
        });
        let KiroSessionStart::New(new) = turn
            .configure_kiro_session(&metadata.program, &cwd, &current)
            .unwrap()
        else {
            panic!("new turn must construct New");
        };
        assert_eq!(new.cwd, cwd);
        assert_eq!(new.meta, expected_meta);
        assert!(new.mcp_servers.is_empty() && new.additional_directories.is_empty());
        assert_eq!(
            kind(turn.configure_codex_request(&mut command(), &current)),
            ConversationErrorKind::BindingChanged
        );
        let id = agent_client_protocol::schema::v1::SessionId::new("private-kiro-session");
        turn.observe_kiro_session(&id, &current).unwrap();
        let public = serde_json::to_string(&turn.commit(&current).unwrap()).unwrap();
        assert!(!public.contains("private-kiro-session"));
        drop(turn);

        let next = original(Duration::from_secs(20));
        let reopened = f.store("A", &next, limits());
        let mut loaded = reopened.begin_turn("selected", &metadata, &next).unwrap();
        let KiroSessionStart::Load(load) = loaded
            .configure_kiro_session(&metadata.program, &cwd, &next)
            .unwrap()
        else {
            panic!("continuation must construct Load without fallback");
        };
        assert_eq!(load.session_id, id);
        assert_eq!(load.cwd, cwd);
        assert_eq!(load.meta, expected_meta);
        assert!(load.mcp_servers.is_empty() && load.additional_directories.is_empty());
        loaded.observe_kiro_session(&id, &next).unwrap();
        loaded.commit(&next).unwrap();
        drop(loaded);
        for changed in [
            ConversationBinding {
                engine: ConversationEngine::Codex,
                ..metadata.clone()
            },
            ConversationBinding {
                model: "changed".into(),
                ..metadata.clone()
            },
            ConversationBinding {
                program: cwd.join("other-program"),
                ..metadata
            },
        ] {
            assert_eq!(
                kind(reopened.begin_turn("selected", &changed, &next)),
                ConversationErrorKind::BindingChanged
            );
        }
        assert!(
            f.store("B", &next, limits())
                .list(&next)
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn kiro_session_mismatch_remains_failed_and_never_publishes_replacement() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    let mut metadata = binding();
    metadata.engine = ConversationEngine::KiroV3;
    let mut created = store
        .begin_create("selected", metadata.clone(), &current)
        .unwrap();
    let id = agent_client_protocol::schema::v1::SessionId::new("private-kiro-session");
    assert_eq!(
        kind(created.observe_thread_started("wrong-protocol", &current)),
        ConversationErrorKind::BindingChanged
    );
    created.observe_kiro_session(&id, &current).unwrap();
    created.commit(&current).unwrap();
    drop(created);
    let record = f.record_path("A", "selected", &current);
    let before = std::fs::read(&record).unwrap();
    let mut loaded = store.begin_turn("selected", &metadata, &current).unwrap();
    let mismatch = agent_client_protocol::schema::v1::SessionId::new("private-replacement");
    let error = loaded
        .observe_kiro_session(&mismatch, &current)
        .err()
        .unwrap();
    assert_eq!(error.kind(), ConversationErrorKind::NativeSession);
    assert!(!format!("{error:?} {error}").contains("private-replacement"));
    assert_eq!(
        kind(loaded.observe_kiro_session(&id, &current)),
        ConversationErrorKind::NativeSession
    );
    assert_eq!(
        kind(loaded.commit(&current)),
        ConversationErrorKind::NativeSession
    );
    assert_eq!(std::fs::read(&record).unwrap(), before);
    assert!(
        FileLock::is_locked(
            f.scope("A", &current)
                .runtime_root()
                .join("artifact-store.lock")
        )
        .unwrap()
    );
}

#[test]
fn kiro_builder_rejects_foreign_original_program_cwd_and_expired_context() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    let mut metadata = binding();
    metadata.engine = ConversationEngine::KiroV3;
    let mut turn = store
        .begin_create("new", metadata.clone(), &current)
        .unwrap();
    let cwd = f._temp.path();
    assert_eq!(
        kind(turn.configure_kiro_session(&cwd.join("different"), cwd, &current)),
        ConversationErrorKind::BindingChanged
    );
    assert_eq!(
        kind(turn.configure_kiro_session(
            &metadata.program,
            std::path::Path::new("relative"),
            &current
        )),
        ConversationErrorKind::InvalidInput
    );
    let long_cwd = PathBuf::from(format!("/{}", "x".repeat(limits().field_bytes.get())));
    assert_eq!(
        kind(turn.configure_kiro_session(&metadata.program, &long_cwd, &current)),
        ConversationErrorKind::Limit
    );
    let foreign = Arc::new(SnapshotContext::new(
        current.required_deadline().unwrap(),
        current.cancellation().clone(),
    ));
    assert_eq!(
        kind(turn.configure_kiro_session(&metadata.program, cwd, &foreign)),
        ConversationErrorKind::Request
    );
    let session = agent_client_protocol::schema::v1::SessionId::new("actual-session");
    assert_eq!(
        kind(turn.observe_kiro_session(&session, &foreign)),
        ConversationErrorKind::Request
    );
    let unpublished = f.record_path("A", "new", &current);
    current.cancellation().cancel();
    let error = turn
        .configure_kiro_session(&metadata.program, cwd, &current)
        .err()
        .unwrap();
    assert_eq!(error.kind(), ConversationErrorKind::Request);
    assert_eq!(
        error
            .cause()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .kind(),
        std::io::ErrorKind::Interrupted
    );
    assert!(!unpublished.exists());
    drop(turn);

    let live = original(Duration::from_secs(2));
    let timed_store = f.store("B", &live, limits());
    let timed = timed_store.begin_create("new", metadata, &live).unwrap();
    // The same live request must expire after acquiring its writer. Wait only
    // until that request's actual deadline, without padding for the fixture.
    std::thread::sleep(
        live.deadline()
            .unwrap()
            .saturating_duration_since(Instant::now()),
    );
    let error = timed
        .configure_kiro_session(&binding().program, cwd, &live)
        .err()
        .unwrap();
    assert_eq!(
        error
            .cause()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .kind(),
        std::io::ErrorKind::TimedOut
    );
}

#[test]
fn kiro_inference_builder_requires_finite_original_but_store_keeps_service_io() {
    let f = Fixture::new();
    let current = Arc::new(SnapshotContext::service(Cancellation::default()));
    let store = f.store("A", &current, limits());
    let mut metadata = binding();
    metadata.engine = ConversationEngine::KiroV3;
    let turn = store.begin_create("new", metadata, &current).unwrap();
    assert_eq!(
        kind(turn.configure_kiro_session(&binding().program, f._temp.path(), &current)),
        ConversationErrorKind::Request
    );
    drop(turn);
    let finite = original(Duration::from_secs(20));
    let codex_store = f.store("B", &finite, limits());
    let mut codex = codex_store.begin_create("new", binding(), &finite).unwrap();
    assert_eq!(
        kind(codex.configure_kiro_session(&binding().program, f._temp.path(), &finite)),
        ConversationErrorKind::BindingChanged
    );
    assert_eq!(
        kind(codex.observe_kiro_session(
            &agent_client_protocol::schema::v1::SessionId::new("kiro"),
            &finite
        )),
        ConversationErrorKind::BindingChanged
    );
}
#[test]
fn resume_preserves_captured_request_and_mismatch_is_sticky() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    create(&store, "selected", &current);
    let mut turn = store.begin_turn("selected", &binding(), &current).unwrap();
    let mut request = command();
    request.args.insert(1, OsString::from("--strict-config"));
    let environment = request.environment.clone();
    let cwd = request.cwd.clone();
    turn.configure_codex_request(&mut request, &current)
        .unwrap();
    assert_eq!(
        request.args,
        [
            "exec",
            "--strict-config",
            "--json",
            "--model",
            "literal-model",
            "resume",
            "fixture-native-session",
            "-"
        ]
        .map(OsString::from)
    );
    assert_eq!(request.environment, environment);
    assert_eq!(request.cwd, cwd);
    assert_eq!(request.limits.stdin_bytes, 4096);
    turn.observe_thread_started("fixture-native-session", &current)
        .unwrap();
    let error = turn
        .observe_thread_started("fixture-new-session", &current)
        .err()
        .unwrap();
    assert_eq!(error.kind(), ConversationErrorKind::NativeSession);
    assert!(!format!("{error:?} {error}").contains("fixture-new-session"));
    assert_eq!(
        kind(turn.commit(&current)),
        ConversationErrorKind::NativeSession
    );
    assert_eq!(
        kind(turn.observe_thread_started("fixture-native-session", &current)),
        ConversationErrorKind::NativeSession
    );
}
#[test]
fn malformed_record_and_replaced_namespace_preserve_causal_failure() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    create(&store, "selected", &current);
    let path = f.record_path("A", "selected", &current);
    let mut saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    saved["version"] = 2.into();
    std::fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(
        kind(store.read("selected", &current)),
        ConversationErrorKind::InvalidRecord
    );
    std::fs::write(&path, b"secret-marker-invalid-json").unwrap();
    let error = store.read("selected", &current).err().unwrap();
    assert_eq!(error.kind(), ConversationErrorKind::InvalidRecord);
    assert!(error.cause().is_some());
    assert!(!format!("{error:?} {error}").contains("secret-marker"));
    let user_root = f.scope("A", &current).user_root().unwrap().to_owned();
    std::fs::rename(&user_root, user_root.with_extension("retained")).unwrap();
    std::fs::create_dir(&user_root).unwrap();
    assert!(store.list(&current).is_err());
}
#[test]
fn bounds_and_original_cancel_prevent_known_effects() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let mut bounds = limits();
    bounds.record_bytes = positive(8);
    let store = f.store("A", &current, bounds);
    assert_eq!(
        kind(store.begin_create("selected", binding(), &current)),
        ConversationErrorKind::Limit
    );
    assert!(!f.scope("A", &current).runtime_root().exists());
    let store = f.store("A", &current, limits());
    current.cancellation().cancel();
    let error = store
        .begin_create("selected", binding(), &current)
        .err()
        .unwrap();
    assert_eq!(error.kind(), ConversationErrorKind::Request);
    let cause = error
        .cause()
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .unwrap();
    assert_eq!(cause.kind(), std::io::ErrorKind::Interrupted);
}
#[test]
fn user_writer_wait_obeys_actual_deadline_then_releases_after_guard_drop() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    let held = store.begin_create("first", binding(), &current).unwrap();
    let next = original(Duration::from_millis(250));
    let competing = f.store("A", &next, limits());
    let error = competing
        .begin_create("second", binding(), &next)
        .err()
        .unwrap();
    assert_eq!(error.kind(), ConversationErrorKind::Store);
    assert!(error.cause().is_some());
    assert!(
        FileLock::is_locked(
            f.scope("A", &current)
                .runtime_root()
                .join("artifact-store.lock")
        )
        .unwrap()
    );
    drop(held);
    let next = original(Duration::from_secs(20));
    let resumed = f.store("A", &next, limits());
    let released = resumed.begin_create("second", binding(), &next).unwrap();
    drop(released);
}

#[test]
fn installation_owner_and_foreign_installation_record_are_not_user_access() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let installation_scope = CapturedPublicationScope::installation(
        &f.installation,
        InstallationId::new("conversation-fixture").unwrap(),
        [f.installation.root().join("owned-artifact")],
        ScopeLimits::new(1, 16384, 1024).unwrap(),
        &current,
    )
    .unwrap();
    assert_eq!(
        kind(KnowledgeConversationStore::capture(
            installation_scope,
            Arc::clone(&current),
            limits()
        )),
        ConversationErrorKind::InvalidInput
    );
    let store = f.store("A", &current, limits());
    create(&store, "selected", &current);
    let foreign = CapturedPublicationScope::user(
        &f.installation,
        InstallationId::new("other-installation").unwrap(),
        UserIdentity::new("https://issuer.example".into(), "A".into()).unwrap(),
        ScopeLimits::new(1, 16384, 1024).unwrap(),
        &current,
    )
    .unwrap();
    let foreign =
        KnowledgeConversationStore::capture(foreign, Arc::clone(&current), limits()).unwrap();
    assert_eq!(
        kind(foreign.read("selected", &current)),
        ConversationErrorKind::Store
    );
    assert_eq!(
        store.read("selected", &current).unwrap().conversation_id,
        "selected"
    );
}
#[test]
fn canceled_commit_retains_actual_writer_without_persisting_or_renewing_original() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    let scope = f.scope("A", &current);
    let mut turn = store.begin_create("selected", binding(), &current).unwrap();
    turn.observe_thread_started("fixture-native-session", &current)
        .unwrap();
    current.cancellation().cancel();
    let error = turn.commit(&current).err().unwrap();
    assert_eq!(error.kind(), ConversationErrorKind::Request);
    assert_eq!(
        error
            .cause()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .kind(),
        std::io::ErrorKind::Interrupted
    );
    let lock = scope.runtime_root().join("artifact-store.lock");
    assert!(FileLock::is_locked(&lock).unwrap());
    assert!(
        !f.record_path("A", "selected", &original(Duration::from_secs(20)))
            .exists()
    );
    drop(turn);
    assert!(!FileLock::is_locked(lock).unwrap());
}

#[test]
fn service_original_preserves_cold_read_sdk_write_delete_and_cancel() {
    let f = Fixture::new();
    let current = Arc::new(SnapshotContext::service(Cancellation::default()));
    let store = f.store("A", &current, limits());
    assert!(store.list(&current).unwrap().is_empty());
    assert!(!f.scope("A", &current).user_root().unwrap().exists());
    create(&store, "selected", &current);
    let path = f.record_path("A", "selected", &current);
    assert!(path.is_file());
    let same = Arc::clone(&current);
    let reopened = f.store("A", &same, limits());
    assert_eq!(reopened.list(&same).unwrap().len(), 1);
    reopened.delete("selected", &same).unwrap();
    assert!(!path.exists());
    assert!(reopened.list(&same).unwrap().is_empty());
    assert_eq!(current.deadline(), None);
    let cancelled_path = f.record_path("A", "after-cancel", &current);
    current.cancellation().cancel();
    let failure = reopened
        .begin_create("after-cancel", binding(), &same)
        .err()
        .unwrap();
    assert_eq!(failure.kind(), ConversationErrorKind::Request);
    let cause = failure
        .cause()
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .unwrap();
    assert_eq!(cause.kind(), std::io::ErrorKind::Interrupted);
    assert!(!cancelled_path.exists());
}

fn claude_binding() -> ConversationBinding {
    let mut metadata = binding();
    metadata.engine = ConversationEngine::Claude;
    metadata.provider = "claude".into();
    metadata
}
fn claude_command() -> Request {
    let mut request = command();
    request.args = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--model",
        "literal-model",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    request.stdin = Some(b"Authorized documentary prompt".to_vec().into());
    request
}
#[test]
fn claude_native_continuity_cold_reopen_keeps_exact_private_identity_and_request() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    let mut turn = store
        .begin_create("claude-selected", claude_binding(), &current)
        .unwrap();
    let mut request = claude_command();
    let args = request.args.clone();
    let environment = request.environment.clone();
    let cwd = request.cwd.clone();
    turn.configure_claude_request(&mut request, &current)
        .unwrap();
    assert_eq!(request.args, args);
    assert_eq!(request.environment, environment);
    assert_eq!(request.cwd, cwd);
    assert!(request.stdin.is_some());
    assert_eq!(
        kind(turn.commit(&current)),
        ConversationErrorKind::NativeSession
    );
    turn.observe_claude_session("00000000-0000-4000-8000-000000000001", &current)
        .unwrap();
    let projection = turn.commit(&current).unwrap();
    assert_eq!(
        serde_json::to_value(projection).unwrap()["engine"],
        "claude"
    );
    drop(turn);
    drop(store);
    let next = original(Duration::from_secs(20));
    let reopened = f.store("A", &next, limits());
    let public = serde_json::to_string(&reopened.read("claude-selected", &next).unwrap()).unwrap();
    assert!(!public.contains("00000000-0000-4000-8000-000000000001"));
    assert!(!public.contains("native_id"));
    assert!(
        f.store("B", &next, limits())
            .list(&next)
            .unwrap()
            .is_empty()
    );
    let mut turn = reopened
        .begin_turn("claude-selected", &claude_binding(), &next)
        .unwrap();
    let mut resumed = claude_command();
    turn.configure_claude_request(&mut resumed, &next).unwrap();
    assert_eq!(
        &resumed.args[7..],
        [
            OsString::from("--resume"),
            OsString::from("00000000-0000-4000-8000-000000000001")
        ]
    );
    assert_eq!(resumed.environment, environment);
    assert_eq!(resumed.cwd, cwd);
    turn.observe_claude_session("00000000-0000-4000-8000-000000000001", &next)
        .unwrap();
    turn.commit(&next).unwrap();
}
#[test]
fn claude_continuity_rejects_invalid_requests_originals_and_sticky_mismatch() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    let mut turn = store
        .begin_create("claude-invalid", claude_binding(), &current)
        .unwrap();
    for change in 0..8 {
        let mut request = claude_command();
        match change {
            0 => request.args.push("--continue".into()),
            1 => request.args.extend(["--resume".into(), "injected".into()]),
            2 => request.args[6] = "other model".into(),
            3 => request.args[2] = "json".into(),
            4 => request.program = PathBuf::from("/another/program"),
            5 => request.stdin = None,
            6 => request.cwd = PathBuf::from("relative"),
            7 => request.args.push("--no-session-persistence".into()),
            _ => unreachable!(),
        }
        let before = request.args.clone();
        assert!(
            turn.configure_claude_request(&mut request, &current)
                .is_err()
        );
        assert_eq!(request.args, before);
    }
    let different = original(Duration::from_secs(20));
    assert_eq!(
        kind(turn.configure_claude_request(&mut claude_command(), &different)),
        ConversationErrorKind::Request
    );
    assert_eq!(
        kind(turn.observe_claude_session("", &current)),
        ConversationErrorKind::InvalidInput
    );
    assert_eq!(
        kind(turn.observe_claude_session("later-valid", &current)),
        ConversationErrorKind::NativeSession
    );
    assert_eq!(
        kind(turn.commit(&current)),
        ConversationErrorKind::NativeSession
    );
    drop(turn);
    let mut turn = store
        .begin_create("claude-mismatch", claude_binding(), &current)
        .unwrap();
    turn.observe_claude_session("expected-native", &current)
        .unwrap();
    turn.commit(&current).unwrap();
    drop(turn);
    let mut continued = store
        .begin_turn("claude-mismatch", &claude_binding(), &current)
        .unwrap();
    assert_eq!(
        kind(continued.observe_claude_session("different-native", &current)),
        ConversationErrorKind::NativeSession
    );
    assert_eq!(
        kind(continued.observe_claude_session("expected-native", &current)),
        ConversationErrorKind::NativeSession
    );
    assert_eq!(
        kind(continued.commit(&current)),
        ConversationErrorKind::NativeSession
    );
    drop(continued);
    let mut valid = store
        .begin_turn("claude-mismatch", &claude_binding(), &current)
        .unwrap();
    let mut request = claude_command();
    valid
        .configure_claude_request(&mut request, &current)
        .unwrap();
    assert_eq!(request.args[8], "expected-native");
    current.cancellation().cancel();
    assert_eq!(
        kind(valid.observe_claude_session("expected-native", &current)),
        ConversationErrorKind::Request
    );
}
#[test]
fn claude_engine_cannot_enter_any_kiro_api_and_other_engines_cannot_enter_claude() {
    let f = Fixture::new();
    let current = original(Duration::from_secs(20));
    let store = f.store("A", &current, limits());
    let metadata = claude_binding();
    assert_eq!(
        kind(store.begin_kiro_delete("absent", kiro_target(&metadata), &current)),
        ConversationErrorKind::BindingChanged
    );
    let mut turn = store
        .begin_create("claude-isolated", metadata.clone(), &current)
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
    drop(turn);
    for engine in [ConversationEngine::Codex, ConversationEngine::KiroV3] {
        let mut metadata = binding();
        metadata.engine = engine;
        let mut turn = store
            .begin_create("other-engine", metadata, &current)
            .unwrap();
        assert_eq!(
            kind(turn.configure_claude_request(&mut claude_command(), &current)),
            ConversationErrorKind::BindingChanged
        );
        assert_eq!(
            kind(turn.observe_claude_session("claude", &current)),
            ConversationErrorKind::BindingChanged
        );
        drop(turn);
    }
}

#[path = "knowledge_conversation/antigravity.rs"]
mod antigravity;
#[path = "knowledge_conversation/copilot.rs"]
mod copilot;
#[path = "knowledge_conversation/cursor.rs"]
mod cursor;
#[path = "knowledge_conversation/grok.rs"]
mod grok;
#[path = "knowledge_conversation/kimi.rs"]
mod kimi;
#[path = "knowledge_conversation/opencode.rs"]
mod opencode;
#[path = "knowledge_conversation/qwen.rs"]
mod qwen;
