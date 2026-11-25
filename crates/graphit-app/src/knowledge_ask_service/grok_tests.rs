use super::*;
use graphit_ai::{
    cli_grok::{GrokReceipt, GrokSink, run_grok},
    cli_grok_wire::{GrokEventData, GrokEventView},
};
use graphit_core::file_io::SnapshotContext;
use std::io::Write;

struct CancelAfterText<'a> {
    prompt: &'a Path,
}
impl GrokSink for CancelAfterText<'_> {
    async fn push<'a>(
        &'a mut self,
        event: GrokEventView<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), graphit_process::SinkFailure> {
        assert!(matches!(&event.data, GrokEventData::Text(text) if text.data == "native prefix"));
        assert_eq!(
            std::fs::read_to_string(self.prompt).unwrap(),
            "private prompt"
        );
        original.cancellation().cancel();
        // Keep the callback pending so the actual Process cancellation wins.
        std::future::pending().await
    }
}

fn cancelled_process_has_remaining(receipt: &GrokReceipt) -> bool {
    assert_eq!(receipt.frames(), 1);
    assert!(receipt.answer().is_none());
    let process = receipt
        .process()
        .expect("the real child delivered its text before cancellation")
        .as_ref()
        .expect_err("the original request was cancelled");
    assert_eq!(process.reason, graphit_process::Reason::Cancelled);
    let remaining = process.remaining.is_some();
    if !remaining {
        assert_eq!(process.cleanup, Cleanup::Complete);
        assert!(
            process.status.is_some(),
            "terminal observation must reap the child"
        );
    }
    remaining
}

#[tokio::test]
async fn grok_cancelled_process_keeps_prompt_until_actual_observation() {
    let temp = tempfile::tempdir().unwrap();
    let program = temp.path().join("grok");
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"private prompt").unwrap();
    let file = file.into_temp_path();
    let path = file.to_path_buf();
    crate::test_agent_fixture::install(
        &program,
        serde_json::json!({
            "kind":"raw", "process":{
                "args":["--prompt-file",path,"--output-format","streaming-json","--model","literal model","--verbatim","--deny","*"],
                "prompt_file":{"flag":"--prompt-file","expected":"private prompt"},
                "stdout":"{\"type\":\"text\",\"data\":\"native prefix\"}\n", "hold":true
            }
        }),
    );

    let available = AskAvailability::capture(
        CliCompletionProvider::Grok,
        program.as_os_str(),
        Vec::new(),
        temp.path(),
    )
    .unwrap();
    let service = available.service().unwrap();
    let host = service.host().unwrap();
    assert!(matches!(
        host.ready(Instant::now() + Duration::from_secs(5)).await,
        HostReadiness::Ready
    ));
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(10),
        Cancellation::default(),
    ));

    let mut request = service.grok_request(temp.path(), "literal model", file.as_ref());
    assert!(request.stdin.is_none());
    // A short observation can return Remaining or consume an already reaped child.
    request.limits.cleanup = Duration::from_nanos(2);
    request.limits.termination_grace = Duration::from_nanos(1);
    let mut execution = run_grok(
        &original,
        &host,
        request,
        service.grok_limits(),
        &mut CancelAfterText { prompt: &path },
    )
    .await;
    let receipt = execution
        .as_mut()
        .expect_err("cancelled Grok must not deliver a final answer")
        .receipt_mut();
    let pending = cancelled_process_has_remaining(receipt);
    service.retain_grok(receipt, file);
    assert_eq!(
        path.exists(),
        pending,
        "prompt custody follows the actual receipt"
    );
    assert_eq!(
        service.remaining.lock().unwrap().len(),
        usize::from(pending)
    );
    if pending {
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "private prompt");
    }
    drop(execution);
    assert_eq!(
        path.exists(),
        pending,
        "custody must outlive the caller receipt"
    );
    drop(host);
    let joined = service.shutdown().await;
    service.reap().await;
    assert!(service.remaining.lock().unwrap().is_empty());
    assert!(
        !path.exists(),
        "observing the process must release its prompt file"
    );
    assert_eq!(
        joined.is_err(),
        service.cleanup_failure.lock().unwrap().is_some(),
        "{joined:?}"
    );
}

#[tokio::test]
async fn grok_conversation_guard_retains_prompt_and_writer_until_real_join() {
    use crate::knowledge_conversation::ConversationDecisionGuard;
    use graphit_ai::knowledge_conversation::ConversationEngine;
    for cancelled in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let program = temp.path().join("grok");
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"private prompt").unwrap();
        let file = file.into_temp_path();
        let path = file.to_path_buf();
        crate::test_agent_fixture::install(
            &program,
            serde_json::json!({
                "kind":"raw", "process":{
                    "args":["--prompt-file",path,"--output-format","streaming-json","--model","fixture-model","--verbatim","--deny","*"],
                    "prompt_file":{"flag":"--prompt-file","expected":"private prompt"},
                    "stdout":"{\"type\":\"text\",\"data\":\"native prefix\"}\n",
                    "ignore_term":true, "release":temp.path().join("release")
                }
            }),
        );

        let available = AskAvailability::capture(
            CliCompletionProvider::Grok,
            program.as_os_str(),
            vec![(
                "GROK_RELEASE".into(),
                temp.path().join("release").into_os_string(),
            )],
            temp.path(),
        )
        .unwrap();
        let service = available.service().unwrap();
        let host = service.host().unwrap();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        assert!(matches!(
            host.ready(original.required_deadline().unwrap()).await,
            HostReadiness::Ready
        ));
        let (_user, scope, turn) = super::tests::conversation_fixture_engine(
            &original,
            service.program(),
            ConversationEngine::Grok,
        );
        let entry = service.hold_conversation_turn(turn);

        let mut request = service.grok_request(temp.path(), "fixture-model", file.as_ref());
        assert!(request.stdin.is_none());
        if cancelled {
            request.limits.cleanup = Duration::from_nanos(2);
            request.limits.termination_grace = Duration::from_nanos(1);
        }
        entry.configure_grok(&mut request, file, &original).unwrap();
        let mut decision = ConversationDecisionGuard::new(Arc::clone(&entry));
        decision.submitted();
        struct Observe<'a> {
            started: &'a tokio::sync::Notify,
            cancel: bool,
        }
        impl GrokSink for Observe<'_> {
            async fn push<'a>(
                &'a mut self,
                event: GrokEventView<'a>,
                original: &'a SnapshotContext,
            ) -> Result<(), graphit_process::SinkFailure> {
                assert!(
                    matches!(&event.data, GrokEventData::Text(text) if text.data == "native prefix")
                );
                self.started.notify_one();
                if self.cancel {
                    original.cancellation().cancel();
                    return std::future::pending().await;
                }
                Ok(())
            }
        }
        let started = tokio::sync::Notify::new();
        let mut sink = Observe {
            started: &started,
            cancel: cancelled,
        };
        let mut execution = Box::pin(run_grok(
            &original,
            &host,
            request,
            service.grok_limits(),
            &mut sink,
        ));
        let pending = if cancelled {
            let mut result = execution.await;
            let receipt = result
                .as_mut()
                .expect_err("cancelled Grok must not deliver a final answer")
                .receipt_mut();
            let pending = cancelled_process_has_remaining(receipt);
            entry.retain_grok(receipt);
            assert!(matches!(
                entry.finish(&original, false),
                crate::knowledge_conversation::ConversationPersistence::Denied
            ));
            decision.decided();
            assert_eq!(entry.remaining().is_some(), pending);
            pending
        } else {
            tokio::select! { ()=started.notified()=>(), _=&mut execution=>panic!("native producer returned before controlled future drop") }
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "private prompt");
            assert!(!entry.released());

            drop(execution);
            drop(decision);
            assert!(entry.remaining().is_none());
            true
        };
        assert_eq!(
            path.exists(),
            pending,
            "prompt custody follows actual observation"
        );
        assert_eq!(entry.released(), !pending);
        if pending {
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "private prompt");
        } else {
            assert!(
                !graphit_core::file_lock::FileLock::is_locked(
                    scope.runtime_root().join("artifact-store.lock")
                )
                .unwrap()
            );
        }
        if !cancelled {
            service.reap().await;
            assert!(
                !entry.released(),
                "dropped future has no actual terminal receipt yet"
            );
            assert!(path.exists());
        }
        std::fs::write(temp.path().join("release"), b"release").unwrap();
        drop(host);
        let joined = service.shutdown().await;
        service.reap().await;
        assert!(entry.released());
        assert!(!path.exists(), "private prompt retained after actual join");
        assert!(service.conversations.lock().unwrap().is_empty());
        assert!(
            !graphit_core::file_lock::FileLock::is_locked(
                scope.runtime_root().join("artifact-store.lock")
            )
            .unwrap()
        );
        assert_eq!(
            joined.is_err(),
            service.cleanup_failure.lock().unwrap().is_some()
        );
    }
}
