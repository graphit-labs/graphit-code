//! Existing CLI adapters for a single authorized Role child.
use crate::cli_launch_policy::CliLaunchPolicy;
use graphit_ai::{
    cli_antigravity::{self, AntigravityLimits, AntigravitySink},
    cli_claude::{self, ClaudeLimits, ClaudeSink},
    cli_codex::{self, CodexSink, CodexStreamLimits},
    cli_cursor::{self, CursorLimits, CursorSink},
    cli_kimi_print::{self, KimiPrintEvent, KimiPrintEventData, KimiPrintSink},
    cli_qwen::{self, QwenLimits, QwenSink},
    codex_completion::{AttemptPurpose, CodexCompletionRun, CompletionOutcome},
};
use graphit_core::file_io::SnapshotContext;
use graphit_hub::NativeBinding;
use graphit_process::{
    Cleanup, HostLimits, HostReadiness, HostShutdown, HostShutdownObservation, PendingHost,
    ProcessHost, RemainingProcess, RepairObservation, Request, SinkFailure,
};
use std::{
    error::Error,
    ffi::{OsStr, OsString},
    io,
    path::Path,
    sync::Arc,
    time::Instant,
};

pub(super) type NativeFailure = Box<dyn Error + Send + Sync>;
pub(super) struct ProcessResult {
    pub answer: Option<String>,
    pub failure: Option<NativeFailure>,
    pub process: ProcessRun,
}
/// The real Process custody survives a cancelled or bounded cleanup waiter.
pub(in crate::managed_role_run) struct ProcessRun {
    pub(super) host: Option<ProcessHost>,
    pub(super) pending: Option<PendingHost>,
    pub(super) remaining: Option<RemainingProcess>,
}
impl ProcessRun {
    fn retain(
        &mut self,
        process: Option<&mut Result<graphit_process::Outcome, graphit_process::Failure>>,
    ) {
        self.remaining = match process {
            Some(Ok(outcome)) => outcome.remaining.take(),
            Some(Err(failure)) => failure.remaining.take(),
            None => None,
        };
    }
    pub async fn close(&mut self, deadline: Instant) -> (bool, Option<io::Error>) {
        let mut first = None;
        if let Some(remaining) = self.remaining.as_mut() {
            match remaining.repair(deadline).await {
                RepairObservation::Pending { .. } => {}
                RepairObservation::Observed { cleanup, .. } => {
                    if matches!(cleanup, Cleanup::Complete | Cleanup::NotSpawned) {
                        self.remaining = None;
                    } else {
                        first = Some(io::Error::other(format!("Role child cleanup: {cleanup:?}")));
                    }
                }
            }
        }
        if let Some(host) = self.host.take() {
            match host.shutdown(deadline).await {
                HostShutdown::Joined(report) => {
                    if let Some(cause) = report.failure() {
                        first.get_or_insert_with(|| {
                            io::Error::other(format!("Role process host: {cause:?}"))
                        });
                    }
                }
                HostShutdown::Pending(pending) => self.pending = Some(pending),
            }
        }
        if let Some(pending) = self.pending.as_mut()
            && let HostShutdownObservation::Joined(report) = pending.repair(deadline).await
        {
            self.pending = None;
            if let Some(cause) = report.failure() {
                first.get_or_insert_with(|| {
                    io::Error::other(format!("Role process host: {cause:?}"))
                });
            }
        }
        let joined = self.remaining.is_none() && self.pending.is_none();
        if !joined {
            first.get_or_insert_with(|| io::ErrorKind::TimedOut.into());
        }
        (joined, first)
    }
}
struct Sink<'a>(&'a (dyn Fn() -> io::Result<()> + Sync));
impl CodexSink for Sink<'_> {
    async fn push<'a>(
        &'a mut self,
        _: graphit_ai::cli_codex_wire::CodexEventView<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        original
            .check()
            .and_then(|()| (self.0)())
            .map_err(|_| SinkFailure::ConsumerFailed)
    }
}
impl ClaudeSink for Sink<'_> {
    async fn push<'a>(
        &'a mut self,
        _: graphit_ai::cli_claude_wire::ClaudeEventView<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        original
            .check()
            .and_then(|()| (self.0)())
            .map_err(|_| SinkFailure::ConsumerFailed)
    }
}
impl CursorSink for Sink<'_> {
    async fn push<'a>(
        &'a mut self,
        _: graphit_ai::cli_cursor_wire::CursorEventView<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        original
            .check()
            .and_then(|()| (self.0)())
            .map_err(|_| SinkFailure::ConsumerFailed)
    }
}
impl QwenSink for Sink<'_> {
    async fn push<'a>(
        &'a mut self,
        _: graphit_ai::cli_qwen_wire::QwenEventView<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        original
            .check()
            .and_then(|()| (self.0)())
            .map_err(|_| SinkFailure::ConsumerFailed)
    }
}
impl AntigravitySink for Sink<'_> {
    async fn push<'a>(
        &'a mut self,
        _: graphit_ai::cli_antigravity_wire::AntigravityEventView<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        original
            .check()
            .and_then(|()| (self.0)())
            .map_err(|_| SinkFailure::ConsumerFailed)
    }
}

struct KimiSink {
    answer: String,
    response_bytes: usize,
}
impl KimiPrintSink for KimiSink {
    async fn push<'a>(
        &'a mut self,
        event: KimiPrintEvent<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        original.check().map_err(|_| SinkFailure::ConsumerFailed)?;
        if let KimiPrintEventData::Assistant {
            content: Some(content),
            ..
        } = event.data
        {
            self.answer
                .len()
                .checked_add(content.len())
                .filter(|&size| size <= self.response_bytes)
                .ok_or(SinkFailure::ConsumerFailed)?;
            self.answer
                .try_reserve(content.len())
                .map_err(|_| SinkFailure::ConsumerFailed)?;
            self.answer.push_str(&content);
        }
        Ok(())
    }
}

pub(in crate::managed_role_run) struct Input<'a> {
    pub binding: NativeBinding,
    pub program: &'a Path,
    pub cwd: &'a Path,
    pub profile: &'a Path,
    pub environment: Vec<(OsString, OsString)>,
    pub name: &'a str,
    pub instructions: &'a str,
    pub allows_shell: bool,
    pub model: Option<&'a str>,
    pub prompt: String,
}
/// Native profile selection uses only documented host fields and the captured Role body.
pub(super) async fn run(
    original: &Arc<SnapshotContext>,
    input: Input<'_>,
    recheck: &(dyn Fn() -> io::Result<()> + Sync),
) -> io::Result<ProcessResult> {
    recheck()?;
    original.check()?;
    let policy = CliLaunchPolicy::capture();
    let mut args: Vec<OsString> = Vec::new();
    let mut prompt = input.prompt;
    match input.binding {
        NativeBinding::Codex => {
            args.extend(["exec", "--json", "--skip-git-repo-check", "-c"].map(OsString::from));
            args.push(
                format!(
                    "developer_instructions={}",
                    serde_json::to_string(input.instructions)?
                )
                .into(),
            );
            args.push("-c".into());
            args.push(format!("features.shell_tool={}", input.allows_shell).into());
        }
        NativeBinding::Claude => {
            args.extend(
                [
                    "--print",
                    "--output-format",
                    "stream-json",
                    "--verbose",
                    "--include-partial-messages",
                    "--agent",
                    input.name,
                ]
                .map(OsString::from),
            );
        }
        NativeBinding::Cursor => {
            args.extend(["--print", "--output-format", "stream-json"].map(OsString::from));
            prompt = format!("{}\n\n{prompt}", input.instructions);
        }
        NativeBinding::Qwen => {
            args.extend(
                [
                    "--input-format",
                    "text",
                    "--output-format",
                    "stream-json",
                    "--include-partial-messages",
                    "--system-prompt",
                    input.instructions,
                ]
                .map(OsString::from),
            );
        }
        NativeBinding::Kimi => {
            args.extend([
                OsString::from("--agent-file"),
                input.profile.as_os_str().to_owned(),
            ]);
            args.extend(["--output-format", "stream-json"].map(OsString::from));
        }
        NativeBinding::Antigravity => {
            args.extend(
                [
                    "--print",
                    "--input-format",
                    "stream-json",
                    "--output-format",
                    "stream-json",
                    "--agent",
                    input.name,
                ]
                .map(OsString::from),
            );
        }
        _ => return Err(io::ErrorKind::Unsupported.into()),
    }
    if let Some(model) = input.model {
        if model.is_empty() || model.len() > policy.wire.id_bytes {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        args.extend([
            OsString::from(if input.binding == NativeBinding::Kimi {
                "-m"
            } else {
                "--model"
            }),
            OsString::from(model),
        ]);
    }
    let stdin = match input.binding {
        NativeBinding::Codex => {
            args.push("-".into());
            Some(prompt.into_bytes().into())
        }
        NativeBinding::Kimi => {
            args.extend([OsString::from("-p"), OsString::from(prompt)]);
            None
        }
        NativeBinding::Cursor => {
            args.push(prompt.into());
            None
        }
        NativeBinding::Antigravity => Some(
            graphit_ai::cli_antigravity_wire::encode_antigravity_prompt(&prompt)
                .map_err(io::Error::other)?
                .into(),
        ),
        _ => Some(prompt.into_bytes().into()),
    };
    let paths = input
        .environment
        .iter()
        .find(|(key, _)| key == OsStr::new("PATH"))
        .map(|(_, value)| value);
    let program = which::which_in(input.program.as_os_str(), paths, input.cwd)
        .map_err(|cause| io::Error::new(io::ErrorKind::NotFound, cause))?;
    let request = Request {
        program,
        args,
        cwd: input.cwd.to_path_buf(),
        environment: input.environment,
        stdin,
        limits: policy.process,
    };
    let host = ProcessHost::install(original.cancellation(), HostLimits::new(1, 1)?)?;
    let mut process = ProcessRun {
        host: Some(host),
        pending: None,
        remaining: None,
    };
    let deadline = original.required_deadline()?;
    let host = process.host.as_ref().expect("installed Role process host");
    let ready = tokio::select! {
        ready = host.ready(deadline) => ready,
        () = original.cancellation().cancelled() => HostReadiness::Sealed,
    };
    let mut sink = Sink(recheck);
    let mut answer = None;
    let mut failure: Option<NativeFailure> = None;
    if !matches!(ready, HostReadiness::Ready) {
        failure = Some(Box::new(io::Error::other(format!(
            "Role process readiness: {ready:?}"
        ))));
    } else {
        match input.binding {
            NativeBinding::Codex => {
                match CodexCompletionRun::new(
                    original,
                    input.model,
                    &policy.wire,
                    policy.completion,
                ) {
                    Ok(mut run) => {
                        let result = cli_codex::run_codex(
                            &mut run,
                            host,
                            request,
                            AttemptPurpose::Request,
                            None,
                            CodexStreamLimits {
                                stdout_bytes: policy.process.stdout_bytes,
                                stderr_bytes: policy.process.stderr_bytes,
                            },
                            &mut sink,
                        )
                        .await;
                        let receipt = match result {
                            Ok(receipt) => receipt,
                            Err(cause) => {
                                let parts = cause.into_parts();
                                failure = Some(match parts.cause {
                                    Some(cause) => Box::new(cause),
                                    None => Box::new(io::Error::other(format!(
                                        "Codex Role run: {:?}",
                                        parts.reason
                                    ))),
                                });
                                parts.receipt
                            }
                        };
                        let mut parts = receipt.into_parts();
                        process.retain(parts.process.as_mut());
                        match parts.completion {
                            Some(CompletionOutcome::Answer(value)) if failure.is_none() => {
                                answer = Some(value.text().to_owned())
                            }
                            Some(CompletionOutcome::Failed(cause)) if failure.is_none() => {
                                failure = Some(Box::new(cause))
                            }
                            _ if failure.is_none() => {
                                failure = Some(Box::new(io::Error::other(
                                    "Codex Role returned no answer",
                                )))
                            }
                            _ => {}
                        }
                    }
                    Err(cause) => failure = Some(Box::new(cause)),
                }
            }
            NativeBinding::Claude => {
                let limits = ClaudeLimits {
                    wire: policy.wire,
                    stdout_bytes: policy.process.stdout_bytes,
                    stderr_bytes: policy.process.stderr_bytes,
                    frames: 65536,
                    response_bytes: policy.completion.max_response_bytes,
                };
                match cli_claude::run_claude(original, host, request, limits, &mut sink).await {
                    Ok(mut receipt) => {
                        process.retain(receipt.process_mut());
                        answer = receipt.answer().map(str::to_owned);
                    }
                    Err(mut cause) => {
                        process.retain(cause.receipt_mut().process_mut());
                        failure = Some(Box::new(cause));
                    }
                }
            }
            NativeBinding::Cursor => {
                let limits = CursorLimits {
                    wire: policy.wire,
                    stdout_bytes: policy.process.stdout_bytes,
                    stderr_bytes: policy.process.stderr_bytes,
                    frames: 65536,
                    response_bytes: policy.completion.max_response_bytes,
                };
                match cli_cursor::run_cursor(original, host, request, limits, &mut sink).await {
                    Ok(mut receipt) => {
                        process.retain(receipt.process_mut());
                        answer = receipt.answer().map(str::to_owned);
                    }
                    Err(mut cause) => {
                        process.retain(cause.receipt_mut().process_mut());
                        failure = Some(Box::new(cause));
                    }
                }
            }
            NativeBinding::Qwen => {
                let limits = QwenLimits {
                    wire: policy.wire,
                    stdout_bytes: policy.process.stdout_bytes,
                    stderr_bytes: policy.process.stderr_bytes,
                    frames: 65536,
                    response_bytes: policy.completion.max_response_bytes,
                };
                match cli_qwen::run_qwen(original, host, request, limits, &mut sink).await {
                    Ok(mut receipt) => {
                        process.retain(receipt.process_mut());
                        answer = receipt.answer().map(str::to_owned);
                    }
                    Err(mut cause) => {
                        process.retain(cause.receipt_mut().process_mut());
                        failure = Some(Box::new(cause));
                    }
                }
            }
            NativeBinding::Kimi => {
                let mut sink = KimiSink {
                    answer: String::new(),
                    response_bytes: policy.completion.max_response_bytes,
                };
                match cli_kimi_print::run_kimi_print(
                    original,
                    host,
                    request,
                    recheck,
                    policy.wire,
                    &mut sink,
                )
                .await
                {
                    Ok(mut receipt) => {
                        process.retain(receipt.process_mut());
                        if !sink.answer.is_empty() {
                            answer = Some(sink.answer);
                        }
                    }
                    Err(mut cause) => {
                        process.retain(cause.receipt_mut().process_mut());
                        failure = Some(Box::new(cause));
                    }
                }
            }
            NativeBinding::Antigravity => {
                let limits = AntigravityLimits {
                    wire: policy.wire,
                    stdout_bytes: policy.process.stdout_bytes,
                    stderr_bytes: policy.process.stderr_bytes,
                    frames: 65536,
                    response_bytes: policy.completion.max_response_bytes,
                };
                match cli_antigravity::run_antigravity(original, host, request, limits, &mut sink)
                    .await
                {
                    Ok(mut receipt) => {
                        process.retain(receipt.process_mut());
                        answer = receipt.answer().map(str::to_owned);
                    }
                    Err(mut cause) => {
                        process.retain(cause.receipt_mut().process_mut());
                        failure = Some(Box::new(cause));
                    }
                }
            }
            _ => unreachable!("validated process Role binding"),
        }
    }
    if answer.is_none() && failure.is_none() {
        failure = Some(Box::new(io::Error::other("native Role returned no answer")));
    }
    Ok(ProcessResult {
        answer,
        failure,
        process,
    })
}
