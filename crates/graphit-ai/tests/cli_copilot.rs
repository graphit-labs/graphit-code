use graphit_ai::{cli_codex_wire::CodexWireLimits, cli_copilot::*};
use graphit_auth::{
    AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, Permission, ProjectCapture,
    ProjectReadModule, ProjectRecordAuthority, RoleIssuer, ScopeCapture, ScopeInput,
    ServiceAdmissionLimits, ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy,
    profile::ProfileLimits, store::AuthStore,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeBindingRecord, ScopeLimits},
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationId,
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
    version_context::{Checkout, VersionContext},
};
use graphit_hub::{NativeBinding, native::ContentLimits, policy::RequestContext};
use graphit_process::{
    Cleanup, HostLimits, HostReadiness, HostShutdown, Limits, PipeEnd, ProcessHost, Request,
};
use serde_json::value::RawValue;
use std::{
    io,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
struct Fixture {
    issuer: RoleIssuer,
    runtime: Arc<RuntimeKeyOwner>,
    realm: CapturedPublicationScope,
    realm_record: ScopeBindingRecord,
    project: CapturedPublicationScope,
    record: ScopeBindingRecord,
    version: VersionContext,
    root: PathBuf,
    _dir: tempfile::TempDir,
}
impl Fixture {
    fn new(request: &RequestContext<'_>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let home = base.join("home");
        let root = base.join("project");
        std::fs::create_dir(&home).unwrap();
        std::fs::create_dir(&root).unwrap();
        let paths = InstallationPaths::resolve(Brand::default(), &root, Some(&home), None).unwrap();
        let private = installation_bootstrap::prepare(
            &paths,
            BootstrapLimits::new(65536, 256).unwrap(),
            request.snapshot(),
        )
        .into_installation()
        .unwrap();
        let runtime = Arc::new(
            RuntimeKeyOwner::start(
                private,
                RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
                request.snapshot(),
            )
            .into_owner()
            .ok()
            .unwrap(),
        );
        let project_paths = ProjectPaths::at(&root, &paths).unwrap();
        let sl = ScopeLimits::new(16, 4096, 128).unwrap();
        let realm = CapturedPublicationScope::installation(
            &paths,
            InstallationId::new("fixture-installation").unwrap(),
            [paths.root().to_path_buf(), root.clone()],
            sl,
            request.snapshot(),
        )
        .unwrap();
        let realm_record = realm.binding().unwrap();
        let project = CapturedPublicationScope::project(
            &project_paths,
            "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
            sl,
            request.snapshot(),
        )
        .unwrap();
        let record = project.binding().unwrap();
        let version =
            VersionContext::select(&project_paths, Checkout::NonGit, "development", None, "")
                .unwrap();
        let store = Arc::new(
            AuthStore::new(
                AuthDocumentStore::capture(
                    realm.clone(),
                    AuthDocumentLimits::new(65536, 4096, 128, 65536).unwrap(),
                    request.snapshot(),
                )
                .unwrap(),
                ProfileLimits {
                    bytes: 65536,
                    nodes: 4096,
                    scalar_bytes: 4096,
                    collection_entries: 128,
                    providers: 16,
                    profiles: 16,
                },
                request,
            )
            .unwrap(),
        );
        let ordinary = [
            Permission::Read(graphit_core::module_settings::Module::Knowledge),
            Permission::Read(graphit_core::module_settings::Module::Ast),
        ];
        let source = || SourceAuthorityPolicy {
            ordinary: &ordinary,
            administrative: &[],
            sessions: AdminSessionPolicy {
                lifetime: Duration::from_secs(60),
                maximum: Duration::from_secs(60),
            },
        };
        let policy = IssuerAuthorityPolicy::new(
            source(),
            source(),
            IssuerLimits {
                max_permissions: 64,
                max_scope_bytes: 65536,
                max_parent_bearer_bytes: 1024,
            },
            ServiceAuthorityInput {
                rules: &[],
                limits: ServiceAdmissionLimits {
                    max_rules: 16,
                    max_binding_bytes: 8192,
                },
                native_refresh_margin: Duration::ZERO,
            },
        )
        .unwrap();
        let issuer = RoleIssuer::new(
            ScopeCapture {
                live: &realm,
                record: &realm_record,
            },
            runtime.clone(),
            store,
            policy,
            TtlPolicy {
                ttl: Duration::from_secs(60),
                maximum: Duration::from_secs(60),
                refresh_margin: Duration::from_secs(1),
            },
            request,
        )
        .unwrap();
        Self {
            issuer,
            runtime,
            realm,
            realm_record,
            project,
            record,
            version,
            root,
            _dir: dir,
        }
    }
    fn scope(&self) -> ScopeInput<'_> {
        ScopeInput {
            installation: ScopeCapture {
                live: &self.realm,
                record: &self.realm_record,
            },
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &self.project,
                    record: &self.record,
                },
                version: &self.version,
            }),
            providers: &[],
            binding: NativeBinding::Codex,
            global_version: "",
        }
    }
    fn command(&self, mode: &str) -> Request {
        let mut environment = vec![
            ("COPILOT_MODE".into(), mode.into()),
            (
                "COPILOT_SESSION_STORE".into(),
                self.root.join("native-session").into_os_string(),
            ),
        ];
        environment.extend(
            std::env::vars_os().filter(|(k, _)| k == "LD_LIBRARY_PATH" || k == "SYSTEMROOT"),
        );
        Request {
            program: env!("CARGO_BIN_EXE_ai-copilot-fixture").into(),
            args: Vec::new(),
            cwd: self.root.clone(),
            environment,
            stdin: None,
            limits: Limits {
                stdin_bytes: 65536,
                stdout_bytes: 65536,
                stderr_bytes: 65536,
                termination_grace: Duration::from_millis(20),
                cleanup: Duration::from_secs(2),
            },
        }
    }
}
fn request(signal: &Cancellation) -> RequestContext<'_> {
    request_until(signal, Instant::now() + Duration::from_secs(15))
}
fn request_until(signal: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 64,
            max_nodes: 4096,
            max_scalar_bytes: 65536,
            max_collection_entries: 128,
        },
        signal,
        deadline,
    )
    .unwrap()
}
fn limits() -> CopilotLimits {
    CopilotLimits {
        wire: CodexWireLimits {
            frame_bytes: 8192,
            depth: 32,
            nodes: 4096,
            fields: 1024,
            array_items: 128,
            string_bytes: 8192,
            decoded_string_bytes: 16384,
            id_bytes: 256,
        },
        input_bytes: 65536,
        output_bytes: 65536,
        prompt_text_bytes: 65536,
        response_text_bytes: 8192,
    }
}
#[derive(Default)]
struct Sink {
    bound: Vec<(String, bool)>,
    events: Vec<String>,
    fail: bool,
    cancel: Option<Cancellation>,
    blocked: Option<(Arc<tokio::sync::Notify>, Arc<std::sync::atomic::AtomicBool>)>,
    expected: Option<Arc<graphit_core::file_io::SnapshotContext>>,
}
impl CopilotSink for Sink {
    fn recheck(&self, original: &graphit_core::file_io::SnapshotContext) -> io::Result<()> {
        if let Some(expected) = &self.expected {
            assert!(std::ptr::eq(original, expected.as_ref()));
        }
        original.check()
    }
    fn session_bound(
        &mut self,
        id: &str,
        resumed: bool,
        _: &graphit_core::file_io::SnapshotContext,
    ) -> io::Result<()> {
        self.bound.push((id.into(), resumed));
        Ok(())
    }
    async fn push<'a>(
        &'a mut self,
        event: &'a RawValue,
        original: &'a graphit_core::file_io::SnapshotContext,
    ) -> io::Result<()> {
        if let Some(expected) = &self.expected {
            assert!(std::ptr::eq(original, expected.as_ref()));
        }
        if let Some((entered, dropped)) = &self.blocked {
            struct Guard(Arc<std::sync::atomic::AtomicBool>);
            impl Drop for Guard {
                fn drop(&mut self) {
                    self.0.store(true, std::sync::atomic::Ordering::Release);
                }
            }
            let _guard = Guard(Arc::clone(dropped));
            entered.notify_one();
            return std::future::pending().await;
        }
        self.events.push(event.get().into());
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
        if self.fail {
            Err(io::ErrorKind::BrokenPipe.into())
        } else {
            Ok(())
        }
    }
}
async fn run(
    f: &Fixture,
    ctx: &RequestContext<'_>,
    host: &ProcessHost,
    mode: &str,
    session: CopilotSessionStart,
    policy: CopilotLimits,
    sink: &mut Sink,
) -> Result<CopilotReceipt, CopilotFailure> {
    let key = f.runtime.check(ctx.snapshot()).unwrap();
    let raw = std::str::from_utf8(key.secret(ctx.snapshot()).unwrap()).unwrap();
    let caller = f.issuer.authenticate_runtime(raw, ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&f.issuer);
    let admission = authority
        .admit_authenticated_read(&caller, ctx, f.scope(), ProjectReadModule::Knowledge)
        .unwrap();
    run_copilot(
        ctx.snapshot(),
        &admission,
        host,
        CopilotRequest {
            command: f.command(mode),
            session,
            model: "fixture-model".into(),
            prompt: "question 🦀\nprivate-prompt".into(),
        },
        policy,
        sink,
    )
    .await
}
async fn host() -> ProcessHost {
    let host =
        ProcessHost::install(&Cancellation::default(), HostLimits::new(1, 1).unwrap()).unwrap();
    assert!(matches!(
        host.ready(Instant::now() + Duration::from_secs(3)).await,
        HostReadiness::Ready
    ));
    host
}
async fn finish(host: ProcessHost, result: &mut Result<CopilotReceipt, CopilotFailure>) {
    let receipt = match result {
        Ok(r) => r,
        Err(e) => e.receipt_mut(),
    };
    if let Some(process) = receipt.process_mut() {
        let remaining = match process {
            Ok(o) => o.remaining.as_mut(),
            Err(e) => e.remaining.as_mut(),
        };
        if let Some(remaining) = remaining {
            remaining
                .repair(Instant::now() + Duration::from_secs(3))
                .await;
        }
    }
    assert!(
        matches!(host.shutdown(Instant::now()+Duration::from_secs(3)).await,HostShutdown::Joined(report) if report.failure().is_none())
    );
}
#[tokio::test]
async fn create_and_cold_resume_confirm_exact_native_result_and_clean_process() {
    let signal = Cancellation::default();
    let ctx = request(&signal);
    let f = Fixture::new(&ctx);
    let h = host().await;
    let mut sink = Sink::default();
    let mut result = run(
        &f,
        &ctx,
        &h,
        "answer",
        CopilotSessionStart::Create,
        limits(),
        &mut sink,
    )
    .await;
    let receipt = result.as_ref().unwrap();
    assert_eq!(
        receipt.eligible_text(),
        Some("Native Copilot documentary answer 🦀\n")
    );
    let id = receipt.session().unwrap().to_owned();
    assert_eq!(sink.bound, vec![(id.clone(), false)]);
    assert!(
        sink.events
            .iter()
            .any(|e| e.contains("tool.execution_start"))
    );
    assert!(
        !sink
            .events
            .iter()
            .any(|e| e.contains("sessionId") || e.contains("\"type\":\"result\""))
    );
    let process = receipt.process().unwrap().as_ref().unwrap();
    assert!(process.status.success());
    assert_eq!(process.cleanup, Cleanup::Complete);
    assert_eq!(process.pipes.stdout, PipeEnd::Eof);
    assert_eq!(process.pipes.stderr, PipeEnd::Eof);
    finish(h, &mut result).await;
    let h = host().await;
    let mut sink = Sink::default();
    let mut result = run(
        &f,
        &ctx,
        &h,
        "answer",
        CopilotSessionStart::Resume(id.clone()),
        limits(),
        &mut sink,
    )
    .await;
    assert_eq!(result.as_ref().unwrap().session(), Some(id.as_str()));
    assert_eq!(sink.bound, vec![(id, true)]);
    finish(h, &mut result).await;
}
#[tokio::test]
async fn errors_missing_mismatched_terminal_and_post_result_save_failure_never_deliver_answer() {
    let signal = Cancellation::default();
    let ctx = request(&signal);
    let f = Fixture::new(&ctx);
    for (mode, expected) in [
        ("startup_error", CopilotReason::NonzeroExit),
        ("native_error", CopilotReason::Terminal),
        ("result_error", CopilotReason::Terminal),
        ("no_result", CopilotReason::Terminal),
        ("empty_text", CopilotReason::MissingText),
        ("malformed", CopilotReason::Wire),
        ("duplicate_keys", CopilotReason::Wire),
        ("truncated", CopilotReason::Wire),
        ("duplicate_result", CopilotReason::Protocol),
        ("nonzero_after_result", CopilotReason::NonzeroExit),
    ] {
        let h = host().await;
        let mut sink = Sink::default();
        let mut result = run(
            &f,
            &ctx,
            &h,
            mode,
            CopilotSessionStart::Create,
            limits(),
            &mut sink,
        )
        .await;
        let error = result.as_ref().unwrap_err();
        assert_eq!(error.reason(), expected, "{mode}");
        assert!(error.receipt().eligible_text().is_none());
        assert!(!format!("{error:?}").contains("canary"));
        assert!(error.receipt().process().is_some());
        finish(h, &mut result).await;
    }
    let id = std::fs::read_to_string(f.root.join("native-session")).unwrap();
    for mode in ["resume_mismatch", "resume_error"] {
        let h = host().await;
        let mut sink = Sink::default();
        let mut result = run(
            &f,
            &ctx,
            &h,
            mode,
            CopilotSessionStart::Resume(id.clone()),
            limits(),
            &mut sink,
        )
        .await;
        assert!(result.is_err());
        assert!(sink.bound.is_empty());
        finish(h, &mut result).await;
    }
    std::fs::remove_file(f.root.join("native-session")).unwrap();
    let h = host().await;
    let mut sink = Sink::default();
    let mut result = run(
        &f,
        &ctx,
        &h,
        "answer",
        CopilotSessionStart::Resume(id),
        limits(),
        &mut sink,
    )
    .await;
    assert!(result.is_err());
    assert!(sink.bound.is_empty());
    assert!(!f.root.join("native-session").exists());
    finish(h, &mut result).await;
}
#[tokio::test]
async fn fragmented_utf8_final_eof_and_transient_private_error_keep_real_answer() {
    let signal = Cancellation::default();
    let ctx = request(&signal);
    let f = Fixture::new(&ctx);
    for mode in ["fragmented", "final_fragment", "transient"] {
        let h = host().await;
        let mut sink = Sink::default();
        let mut result = run(
            &f,
            &ctx,
            &h,
            mode,
            CopilotSessionStart::Create,
            limits(),
            &mut sink,
        )
        .await;
        let receipt = result.as_ref().unwrap();
        assert!(receipt.eligible_text().is_some());
        if mode == "transient" {
            assert!(receipt.nonterminal_error().is_some());
            assert!(
                !sink
                    .events
                    .iter()
                    .any(|e| e.contains("private-native-error-canary"))
            );
        }
        finish(h, &mut result).await;
    }
}
#[tokio::test]
async fn output_frame_response_limits_and_sink_error_retain_cleanup() {
    let signal = Cancellation::default();
    let ctx = request(&signal);
    let f = Fixture::new(&ctx);
    for variant in 0..4 {
        let h = host().await;
        let mut policy = limits();
        let mut sink = Sink::default();
        match variant {
            0 => policy.output_bytes = 32,
            1 => policy.wire.frame_bytes = 32,
            2 => policy.response_text_bytes = 2,
            _ => sink.fail = true,
        };
        let mut result = run(
            &f,
            &ctx,
            &h,
            "answer",
            CopilotSessionStart::Create,
            policy,
            &mut sink,
        )
        .await;
        assert!(result.is_err());
        assert!(result.as_ref().unwrap_err().receipt().process().is_some());
        finish(h, &mut result).await;
    }
}
#[tokio::test]
async fn cancellation_during_native_output_joins_without_success() {
    let signal = Cancellation::default();
    let ctx = request(&signal);
    let f = Fixture::new(&ctx);
    let h = host().await;
    let mut sink = Sink {
        cancel: Some(signal.clone()),
        ..Default::default()
    };
    let mut result = run(
        &f,
        &ctx,
        &h,
        "answer",
        CopilotSessionStart::Create,
        limits(),
        &mut sink,
    )
    .await;
    assert!(result.is_err());
    assert!(
        result
            .as_ref()
            .unwrap_err()
            .receipt()
            .eligible_text()
            .is_none()
    );
    finish(h, &mut result).await;
}
#[tokio::test]
async fn invalid_resume_and_prompt_limits_fail_before_spawn() {
    let signal = Cancellation::default();
    let ctx = request(&signal);
    let f = Fixture::new(&ctx);
    let h = host().await;
    let mut sink = Sink::default();
    let mut result = run(
        &f,
        &ctx,
        &h,
        "answer",
        CopilotSessionStart::Resume("prefix-not-a-session".into()),
        limits(),
        &mut sink,
    )
    .await;
    assert_eq!(result.as_ref().unwrap_err().reason(), CopilotReason::Policy);
    assert!(result.as_ref().unwrap_err().receipt().process().is_none());
    finish(h, &mut result).await;
    let h = host().await;
    let mut policy = limits();
    policy.prompt_text_bytes = 2;
    let mut result = run(
        &f,
        &ctx,
        &h,
        "answer",
        CopilotSessionStart::Create,
        policy,
        &mut sink,
    )
    .await;
    assert!(result.as_ref().unwrap_err().receipt().process().is_none());
    finish(h, &mut result).await;
}

#[tokio::test]
async fn original_deadline_stops_native_start_and_output_waits_and_joins() {
    let signal = Cancellation::default();
    let setup = request(&signal);
    let f = Fixture::new(&setup);
    for mode in ["start_hold", "hold"] {
        let h = host().await;
        let ctx = request_until(&signal, Instant::now() + Duration::from_secs(1));
        let mut sink = Sink::default();
        let mut result = run(
            &f,
            &ctx,
            &h,
            mode,
            CopilotSessionStart::Create,
            limits(),
            &mut sink,
        )
        .await;
        let error = result.as_ref().unwrap_err();
        assert_eq!(
            error.reason(),
            CopilotReason::Context(io::ErrorKind::TimedOut)
        );
        assert!(error.receipt().process().is_some());
        assert!(error.receipt().eligible_text().is_none());
        assert!(sink.bound.is_empty());
        finish(h, &mut result).await;
    }
}
#[tokio::test]
async fn foreign_original_wrong_module_and_cancelled_original_reject_before_spawn() {
    for variant in 0..3 {
        let signal = Cancellation::default();
        let ctx = request(&signal);
        let f = Fixture::new(&ctx);
        let h = host().await;
        let key = f.runtime.check(ctx.snapshot()).unwrap();
        let raw = std::str::from_utf8(key.secret(ctx.snapshot()).unwrap()).unwrap();
        let caller = f.issuer.authenticate_runtime(raw, &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&f.issuer);
        let module = if variant == 1 {
            ProjectReadModule::Ast
        } else {
            ProjectReadModule::Knowledge
        };
        let admission = authority
            .admit_authenticated_read(&caller, &ctx, f.scope(), module)
            .unwrap();
        let other_signal = Cancellation::default();
        let other = request(&other_signal);
        let original = if variant == 0 {
            other.snapshot()
        } else {
            ctx.snapshot()
        };
        if variant == 2 {
            signal.cancel();
        }
        let mut sink = Sink::default();
        let mut result = run_copilot(
            original,
            &admission,
            &h,
            CopilotRequest {
                command: f.command("answer"),
                session: CopilotSessionStart::Create,
                model: "fixture-model".into(),
                prompt: "documentary question".into(),
            },
            limits(),
            &mut sink,
        )
        .await;
        let error = result.as_ref().unwrap_err();
        assert_eq!(
            error.reason(),
            if variant == 2 {
                CopilotReason::Context(io::ErrorKind::Interrupted)
            } else {
                CopilotReason::Admission
            }
        );
        assert!(error.receipt().process().is_none());
        assert!(!f.root.join("native-session").exists());
        assert!(sink.bound.is_empty());
        finish(h, &mut result).await;
    }
}

#[tokio::test]
async fn cancel_and_deadline_of_backpressured_frame_and_final_eof_drop_same_original_callback() {
    for deadline in [false, true] {
        for mode in ["answer", "eof_progress"] {
            let signal = Cancellation::default();
            let setup = request(&signal);
            let f = Fixture::new(&setup);
            let h = host().await;
            let ctx = request_until(
                &signal,
                Instant::now() + Duration::from_secs(if deadline { 1 } else { 15 }),
            );
            let entered = Arc::new(tokio::sync::Notify::new());
            let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let mut sink = Sink {
                blocked: Some((Arc::clone(&entered), Arc::clone(&dropped))),
                expected: Some(Arc::clone(ctx.snapshot())),
                ..Default::default()
            };
            let cancellation = async {
                entered.notified().await;
                if !deadline {
                    signal.cancel();
                }
            };
            let joined = async {
                tokio::join!(
                    run(
                        &f,
                        &ctx,
                        &h,
                        mode,
                        CopilotSessionStart::Create,
                        limits(),
                        &mut sink
                    ),
                    cancellation
                )
            };
            let (mut result, ()) = tokio::time::timeout(Duration::from_secs(5), joined)
                .await
                .unwrap();
            assert_eq!(
                result.as_ref().unwrap_err().reason(),
                CopilotReason::Context(if deadline {
                    io::ErrorKind::TimedOut
                } else {
                    io::ErrorKind::Interrupted
                })
            );
            assert!(dropped.load(std::sync::atomic::Ordering::Acquire));
            assert!(result.as_ref().unwrap_err().receipt().process().is_some());
            assert!(
                result
                    .as_ref()
                    .unwrap_err()
                    .receipt()
                    .eligible_text()
                    .is_none()
            );
            finish(h, &mut result).await;
        }
    }
}
