//! Dispatch the captured native binding through existing protocol adapters.
mod acp;
mod process;
use graphit_core::file_io::SnapshotContext;
use graphit_hub::{NativeBinding, native::ContentLimits};
use process::{Input, NativeFailure, ProcessRun};
use std::{io, sync::Arc, time::Instant};

pub(super) struct NativeExecution {
    pub answer: Option<String>,
    pub failure: Option<NativeFailure>,
    pub cleanup: Option<NativeCleanup>,
}
pub(super) enum NativeCleanup {
    Process(ProcessRun),
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    Acp(acp::AcpRun),
}
impl NativeCleanup {
    pub async fn close(&mut self, deadline: Instant) -> (bool, Option<io::Error>) {
        match self {
            Self::Process(owner) => owner.close(deadline).await,
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::Acp(owner) => owner.close(deadline).await,
        }
    }
}
pub(super) use process::Input as NativeInput;
pub(super) async fn run(
    original: &Arc<SnapshotContext>,
    input: Input<'_>,
    content: ContentLimits,
    recheck: &(dyn Fn() -> io::Result<()> + Sync),
) -> io::Result<NativeExecution> {
    match input.binding {
        NativeBinding::Codex
        | NativeBinding::Claude
        | NativeBinding::Cursor
        | NativeBinding::Qwen
        | NativeBinding::Antigravity
        | NativeBinding::Kimi => {
            let result = process::run(original, input, recheck).await?;
            Ok(NativeExecution {
                answer: result.answer,
                failure: result.failure,
                cleanup: Some(NativeCleanup::Process(result.process)),
            })
        }
        NativeBinding::KiroV3 | NativeBinding::OpenCode => {
            let result = acp::run(original, input, content, recheck).await?;
            Ok(NativeExecution {
                answer: result.answer,
                failure: result.failure,
                #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
                cleanup: Some(NativeCleanup::Acp(result.process)),
            })
        }
    }
}
