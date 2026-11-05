// Included inside the existing AskService test module to reuse its USER-store fixture.
#[cfg(unix)]
use crate::{
    native_install_http::NativeInstallPolicy,
    source_server::{SourcePolicy, SourceServer},
};
#[allow(dead_code)]
#[cfg(unix)]
mod copilot_authority_fixture {
    include!("../tests/support/knowledge_ask_server.rs");
}
#[tokio::test]
#[cfg(unix)]
async fn copilot_unsubmitted_guard_releases_actual_user_writer_without_native_owner() {
    use graphit_core::file_lock::FileLock;
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(10),
        Cancellation::default(),
    ));
    let (_temp, scope, turn) = conversation_fixture_engine(
        &original,
        Path::new("/captured/copilot"),
        graphit_ai::knowledge_conversation::ConversationEngine::Copilot,
    );
    let entry = Arc::new(crate::knowledge_conversation::ConversationCustodyEntry::new(turn));
    assert_writer_held(&scope);
    let decision =
        crate::knowledge_conversation::ConversationDecisionGuard::new(Arc::clone(&entry));
    drop(decision);
    assert!(entry.released());
    assert!(!FileLock::is_locked(scope.runtime_root().join("artifact-store.lock")).unwrap());
}
#[tokio::test]
#[cfg(unix)]
async fn copilot_abandoned_or_cancelled_cli_retains_user_writer_until_actual_process_join() {
    use graphit_ai::cli_copilot::{CopilotRequest, CopilotSessionStart, CopilotSink, run_copilot};
    use graphit_auth::{
        Permission, ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ScopeCapture,
        ScopeInput,
    };
    use graphit_core::{
        artifact_store::{CapturedPublicationScope, ScopeLimits},
        brand::Brand,
        file_lock::FileLock,
        module_settings::Module,
    };
    use graphit_hub::{native::ContentLimits, policy::RequestContext};
    use graphit_process::{HostLimits, HostReadiness, HostShutdown};

    for action in ["abandon", "cancel"] {
        let fixture = copilot_authority_fixture::fixture(
            &[
                Permission::Read(Module::Ast),
                Permission::Read(Module::Knowledge),
                Permission::MutateModule(Module::Knowledge),
            ],
            Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap(),
            NativeBinding::Codex,
        );
        let cancel = Cancellation::default();
        let context = RequestContext::new(
            ContentLimits {
                max_input_bytes: 1024 * 1024,
                max_output_bytes: 8 * 1024 * 1024,
                max_depth: 64,
                max_nodes: 65536,
                max_scalar_bytes: 1024 * 1024,
                max_collection_entries: 65536,
            },
            &cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let (paths, id, version) = &fixture.projects[0];
        let live = CapturedPublicationScope::project(
            paths,
            *id,
            ScopeLimits::new(16, 4096, 256).unwrap(),
            context.snapshot(),
        )
        .unwrap();
        let record = live.binding().unwrap();
        let caller = fixture
            .auth
            .issuer()
            .authenticate_runtime(&fixture.raw, &context)
            .unwrap();
        let authority = ProjectRecordAuthority::new(fixture.auth.issuer());
        let admission = authority
            .admit_authenticated_read(
                &caller,
                &context,
                ScopeInput {
                    installation: fixture.auth.installation(),
                    project: Some(ProjectCapture {
                        scope: ScopeCapture {
                            live: &live,
                            record: &record,
                        },
                        version,
                    }),
                    providers: &[],
                    binding: NativeBinding::Codex,
                    global_version: "",
                },
                ProjectReadModule::Knowledge,
            )
            .unwrap();
        let program = fixture.temp.path().join("copilot-before-output");
        // This native child waits before any protocol output. No fabricated SDK ACK.
        let started_path = fixture.temp.path().join("copilot-started");
        crate::test_agent_fixture::install(
            &program,
            serde_json::json!({
                "kind":"raw", "process":{"pid":started_path,"hold":true}
            }),
        );
        let (_user, scope, turn) = conversation_fixture_engine(
            context.snapshot(),
            &program,
            graphit_ai::knowledge_conversation::ConversationEngine::Copilot,
        );
        let entry = Arc::new(crate::knowledge_conversation::ConversationCustodyEntry::new(turn));
        let mut decision =
            crate::knowledge_conversation::ConversationDecisionGuard::new(Arc::clone(&entry));
        let host =
            ProcessHost::install(&Cancellation::default(), HostLimits::new(1, 1).unwrap()).unwrap();
        assert!(matches!(
            host.ready(Instant::now() + Duration::from_secs(5)).await,
            HostReadiness::Ready
        ));
        let policy = AskLaunchPolicy::capture(
            NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap(),
        );
        struct Authority<'a, 'request> {
            original: &'a Arc<SnapshotContext>,
            admission: &'a graphit_auth::ProjectReadAdmission<'request>,
        }
        impl CopilotSink for Authority<'_, '_> {
            fn recheck(&self, original: &SnapshotContext) -> io::Result<()> {
                if !std::ptr::eq(self.original.as_ref(), original) {
                    return Err(io::ErrorKind::PermissionDenied.into());
                }
                self.original.check()?;
                self.admission
                    .recheck()
                    .map_err(|_| io::ErrorKind::PermissionDenied.into())
            }
            fn session_bound(
                &mut self,
                _: &str,
                _: bool,
                original: &SnapshotContext,
            ) -> io::Result<()> {
                self.recheck(original)
            }
            async fn push<'a>(
                &'a mut self,
                _: &'a serde_json::value::RawValue,
                original: &'a SnapshotContext,
            ) -> io::Result<()> {
                self.recheck(original)
            }
        }
        let mut sink = Authority {
            original: context.snapshot(),
            admission: &admission,
        };
        decision.submitted();
        let mut execution = Box::pin(run_copilot(
            context.snapshot(),
            &admission,
            &host,
            CopilotRequest {
                command: Request {
                    program,
                    args: Vec::new(),
                    cwd: paths.root().to_owned(),
                    environment: vec![(
                        "COPILOT_STARTED".into(),
                        started_path.clone().into_os_string(),
                    )],
                    stdin: None,
                    limits: policy.process,
                },
                session: CopilotSessionStart::Create,
                model: "fixture-model".into(),
                prompt: "authorized controlled turn".into(),
            },
            graphit_ai::cli_copilot::CopilotLimits {
                wire: policy.wire,
                input_bytes: policy.process.stdin_bytes,
                output_bytes: policy.process.stdout_bytes,
                prompt_text_bytes: policy.question.max_prompt_bytes.get(),
                response_text_bytes: policy.completion.max_response_bytes,
            },
            &mut sink,
        ));
        let mut early = None;
        let started = tokio::select! {
            result = &mut execution => { early = Some(result); false },
            result = tokio::time::timeout(Duration::from_secs(5), async {
                while !started_path.exists() { tokio::time::sleep(Duration::from_millis(10)).await; }
            }) => result.is_ok(),
        };
        let held_before =
            FileLock::is_locked(scope.runtime_root().join("artifact-store.lock")).unwrap();
        let mut cancelled_failure = false;
        if action == "cancel" {
            cancel.cancel();
            let mut result = match early.take() {
                Some(result) => result,
                None => execution.as_mut().await,
            };
            cancelled_failure = result.is_err();
            match &mut result {
                Ok(receipt) => entry.retain_copilot(receipt),
                Err(error) => entry.retain_copilot(error.receipt_mut()),
            }
        } else {
            if let Some(mut result) = early.take() {
                match &mut result {
                    Ok(receipt) => entry.retain_copilot(receipt),
                    Err(error) => entry.retain_copilot(error.receipt_mut()),
                }
            }
        }
        drop(execution);
        drop(decision);
        let held_after_abandon =
            FileLock::is_locked(scope.runtime_root().join("artifact-store.lock")).unwrap();
        // Always drain the real Host before assertions, including early producer failure.
        let joined = host.shutdown(Instant::now() + Duration::from_secs(5)).await;
        let clean_join = match joined {
            HostShutdown::Joined(report) => {
                entry.host_joined();
                report.failure().is_none()
            }
            HostShutdown::Pending(_) => false,
        };
        assert!(
            started,
            "CLI child did not reach the actual pre-output boundary"
        );
        assert!(held_before);
        assert!(
            clean_join,
            "actual Process Host join failed or remains pending"
        );
        if action == "abandon" {
            assert!(held_after_abandon);
        } else {
            assert!(cancelled_failure);
        }
        assert!(entry.released());
        assert!(!FileLock::is_locked(scope.runtime_root().join("artifact-store.lock")).unwrap());
    }
}
