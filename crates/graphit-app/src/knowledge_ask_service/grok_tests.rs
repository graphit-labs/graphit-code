use super::*;
use graphit_ai::{
    cli_grok::{GrokSink, run_grok},
    cli_grok_wire::GrokEventView,
};
use graphit_core::file_io::SnapshotContext;
use std::{io::Write, os::unix::fs::PermissionsExt};

struct Ignore;
impl GrokSink for Ignore {
    async fn push<'a>(
        &'a mut self,
        _: GrokEventView<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), graphit_process::SinkFailure> {
        original.cancellation().cancel();
        Ok(())
    }
}

#[tokio::test]
async fn grok_pending_process_retains_prompt_until_join() {
    let temp = tempfile::tempdir().unwrap();
    let program = temp.path().join("grok");
    std::fs::write(
        &program,
        r#"#!/usr/bin/python3
import sys,time
from pathlib import Path
assert sys.argv[1]=='--prompt-file'
assert sys.argv[3:5]==['--output-format','streaming-json']
assert sys.argv[5:]==['--model','literal model','--verbatim','--deny','*']
assert Path(sys.argv[2]).read_text()=='private prompt'
assert sys.stdin.read()==''
print('{"type":"text","data":"native prefix"}',flush=True)
time.sleep(2)
"#,
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let native = NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap();
    let available = AskAvailability::capture(
        CliCompletionProvider::Grok,
        program.as_os_str(),
        Vec::new(),
        temp.path(),
        native,
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
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"private prompt").unwrap();
    let file = file.into_temp_path();
    let path = file.to_path_buf();
    let mut request = service.grok_request(temp.path(), "literal model", file.as_ref());
    // Force the existing Process producer to return its real pending cleanup owner.
    request.limits.cleanup = Duration::from_nanos(2);
    request.limits.termination_grace = Duration::from_nanos(1);
    let mut execution = run_grok(
        &original,
        &host,
        request,
        service.grok_limits(),
        &mut Ignore,
    )
    .await;
    let receipt = match &mut execution {
        Ok(receipt) => receipt,
        Err(error) => error.receipt_mut(),
    };
    assert!(
        match receipt.process() {
            Some(Ok(process)) => process.remaining.is_some(),
            Some(Err(process)) => process.remaining.is_some(),
            None => false,
        },
        "the actual producer must return Remaining"
    );
    service.retain_grok(receipt, file);
    assert!(path.exists(), "the pending owner must keep the prompt file");
    assert!(!service.remaining.lock().unwrap().is_empty());
    drop(execution);
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
    for return_remaining in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let program = temp.path().join("grok");
        std::fs::write(&program, r#"#!/usr/bin/python3
import json, os, signal, sys, time
from pathlib import Path
args=sys.argv[1:]
assert args[0]=='--prompt-file'
prompt=Path(args[1])
assert prompt.read_text()=='private prompt'
assert args[2:]==['--output-format','streaming-json','--model','fixture-model','--verbatim','--deny','*']
signal.signal(signal.SIGTERM, lambda *_: None)
print(json.dumps({'type':'text','data':'native prefix'}), flush=True)
while not Path(os.environ['GROK_RELEASE']).exists():
    time.sleep(0.01)
assert prompt.exists()
"#).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let native = NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap();
        let available = AskAvailability::capture(
            CliCompletionProvider::Grok,
            program.as_os_str(),
            vec![(
                "GROK_RELEASE".into(),
                temp.path().join("release").into_os_string(),
            )],
            temp.path(),
            native,
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
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"private prompt").unwrap();
        let file = file.into_temp_path();
        let path = file.to_path_buf();
        let mut request = service.grok_request(temp.path(), "fixture-model", file.as_ref());
        if return_remaining {
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
                _: GrokEventView<'a>,
                original: &'a SnapshotContext,
            ) -> Result<(), graphit_process::SinkFailure> {
                self.started.notify_one();
                if self.cancel {
                    original.cancellation().cancel();
                }
                Ok(())
            }
        }
        let started = tokio::sync::Notify::new();
        let mut sink = Observe {
            started: &started,
            cancel: return_remaining,
        };
        let mut execution = Box::pin(run_grok(
            &original,
            &host,
            request,
            service.grok_limits(),
            &mut sink,
        ));
        if return_remaining {
            let mut result = execution.await;
            let receipt = match &mut result {
                Ok(receipt) => receipt,
                Err(error) => error.receipt_mut(),
            };
            assert!(
                match receipt.process() {
                    Some(Ok(p)) => p.remaining.is_some(),
                    Some(Err(p)) => p.remaining.is_some(),
                    None => false,
                },
                "the actual producer must return Remaining"
            );
            entry.retain_grok(receipt);
            let _ = entry.finish(&original, false);
            decision.decided();
        } else {
            tokio::select! { ()=started.notified()=>(), _=&mut execution=>panic!("native producer returned before controlled future drop") }
            drop(execution);
            drop(decision);
            assert!(entry.remaining().is_none());
        }
        assert!(path.exists(), "prompt lost before actual join");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "private prompt");
        assert!(!entry.released());
        super::tests::assert_writer_held(&scope);
        if !return_remaining {
            service.reap().await;
            assert!(
                !entry.released(),
                "dropped future has no actual terminal receipt yet"
            );
            assert!(path.exists());
            super::tests::assert_writer_held(&scope);
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
