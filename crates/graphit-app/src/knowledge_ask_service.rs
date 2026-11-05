//! Captured completion launch inputs and durable custody of the existing Process owners.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]
use crate::knowledge_question::KnowledgeQuestionLimits;
use graphit_ai::{
    cli_antigravity::{AntigravityLimits, AntigravityReceipt},
    cli_claude::{ClaudeLimits, ClaudeReceipt},
    cli_codex::CodexReceipt,
    cli_codex_wire::CodexWireLimits,
    cli_cursor::{CursorLimits, CursorReceipt},
    cli_grok::{GrokLimits, GrokReceipt},
    cli_provider::CliCompletionProvider,
    cli_qwen::{QwenLimits, QwenReceipt},
    codex_completion::CompletionLimits,
};
use graphit_core::{file_lock::Cancellation, native_document_file::NativeDocumentFileLimits};
use graphit_data::knowledge_index::{KnowledgeIndexContentLimits, KnowledgeIndexReadLimits};
use graphit_hub::{NativeBinding, policy::RequestContext};
use graphit_process::{
    Cleanup, HostLimits, HostReadiness, HostReport, HostShutdown, HostShutdownObservation, Limits,
    PendingHost, ProcessHost, RemainingProcess, RepairObservation, Request,
};
#[cfg(all(test, unix))]
use std::time::Duration;
use std::{
    ffi::{OsStr, OsString},
    io,
    num::{NonZeroU32, NonZeroUsize},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::task::JoinHandle;

/// Distribution launch limits, captured once rather than accepted from a tool call.
pub(crate) struct AskLaunchPolicy {
    pub wire: CodexWireLimits,
    pub completion: CompletionLimits,
    pub question: KnowledgeQuestionLimits,
    pub process: Limits,
}

impl AskLaunchPolicy {
    fn capture(native: NativeDocumentFileLimits) -> Self {
        let positive = |value| NonZeroUsize::new(value).expect("positive distribution limit");
        let native_cli = crate::cli_launch_policy::CliLaunchPolicy::capture();
        Self {
            wire: native_cli.wire,
            completion: native_cli.completion,
            question: KnowledgeQuestionLimits {
                read: KnowledgeIndexReadLimits {
                    content: KnowledgeIndexContentLimits {
                        max_identifier_bytes: positive(4096),
                    },
                    native,
                    batch_rows: NonZeroU32::new(128).expect("positive batch"),
                },
                max_query_bytes: positive(64 * 1024),
                max_selected_pages: positive(4096),
                max_prompt_bytes: positive(1024 * 1024),
            },
            process: native_cli.process,
        }
    }
}

enum HostOwner {
    Live(Arc<ProcessHost>),
    Joining(JoinHandle<HostShutdown>),
    Pending(PendingHost),
    // Empty terminal payloads retain the drained phase after reporting its outcome.
    Joined(Option<HostReport>),
    JoinFailed(Option<tokio::task::JoinError>),
}

struct RetainedProcess {
    process: Arc<tokio::sync::Mutex<RemainingProcess>>,
    _prompt_file: Option<tempfile::TempPath>,
}

/// Capture one provider/program selection without filesystem or environment discovery.
pub(crate) fn captured_cli(
    binding: NativeBinding,
    configured: Option<&str>,
) -> io::Result<(CliCompletionProvider, OsString)> {
    if let Some(program) = configured {
        if program.trim().is_empty() || program.contains('\0') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "configured completion CLI is invalid",
            ));
        }
        let name = Path::new(program)
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "configured completion CLI is invalid",
                )
            })?;
        let provider = CliCompletionProvider::for_executable(name)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        return Ok((provider, OsString::from(program)));
    }
    let provider = match binding {
        NativeBinding::Antigravity => CliCompletionProvider::Antigravity,
        NativeBinding::Claude => CliCompletionProvider::Claude,
        NativeBinding::Codex => CliCompletionProvider::Codex,
        NativeBinding::Cursor => CliCompletionProvider::Cursor,
        NativeBinding::KiroV3 => CliCompletionProvider::KiroV3,
        NativeBinding::OpenCode => CliCompletionProvider::OpenCode,
        NativeBinding::Qwen => CliCompletionProvider::Qwen,
        NativeBinding::Kimi => CliCompletionProvider::Kimi,
    };
    Ok((provider, OsString::from(provider.executable())))
}

/// A missing selected CLI retains its resolver cause without choosing another provider.
pub(crate) enum AskAvailability {
    Available(Box<AskService>),
    Unavailable(which::Error),
}
impl AskAvailability {
    pub fn capture(
        provider: CliCompletionProvider,
        program: &OsStr,
        environment: Vec<(OsString, OsString)>,
        cwd: &Path,
        native: NativeDocumentFileLimits,
    ) -> io::Result<Self> {
        let paths = environment
            .iter()
            .find(|(name, _)| name == OsStr::new("PATH"))
            .map(|(_, value)| value);
        let program = match which::which_in(program, paths, cwd) {
            Ok(program) => program,
            Err(error) => return Ok(Self::Unavailable(error)),
        };
        let process_host = match provider {
            CliCompletionProvider::Antigravity
            | CliCompletionProvider::Claude
            | CliCompletionProvider::Codex
            | CliCompletionProvider::Copilot
            | CliCompletionProvider::Cursor
            | CliCompletionProvider::Grok
            | CliCompletionProvider::Kimi
            | CliCompletionProvider::Qwen => true,
            CliCompletionProvider::KiroV3 | CliCompletionProvider::OpenCode => false,
        };
        let stop = Cancellation::default();
        let host = if process_host {
            Some(Arc::new(ProcessHost::install(
                &stop,
                HostLimits::new(2, 2)?,
            )?))
        } else {
            None
        };
        let opencode = (provider == CliCompletionProvider::OpenCode).then(OpenCodeProcesses::new);
        Ok(Self::Available(Box::new(AskService {
            program,
            provider,
            environment,
            policy: AskLaunchPolicy::capture(native),
            stop,
            host: Mutex::new(host),
            kiro_slots: Arc::new(tokio::sync::Semaphore::new(2)),
            kiro_processes: Mutex::new(Vec::new()),
            opencode,
            shutdown: tokio::sync::Mutex::new(None),
            remaining: Mutex::new(Vec::new()),
            kimi: Mutex::new(Vec::new()),
            conversations: Mutex::new(Vec::new()),
            cleanup_failure: Mutex::new(None),
        })))
    }
    pub fn service(&self) -> Option<&AskService> {
        match self {
            Self::Available(service) => Some(service.as_ref()),
            _ => None,
        }
    }
    pub fn unavailable_message(&self) -> String {
        match self {
            Self::Unavailable(error) => format!(
                "Knowledge Ask native CLI unavailable: {error}; install the configured CLI in the startup PATH and restart the service"
            ),
            Self::Available(_) => "Knowledge Ask service stopped".into(),
        }
    }
}

struct OpenCodeProcesses {
    slots: Arc<tokio::sync::Semaphore>,
    owners: Mutex<Vec<Arc<crate::acp_process::AcpProcessOwner>>>,
}
impl OpenCodeProcesses {
    fn new() -> Self {
        Self {
            slots: Arc::new(tokio::sync::Semaphore::new(2)),
            owners: Mutex::new(Vec::new()),
        }
    }
}

pub(crate) struct AskService {
    program: PathBuf,
    provider: CliCompletionProvider,
    environment: Vec<(OsString, OsString)>,
    pub policy: AskLaunchPolicy,
    stop: Cancellation,
    host: Mutex<Option<Arc<ProcessHost>>>,
    kiro_slots: Arc<tokio::sync::Semaphore>,
    kiro_processes: Mutex<Vec<Arc<crate::knowledge_kiro_process::KiroProcessOwner>>>,
    opencode: Option<OpenCodeProcesses>,
    shutdown: tokio::sync::Mutex<Option<HostOwner>>,
    remaining: Mutex<Vec<RetainedProcess>>,
    kimi: Mutex<Vec<Arc<graphit_ai::cli_kimi::KimiServer>>>,
    conversations: Mutex<Vec<Arc<crate::knowledge_conversation::ConversationCustodyEntry>>>,
    cleanup_failure: Mutex<Option<Cleanup>>,
}
impl AskService {
    pub(crate) fn program(&self) -> &Path {
        &self.program
    }

    pub(crate) fn is_copilot(&self) -> bool {
        self.provider == CliCompletionProvider::Copilot
    }
    pub(crate) fn copilot_limits(&self) -> graphit_ai::cli_copilot::CopilotLimits {
        graphit_ai::cli_copilot::CopilotLimits {
            wire: self.policy.wire,
            input_bytes: self.policy.process.stdin_bytes,
            output_bytes: self.policy.process.stdout_bytes,
            prompt_text_bytes: self.policy.question.max_prompt_bytes.get(),
            response_text_bytes: self.policy.completion.max_response_bytes,
        }
    }
    pub(crate) fn copilot_request(&self, cwd: &Path) -> Request {
        Request {
            program: self.program.clone(),
            args: Vec::new(),
            cwd: cwd.to_owned(),
            environment: self.environment.clone(),
            stdin: None,
            limits: self.policy.process,
        }
    }
    pub(crate) fn retain_copilot(&self, receipt: &mut graphit_ai::cli_copilot::CopilotReceipt) {
        self.retain_process(receipt.process_mut());
    }
    pub(crate) fn is_kimi(&self) -> bool {
        self.provider == CliCompletionProvider::Kimi
    }
    pub(crate) fn kimi_limits(&self) -> graphit_ai::cli_kimi::KimiLimits {
        crate::cli_launch_policy::CliLaunchPolicy {
            wire: self.policy.wire,
            completion: self.policy.completion,
            process: self.policy.process,
        }
        .kimi_limits(
            self.policy.question.max_prompt_bytes.get(),
            self.policy.completion.max_response_bytes,
        )
    }
    pub(crate) async fn start_kimi(
        &self,
        original: &Arc<graphit_core::file_io::SnapshotContext>,
        admission: &graphit_auth::ProjectReadAdmission<'_>,
        cwd: &Path,
        custody: Option<&crate::knowledge_conversation::ConversationCustodyEntry>,
    ) -> Result<
        Arc<graphit_ai::cli_kimi::KimiServer>,
        crate::knowledge_question::KnowledgeQuestionError,
    > {
        use crate::knowledge_question::{KnowledgeQuestionError, check_read};
        check_read(original, admission)?;
        let host = self.host().map_err(KnowledgeQuestionError::Request)?;
        let request = Request {
            program: self.program.clone(),
            args: [
                "web",
                "--no-open",
                "--host",
                "127.0.0.1",
                "--port",
                "0",
                "--log-level",
                "error",
            ]
            .into_iter()
            .map(OsString::from)
            .collect(),
            cwd: cwd.to_owned(),
            environment: self.environment.clone(),
            stdin: None,
            limits: self.policy.process,
        };
        let owner = graphit_ai::cli_kimi::spawn_kimi_server(
            original,
            &host,
            request,
            self.policy.wire.frame_bytes,
        )
        .await;
        if let Some(custody) = custody {
            custody.retain_kimi(Arc::clone(&owner));
        }
        self.kimi
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(Arc::clone(&owner));
        if self.stop.is_cancelled() {
            owner.stop();
            return Err(KnowledgeQuestionError::Request(
                io::ErrorKind::Interrupted.into(),
            ));
        }
        if let Err(error) = check_read(original, admission) {
            owner.stop();
            return Err(error);
        }
        Ok(owner)
    }
    async fn reap_kimi(&self, deadline: Instant) -> bool {
        let entries = self
            .kimi
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let mut done = Vec::new();
        let mut pending = false;
        for owner in entries {
            let observed = owner.observe(deadline).await;
            if observed.joined {
                if let Some(cleanup) = observed.cleanup {
                    self.conversation_cleanup(cleanup);
                }
                done.push(owner);
            } else {
                pending = true;
            }
        }
        self.kimi
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|owner| !done.iter().any(|joined| Arc::ptr_eq(owner, joined)));
        pending
    }
    pub(crate) fn is_opencode(&self) -> bool {
        self.provider == CliCompletionProvider::OpenCode
    }
    pub(crate) fn opencode_limits(&self) -> graphit_ai::cli_opencode::OpencodeLimits {
        graphit_ai::cli_opencode::OpencodeLimits {
            prompt_text_bytes: self.policy.question.max_prompt_bytes.get(),
            response_text_bytes: self.policy.completion.max_response_bytes,
            notifications: 65536,
            id_bytes: self.policy.wire.id_bytes,
        }
    }
    pub(crate) async fn start_opencode(
        &self,
        original: &Arc<graphit_core::file_io::SnapshotContext>,
        admission: &graphit_auth::ProjectReadAdmission<'_>,
        cwd: &Path,
        conversation: Option<&crate::knowledge_conversation::ConversationCustodyEntry>,
    ) -> Result<
        (
            crate::acp_process::AcpProcess,
            Arc<crate::acp_process::AcpProcessOwner>,
        ),
        crate::knowledge_question::KnowledgeQuestionError,
    > {
        use crate::knowledge_question::{KnowledgeQuestionError, check_read};
        let custody = self
            .opencode
            .as_ref()
            .ok_or_else(|| KnowledgeQuestionError::Request(io::ErrorKind::InvalidInput.into()))?;
        check_read(original, admission)?;
        let deadline = original
            .required_deadline()
            .map_err(KnowledgeQuestionError::Request)?;
        let permit = tokio::select! {
            result=custody.slots.clone().acquire_owned()=>result.map_err(|error|KnowledgeQuestionError::Request(io::Error::other(error)))?,
            ()=original.cancellation().cancelled()=>return Err(KnowledgeQuestionError::Request(io::ErrorKind::Interrupted.into())),
            ()=self.stop.cancelled()=>return Err(KnowledgeQuestionError::Request(io::ErrorKind::Interrupted.into())),
            ()=tokio::time::sleep_until(deadline.into())=>return Err(KnowledgeQuestionError::Request(io::ErrorKind::TimedOut.into())),
        };
        // Serialize the last stop check, spawn and registration with shutdown's
        // owner snapshot. No await separates these operations, across threads too.
        let mut owners = custody
            .owners
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.stop.is_cancelled() {
            return Err(KnowledgeQuestionError::Request(
                io::ErrorKind::Interrupted.into(),
            ));
        }
        check_read(original, admission)?;
        let (transport, owner) = crate::acp_process::spawn(crate::acp_process::AcpLaunch {
            program: &self.program,
            cwd,
            environment: &self.environment,
            original: Arc::clone(original),
            service_stop: self.stop.clone(),
            limits: self.policy.process,
            wire: self.policy.wire,
            permit,
        })
        .map_err(|failure| {
            if let Some(owner) = failure.owner {
                owners.push(Arc::clone(&owner));
                if let Some(conversation) = conversation {
                    conversation.retain_opencode(owner);
                }
            }
            KnowledgeQuestionError::Request(failure.error)
        })?;
        // Register the actual owner before the next await in the caller.
        owners.push(Arc::clone(&owner));
        if let Some(conversation) = conversation {
            conversation.retain_opencode(Arc::clone(&owner));
        }
        Ok((transport, owner))
    }
    pub(crate) async fn join_opencode(
        &self,
        owner: &crate::acp_process::AcpProcessOwner,
    ) -> io::Result<bool> {
        let deadline = Instant::now()
            .checked_add(self.policy.process.cleanup)
            .ok_or(io::ErrorKind::InvalidInput)?;
        // Let the stable native command finish normally after SDK EOF.
        Ok(owner.observe(deadline).await)
    }
    async fn reap_opencode(&self, deadline: Instant) -> bool {
        let Some(custody) = &self.opencode else {
            return false;
        };
        let entries = custody
            .owners
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let mut joined = Vec::new();
        let mut pending = false;
        for owner in entries {
            if owner.observe(deadline).await {
                joined.push(owner);
            } else {
                pending = true;
            }
        }
        custody
            .owners
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|owner| !joined.iter().any(|done| Arc::ptr_eq(owner, done)));
        pending
    }
    pub(crate) fn kiro_engine(&self) -> Option<graphit_ai::cli_kiro::KiroEngine> {
        use graphit_ai::cli_kiro::KiroEngine;
        match self.provider {
            CliCompletionProvider::KiroV3 => Some(KiroEngine::V3),
            _ => None,
        }
    }
    pub(crate) fn conversation_engine(
        &self,
    ) -> graphit_ai::knowledge_conversation::ConversationEngine {
        use graphit_ai::knowledge_conversation::ConversationEngine;
        match self.provider {
            CliCompletionProvider::Codex => ConversationEngine::Codex,
            CliCompletionProvider::Claude => ConversationEngine::Claude,
            CliCompletionProvider::Copilot => ConversationEngine::Copilot,
            CliCompletionProvider::OpenCode => ConversationEngine::OpenCode,
            CliCompletionProvider::Qwen => ConversationEngine::Qwen,
            CliCompletionProvider::Cursor => ConversationEngine::Cursor,
            CliCompletionProvider::Grok => ConversationEngine::Grok,
            CliCompletionProvider::Kimi => ConversationEngine::Kimi,
            CliCompletionProvider::Antigravity => ConversationEngine::Antigravity,
            CliCompletionProvider::KiroV3 => ConversationEngine::KiroV3,
        }
    }
    pub(crate) fn kiro_new_session(
        &self,
        cwd: &Path,
        model: &str,
    ) -> graphit_ai::cli_kiro::KiroSessionStart {
        let meta = matches!(
            self.kiro_engine(),
            Some(graphit_ai::cli_kiro::KiroEngine::V3)
        )
        .then(|| {
            serde_json::json!({"kiro":{"modelId":model}})
                .as_object()
                .expect("object metadata")
                .clone()
        });
        graphit_ai::cli_kiro::KiroSessionStart::New(
            agent_client_protocol::schema::v1::NewSessionRequest::new(cwd).meta(meta),
        )
    }
    pub(crate) fn kiro_prompt(
        &self,
        text: String,
        model: &str,
    ) -> graphit_ai::cli_kiro::KiroPrompt {
        use agent_client_protocol::schema::v1::{SessionConfigId, SessionConfigValueId};
        use graphit_ai::cli_kiro::{KiroConfigSelection, KiroEngine, KiroPrompt};
        KiroPrompt {
            text,
            selection: matches!(self.kiro_engine(), Some(KiroEngine::V3)).then(|| {
                KiroConfigSelection {
                    config_id: SessionConfigId::new("model"),
                    value_id: SessionConfigValueId::new(model),
                    meta: None,
                }
            }),
        }
    }
    pub(crate) fn kiro_limits(&self) -> graphit_ai::cli_kiro::KiroLimits {
        graphit_ai::cli_kiro::KiroLimits {
            prompt_text_bytes: self.policy.question.max_prompt_bytes.get(),
            response_text_bytes: self.policy.completion.max_response_bytes,
            notifications: 65536,
            id_bytes: self.policy.wire.id_bytes,
        }
    }
    pub(crate) async fn kiro_process(
        &self,
        cwd: &Path,
        original: &Arc<graphit_core::file_io::SnapshotContext>,
        custody: Option<Arc<crate::knowledge_conversation::ConversationCustodyEntry>>,
        allowed: &(dyn Fn() -> bool + Sync),
    ) -> io::Result<(
        crate::knowledge_kiro_process::KiroProcess,
        Arc<crate::knowledge_kiro_process::KiroProcessOwner>,
    )> {
        use crate::knowledge_kiro_process::{KiroProcess, KiroProcessOwner};
        let deadline = original.required_deadline()?;
        let permit = tokio::select! {
            result = self.kiro_slots.clone().acquire_owned() => result.map_err(io::Error::other)?,
            () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
            () = self.stop.cancelled() => return Err(io::ErrorKind::Interrupted.into()),
            () = tokio::time::sleep_until(deadline.into()) => return Err(io::ErrorKind::TimedOut.into()),
        };
        original.check()?;
        if !allowed() {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        if self.stop.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        self.kiro_engine().ok_or(io::ErrorKind::InvalidInput)?;
        let args: Vec<OsString> = vec!["acp".into(), "--agent-engine=v3".into()];
        let owner = KiroProcessOwner::new(custody);
        self.kiro_processes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Arc::clone(&owner));
        Ok((
            KiroProcess {
                program: self.program.clone(),
                args,
                cwd: cwd.to_owned(),
                environment: self.environment.clone(),
                original: Arc::clone(original),
                service_stop: self.stop.clone(),
                limits: self.policy.process,
                frame_bytes: self.policy.wire.frame_bytes,
                permit: Some(permit),
                owner: Arc::clone(&owner),
            },
            owner,
        ))
    }
    pub(crate) async fn join_kiro(
        &self,
        owner: &crate::knowledge_kiro_process::KiroProcessOwner,
    ) -> io::Result<bool> {
        let deadline = Instant::now()
            .checked_add(self.policy.process.cleanup)
            .ok_or(io::ErrorKind::InvalidInput)?;
        owner.stop();
        Ok(owner.observe(deadline).await)
    }
    async fn reap_kiro(&self, deadline: Instant) -> bool {
        let entries = self
            .kiro_processes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let mut done = Vec::new();
        let mut pending = false;
        for owner in entries {
            if owner.observe(deadline).await {
                done.push(owner);
            } else {
                pending = true;
            }
        }
        self.kiro_processes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|owner| !done.iter().any(|other| Arc::ptr_eq(owner, other)));
        pending
    }
    pub(crate) fn hold_conversation_turn(
        &self,
        turn: graphit_ai::knowledge_conversation::ConversationTurn,
    ) -> Arc<crate::knowledge_conversation::ConversationCustodyEntry> {
        let entry = Arc::new(crate::knowledge_conversation::ConversationCustodyEntry::new(turn));
        self.conversations
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Arc::clone(&entry));
        entry
    }
    fn conversation_cleanup(&self, cleanup: Cleanup) {
        if cleanup != Cleanup::NotSpawned && !cleanup.leader_reaped_without_control_failure() {
            self.cleanup_failure
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_or_insert(cleanup);
        }
    }
    async fn reap_conversations(&self, deadline: Instant, host_joined: bool) -> bool {
        let entries = self
            .conversations
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let mut pending = false;
        for entry in entries {
            if host_joined {
                entry.host_joined();
            }
            if entry.opencode_owner().is_some() && !entry.observe_opencode(deadline).await {
                pending = true;
            }
            if let Some(owner) = entry.kimi_server() {
                let observed = owner.observe(deadline).await;
                if observed.joined {
                    if let Some(cleanup) = observed.cleanup {
                        self.conversation_cleanup(cleanup);
                    }
                    entry.observed_kimi(&observed);
                } else {
                    pending = true;
                }
            }
            if let Some(record) = entry.remaining() {
                let Ok(mut process) = record.try_lock() else {
                    pending = true;
                    continue;
                };
                match process.repair(deadline).await {
                    RepairObservation::Observed { cleanup, .. } => {
                        self.conversation_cleanup(cleanup);
                        entry.observed_terminal();
                    }
                    RepairObservation::Pending { .. } => pending = true,
                }
            } else if !entry.released() {
                pending = true;
            }
        }
        self.conversations
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|entry| !entry.released());
        pending
    }

    pub fn host(&self) -> io::Result<Arc<ProcessHost>> {
        if self.stop.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        self.host
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .cloned()
            .ok_or_else(|| io::ErrorKind::BrokenPipe.into())
    }
    pub(crate) fn is_claude(&self) -> bool {
        self.provider == CliCompletionProvider::Claude
    }
    pub(crate) fn claude_request(&self, cwd: &Path, model: &str, prompt: String) -> Request {
        Request {
            program: self.program.clone(),
            args: [
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--include-partial-messages",
                "--model",
                model,
            ]
            .into_iter()
            .map(OsString::from)
            .collect(),
            cwd: cwd.to_owned(),
            environment: self.environment.clone(),
            stdin: Some(prompt.into_bytes().into()),
            limits: self.policy.process,
        }
    }
    pub(crate) fn claude_limits(&self) -> ClaudeLimits {
        ClaudeLimits {
            wire: self.policy.wire,
            stdout_bytes: self.policy.process.stdout_bytes,
            stderr_bytes: self.policy.process.stderr_bytes,
            frames: 65536,
            response_bytes: self.policy.completion.max_response_bytes,
        }
    }
    pub(crate) fn retain_claude(&self, receipt: &mut ClaudeReceipt) {
        self.retain_process(receipt.process_mut());
    }
    pub(crate) fn is_cursor(&self) -> bool {
        self.provider == CliCompletionProvider::Cursor
    }
    pub(crate) fn cursor_request(&self, cwd: &Path, model: &str, prompt: String) -> Request {
        let mut args: Vec<OsString> = [
            "--print",
            "--output-format",
            "stream-json",
            "--model",
            model,
            "--mode",
            "ask",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        args.push(prompt.into());
        Request {
            program: self.program.clone(),
            args,
            cwd: cwd.to_owned(),
            environment: self.environment.clone(),
            stdin: None,
            limits: self.policy.process,
        }
    }
    pub(crate) fn cursor_limits(&self) -> CursorLimits {
        CursorLimits {
            wire: self.policy.wire,
            stdout_bytes: self.policy.process.stdout_bytes,
            stderr_bytes: self.policy.process.stderr_bytes,
            frames: 65536,
            response_bytes: self.policy.completion.max_response_bytes,
        }
    }
    pub(crate) fn retain_cursor(&self, receipt: &mut CursorReceipt) {
        self.retain_process(receipt.process_mut());
    }
    pub(crate) fn is_grok(&self) -> bool {
        self.provider == CliCompletionProvider::Grok
    }
    pub(crate) fn grok_request(&self, cwd: &Path, model: &str, prompt_file: &Path) -> Request {
        Request {
            program: self.program.clone(),
            args: vec![
                "--prompt-file".into(),
                prompt_file.as_os_str().to_owned(),
                "--output-format".into(),
                "streaming-json".into(),
                "--model".into(),
                model.into(),
                "--verbatim".into(),
                "--deny".into(),
                "*".into(),
            ],
            cwd: cwd.to_path_buf(),
            environment: self.environment.clone(),
            stdin: None,
            limits: self.policy.process,
        }
    }
    pub(crate) fn grok_limits(&self) -> GrokLimits {
        GrokLimits {
            wire: self.policy.wire,
            stdout_bytes: self.policy.process.stdout_bytes,
            stderr_bytes: self.policy.process.stderr_bytes,
            frames: 65536,
            response_bytes: self.policy.completion.max_response_bytes,
        }
    }
    pub(crate) fn retain_grok(&self, receipt: &mut GrokReceipt, prompt_file: tempfile::TempPath) {
        let remaining = match receipt.process_mut() {
            Some(Ok(process)) => process.remaining.take(),
            Some(Err(process)) => process.remaining.take(),
            None => None,
        };
        if let Some(remaining) = remaining {
            self.remaining
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(RetainedProcess {
                    process: Arc::new(tokio::sync::Mutex::new(remaining)),
                    _prompt_file: Some(prompt_file),
                });
        }
    }
    pub(crate) fn is_antigravity(&self) -> bool {
        self.provider == CliCompletionProvider::Antigravity
    }
    pub(crate) fn antigravity_request(
        &self,
        cwd: &Path,
        model: &str,
        prompt: String,
    ) -> io::Result<Request> {
        let input = graphit_ai::cli_antigravity_wire::encode_antigravity_prompt(&prompt)
            .map_err(io::Error::other)?;
        Ok(Request {
            program: self.program.clone(),
            args: [
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--model",
                model,
            ]
            .into_iter()
            .map(OsString::from)
            .collect(),
            cwd: cwd.to_path_buf(),
            environment: self.environment.clone(),
            stdin: Some(input.into()),
            limits: self.policy.process,
        })
    }
    pub(crate) fn antigravity_limits(&self) -> AntigravityLimits {
        AntigravityLimits {
            wire: self.policy.wire,
            stdout_bytes: self.policy.process.stdout_bytes,
            stderr_bytes: self.policy.process.stderr_bytes,
            frames: 65536,
            response_bytes: self.policy.completion.max_response_bytes,
        }
    }
    pub(crate) fn retain_antigravity(&self, receipt: &mut AntigravityReceipt) {
        self.retain_process(receipt.process_mut());
    }
    pub(crate) fn is_qwen(&self) -> bool {
        self.provider == CliCompletionProvider::Qwen
    }
    pub(crate) fn qwen_request(&self, cwd: &Path, model: &str, prompt: String) -> Request {
        Request {
            program: self.program.clone(),
            args: [
                "--input-format",
                "text",
                "--output-format",
                "stream-json",
                "--include-partial-messages",
                "--model",
                model,
            ]
            .into_iter()
            .map(OsString::from)
            .collect(),
            cwd: cwd.to_owned(),
            environment: self.environment.clone(),
            stdin: Some(prompt.into_bytes().into()),
            limits: self.policy.process,
        }
    }
    pub(crate) fn qwen_limits(&self) -> QwenLimits {
        QwenLimits {
            wire: self.policy.wire,
            stdout_bytes: self.policy.process.stdout_bytes,
            stderr_bytes: self.policy.process.stderr_bytes,
            frames: 65536,
            response_bytes: self.policy.completion.max_response_bytes,
        }
    }
    pub(crate) fn retain_qwen(&self, receipt: &mut QwenReceipt) {
        self.retain_process(receipt.process_mut());
    }
    pub async fn ready(&self, original: &RequestContext<'_>) -> io::Result<()> {
        let deadline = original.snapshot().required_deadline()?;
        if self.kiro_engine().is_some() || self.is_opencode() {
            original.snapshot().check()?;
            return if self.stop.is_cancelled() {
                Err(io::ErrorKind::Interrupted.into())
            } else {
                Ok(())
            };
        }
        let host = self.host()?;
        let observed = tokio::select! {
            observed = host.ready(deadline) => observed,
            () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
        };
        original.snapshot().check()?;
        match observed {
            HostReadiness::Ready => Ok(()),
            HostReadiness::Pending => Err(io::ErrorKind::TimedOut.into()),
            HostReadiness::Failed(error) => Err(io::Error::other(format!(
                "Knowledge Ask host readiness: {error:?}"
            ))),
            HostReadiness::Sealed => Err(io::ErrorKind::BrokenPipe.into()),
        }
    }
    pub fn request(&self, cwd: &Path, model: &str) -> Request {
        Request {
            program: self.program.clone(),
            args: ["exec", "--json", "--model", model, "-"]
                .into_iter()
                .map(OsString::from)
                .collect(),
            cwd: cwd.to_owned(),
            environment: self.environment.clone(),
            stdin: None,
            limits: self.policy.process,
        }
    }
    /// Transfer immediately, before any await can drop the borrowed receipt.
    pub fn retain(&self, receipt: &mut CodexReceipt<'_>) {
        self.retain_process(receipt.process_mut());
    }
    fn retain_process(
        &self,
        process: Option<&mut Result<graphit_process::Outcome, graphit_process::Failure>>,
    ) {
        let remaining = match process {
            Some(Ok(process)) => process.remaining.take(),
            Some(Err(process)) => process.remaining.take(),
            None => None,
        };
        if let Some(remaining) = remaining {
            self.remaining
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(RetainedProcess {
                    process: Arc::new(tokio::sync::Mutex::new(remaining)),
                    _prompt_file: None,
                });
        }
    }
    pub fn stop(&self) {
        self.stop.cancel();
    }
    fn observed(&self, record: &Arc<tokio::sync::Mutex<RemainingProcess>>, cleanup: Cleanup) {
        if !matches!(cleanup, Cleanup::Complete | Cleanup::NotSpawned) {
            self.cleanup_failure
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .get_or_insert(cleanup);
        }
        self.remaining
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|retained| !Arc::ptr_eq(&retained.process, record));
    }
    /// One observation pass over existing owners, without a wait queue or polling.
    /// An immediate repair bound observes completed records and preserves pending ones.
    pub async fn reap(&self) {
        self.reap_opencode(Instant::now()).await;
        self.reap_kimi(Instant::now()).await;
        self.reap_kiro(Instant::now()).await;
        self.reap_conversations(Instant::now(), false).await;
        let records = self
            .remaining
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .map(|record| Arc::clone(&record.process))
            .collect::<Vec<_>>();
        for record in records {
            if let Ok(mut process) = record.try_lock()
                && let RepairObservation::Observed { cleanup, .. } =
                    process.repair(Instant::now()).await
            {
                self.observed(&record, cleanup);
            }
        }
    }
    /// Waiters borrow durable owners. Cancelling a waiter never aborts the Host join.
    pub async fn shutdown(&self) -> io::Result<()> {
        self.stop();
        let deadline = Instant::now()
            .checked_add(self.policy.process.cleanup)
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        if self.is_opencode() {
            let pending = self.reap_opencode(deadline).await;
            let conversation_pending = self.reap_conversations(deadline, false).await;
            return if pending || conversation_pending {
                Err(io::ErrorKind::TimedOut.into())
            } else {
                Ok(())
            };
        }
        if self.kiro_engine().is_some() {
            let entries = self
                .kiro_processes
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            for owner in entries {
                owner.stop();
            }
            let pending = self.reap_kiro(deadline).await;
            let conversation_pending = self.reap_conversations(deadline, false).await;
            return if pending || conversation_pending {
                Err(io::ErrorKind::TimedOut.into())
            } else {
                Ok(())
            };
        }
        let mut owner = self.shutdown.lock().await;
        if owner.is_none()
            && let Some(host) = self
                .host
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .take()
        {
            *owner = Some(HostOwner::Live(host));
        }
        if matches!(owner.as_ref(), Some(HostOwner::Live(_))) {
            let Some(HostOwner::Live(host)) = owner.take() else {
                unreachable!()
            };
            match Arc::try_unwrap(host) {
                Ok(host) => {
                    *owner = Some(HostOwner::Joining(tokio::spawn(host.shutdown(deadline))))
                }
                Err(host) => {
                    *owner = Some(HostOwner::Live(host));
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "Knowledge Ask Host still borrowed by native jobs",
                    ));
                }
            }
        }
        if let Some(HostOwner::Joining(join)) = owner.as_mut() {
            let result = join.await;
            *owner = Some(match result {
                Err(error) => HostOwner::JoinFailed(Some(error)),
                Ok(result) => match result {
                    HostShutdown::Joined(report) => HostOwner::Joined(Some(report)),
                    HostShutdown::Pending(pending) => HostOwner::Pending(pending),
                },
            });
        }
        if let Some(HostOwner::Pending(pending)) = owner.as_mut()
            && let HostShutdownObservation::Joined(report) = pending.repair(deadline).await
        {
            *owner = Some(HostOwner::Joined(Some(report)));
        }
        let records = self
            .remaining
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .map(|record| Arc::clone(&record.process))
            .collect::<Vec<_>>();
        let mut pending = false;
        for record in records {
            let mut process = record.lock().await;
            match process.repair(deadline).await {
                RepairObservation::Pending { .. } => pending = true,
                RepairObservation::Observed { cleanup, .. } => {
                    self.observed(&record, cleanup);
                }
            }
        }
        let joined = matches!(owner.as_ref(), Some(HostOwner::Joined(_)));
        pending |= self.reap_conversations(deadline, joined).await;
        pending |= self.reap_kimi(deadline).await;
        // Keep terminal observations until every awaited repair has returned.
        // Consuming the report preserves the drained phase for the next caller.
        let host = match owner.as_mut() {
            Some(HostOwner::Joined(report)) => match report.take().and_then(|r| r.failure()) {
                Some(error) => Err(io::Error::other(format!(
                    "Knowledge Ask Host join: {error:?}"
                ))),
                None => Ok(()),
            },
            Some(HostOwner::JoinFailed(error)) => match error.take() {
                Some(error) => Err(io::Error::other(error)),
                None => Ok(()),
            },
            _ => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Knowledge Ask Host join pending",
            )),
        };
        host?;
        if pending {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Knowledge Ask Process repair pending",
            ));
        }
        if let Some(cleanup) = self
            .cleanup_failure
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            return Err(io::Error::other(format!(
                "Knowledge Ask Process cleanup: {cleanup:?}"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    include!("knowledge_ask_service_copilot_tests.rs");
    #[test]
    fn cli_capture_separates_hosts_and_keeps_an_explicit_program_path() {
        for host in NativeBinding::ALL {
            let (provider, program) = captured_cli(host, None).unwrap();
            assert_eq!(program, OsStr::new(provider.executable()));
            assert_eq!(provider.as_str(), host.as_str());
        }
        let selected = Path::new("directory with spaces").join("copilot");
        let (provider, program) = captured_cli(NativeBinding::Codex, selected.to_str()).unwrap();
        assert_eq!(provider, CliCompletionProvider::Copilot);
        assert_eq!(program, selected.as_os_str());
        for value in ["", " ", "unknown", "copilot --help", "copilot\0private"] {
            assert_eq!(
                captured_cli(NativeBinding::Codex, Some(value))
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn missing_explicit_program_retains_resolver_failure_without_host_fallback() {
        let directory = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/usr/bin/true", directory.path().join("codex")).unwrap();
        let selected = directory.path().join("missing").join("copilot");
        let available = AskAvailability::capture(
            CliCompletionProvider::Copilot,
            selected.as_os_str(),
            vec![("PATH".into(), directory.path().as_os_str().to_owned())],
            directory.path(),
            NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap(),
        )
        .unwrap();
        assert!(matches!(available, AskAvailability::Unavailable(_)));
    }

    #[cfg(unix)]
    use graphit_core::file_io::SnapshotContext;

    #[cfg(unix)]
    fn conversation_fixture(
        original: &Arc<SnapshotContext>,
        program: &Path,
    ) -> (
        tempfile::TempDir,
        graphit_core::artifact_store::CapturedPublicationScope,
        graphit_ai::knowledge_conversation::ConversationTurn,
    ) {
        conversation_fixture_engine(
            original,
            program,
            graphit_ai::knowledge_conversation::ConversationEngine::Codex,
        )
    }
    #[cfg(unix)]
    pub(super) fn conversation_fixture_engine(
        original: &Arc<SnapshotContext>,
        program: &Path,
        engine: graphit_ai::knowledge_conversation::ConversationEngine,
    ) -> (
        tempfile::TempDir,
        graphit_core::artifact_store::CapturedPublicationScope,
        graphit_ai::knowledge_conversation::ConversationTurn,
    ) {
        use graphit_ai::knowledge_conversation::*;
        use graphit_core::{
            artifact_store::{
                ArtifactLimits, CapturedPublicationScope, CopyLimits, ScopeLimits, UserIdentity,
            },
            brand::Brand,
            installation_identity::InstallationId,
            project_paths::InstallationPaths,
        };
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        std::fs::create_dir_all(paths.root()).unwrap();
        std::fs::set_permissions(paths.root(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let scope = CapturedPublicationScope::user(
            &paths,
            InstallationId::new("app-custody-fixture").unwrap(),
            UserIdentity::new("https://issuer.example".into(), "verified-fixture".into()).unwrap(),
            ScopeLimits::new(1, 16384, 1024).unwrap(),
            original,
        )
        .unwrap();
        let copy = CopyLimits::new(131072, 128, 16384, 8).unwrap();
        let store = KnowledgeConversationStore::capture(
            scope.clone(),
            Arc::clone(original),
            ConversationLimits {
                artifacts: ArtifactLimits::new(262144, 4096, copy).unwrap(),
                native: NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap(),
                record_bytes: NonZeroUsize::new(65536).unwrap(),
                field_bytes: NonZeroUsize::new(4096).unwrap(),
                sources: NonZeroUsize::new(128).unwrap(),
                list_records: NonZeroUsize::new(128).unwrap(),
            },
        )
        .unwrap();
        let turn = store
            .begin_create(
                "private-custody",
                ConversationBinding {
                    provider: match engine {
                        ConversationEngine::Codex => "codex",
                        ConversationEngine::Claude => "claude",
                        ConversationEngine::Qwen => "qwen",
                        ConversationEngine::Cursor => "cursor",
                        ConversationEngine::Grok => "grok",
                        ConversationEngine::Kimi => "kimi",
                        ConversationEngine::Antigravity => "antigravity",
                        ConversationEngine::Copilot => "copilot",
                        ConversationEngine::OpenCode => "opencode",
                        ConversationEngine::KiroV3 => "kiro",
                    }
                    .into(),
                    engine,
                    model: "fixture-model".into(),
                    program: program.to_owned(),
                    project_id: "fixture-project".into(),
                    project_version: "fixture-version".into(),
                    sources: Vec::new(),
                },
                original,
            )
            .unwrap();
        (temp, scope, turn)
    }
    #[cfg(unix)]
    pub(super) fn assert_writer_held(
        scope: &graphit_core::artifact_store::CapturedPublicationScope,
    ) {
        use graphit_core::file_lock::FileLock;
        let path = scope.runtime_root().join("artifact-store.lock");
        assert!(
            FileLock::is_locked(&path).unwrap(),
            "real USER writer must remain held"
        );
    }
    #[tokio::test]
    #[cfg(unix)]
    async fn conversation_guard_survives_internal_codex_future_drop_until_actual_host_join() {
        use graphit_ai::{
            cli_codex::{CodexSink, CodexStreamLimits, run_codex},
            cli_codex_wire::CodexEventView,
            codex_completion::{AttemptPurpose, CodexCompletionRun},
        };
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let program = temp.path().join("codex");
        std::fs::write(&program,"#!/usr/bin/python3\nimport json,time\nprint(json.dumps({'type':'thread.started','thread_id':'private-fixture-id'}),flush=True)\ntime.sleep(10)\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let native = NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap();
        let availability = AskAvailability::capture(
            CliCompletionProvider::Codex,
            OsStr::new("codex"),
            vec![("PATH".into(), temp.path().as_os_str().to_owned())],
            temp.path(),
            native,
        )
        .unwrap();
        let service = availability.service().unwrap();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        let (_user, scope, turn) = conversation_fixture(&original, service.program());
        let entry = service.hold_conversation_turn(turn);
        let host = service.host().unwrap();
        assert!(matches!(
            host.ready(original.required_deadline().unwrap()).await,
            HostReadiness::Ready
        ));
        let started = tokio::sync::Notify::new();
        struct Observe<'a> {
            started: &'a tokio::sync::Notify,
            entry: Arc<crate::knowledge_conversation::ConversationCustodyEntry>,
            original: &'a Arc<SnapshotContext>,
        }
        impl CodexSink for Observe<'_> {
            async fn push<'a>(
                &'a mut self,
                event: CodexEventView<'a>,
                _: &'a SnapshotContext,
            ) -> Result<(), graphit_process::SinkFailure> {
                if let CodexEventView::ThreadStarted { thread_id } = event {
                    assert!(self.entry.observe(thread_id, self.original));
                    self.started.notify_one();
                }
                Ok(())
            }
        }
        let mut sink = Observe {
            started: &started,
            entry: Arc::clone(&entry),
            original: &original,
        };
        let mut run = CodexCompletionRun::new(
            &original,
            Some("fixture-model"),
            &service.policy.wire,
            service.policy.completion,
        )
        .unwrap();
        let request = service.request(temp.path(), "fixture-model");
        let mut execution = Box::pin(run_codex(
            &mut run,
            &host,
            request,
            AttemptPurpose::Request,
            None,
            CodexStreamLimits {
                stdout_bytes: 65536,
                stderr_bytes: 65536,
            },
            &mut sink,
        ));
        tokio::select! { ()=started.notified()=>(), _=&mut execution=>panic!("transport unexpectedly returned before controlled drop") }
        drop(execution);
        drop(host);
        assert!(entry.remaining().is_none());
        assert!(!entry.released());
        assert_writer_held(&scope);
        service.reap().await;
        assert!(!entry.released());
        assert_writer_held(&scope);
        service.shutdown().await.unwrap();
        assert!(entry.released());
        assert!(service.conversations.lock().unwrap().is_empty());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn busy_host_pre_submit_failure_releases_actual_user_writer_without_receipt_fabrication()
    {
        use graphit_ai::{
            cli_codex::{CodexReason, CodexSink, CodexStreamLimits, run_codex},
            cli_codex_wire::CodexEventView,
            codex_completion::{AttemptPurpose, CodexCompletionRun},
        };
        struct Ignore;
        impl CodexSink for Ignore {
            async fn push<'a>(
                &'a mut self,
                _: CodexEventView<'a>,
                _: &'a SnapshotContext,
            ) -> Result<(), graphit_process::SinkFailure> {
                panic!("pre-submit busy must not emit a native event")
            }
        }
        let native = NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap();
        let stop = Cancellation::default();
        let host = ProcessHost::install(&stop, HostLimits::new(2, 2).unwrap()).unwrap();
        assert!(matches!(
            host.ready(Instant::now() + Duration::from_secs(5)).await,
            HostReadiness::Ready
        ));
        let service = AskService {
            program: "/usr/bin/python3".into(),
            provider: CliCompletionProvider::Codex,
            kiro_slots: Arc::new(tokio::sync::Semaphore::new(2)),
            kiro_processes: Mutex::new(Vec::new()),
            opencode: None,
            environment: Vec::new(),
            policy: AskLaunchPolicy::capture(native),
            stop,
            host: Mutex::new(Some(Arc::new(host))),
            shutdown: tokio::sync::Mutex::new(None),
            remaining: Mutex::new(Vec::new()),
            kimi: Mutex::new(Vec::new()),
            conversations: Mutex::new(Vec::new()),
            cleanup_failure: Mutex::new(None),
        };
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        let (_user, scope, turn) = conversation_fixture(&original, service.program());
        let entry = service.hold_conversation_turn(turn);
        let host = service.host().unwrap();
        let (first_stop, first) = graphit_process::operation(&original, &host).unwrap();
        let (second_stop, second) = graphit_process::operation(&original, &host).unwrap();
        assert_writer_held(&scope);
        let mut run = CodexCompletionRun::new(
            &original,
            Some("fixture-model"),
            &service.policy.wire,
            service.policy.completion,
        )
        .unwrap();
        let mut failure = run_codex(
            &mut run,
            &host,
            service.request(scope.user_root().unwrap(), "fixture-model"),
            AttemptPurpose::Request,
            None,
            CodexStreamLimits {
                stdout_bytes: 65536,
                stderr_bytes: 65536,
            },
            &mut Ignore,
        )
        .await
        .expect_err("actual host capacity denial");
        assert!(matches!(failure.reason(), CodexReason::Admission(_)));
        assert!(failure.receipt().process().is_none());
        entry.retain(failure.receipt_mut());
        entry.release_pre_submit_failure(&failure);
        assert!(entry.released());
        assert!(matches!(
            entry.finish(&original, true),
            crate::knowledge_conversation::ConversationPersistence::NotStarted
        ));
        assert!(
            !graphit_core::file_lock::FileLock::is_locked(
                scope.runtime_root().join("artifact-store.lock")
            )
            .unwrap()
        );
        assert!(
            failure.receipt().process().is_none(),
            "do not invent NotSpawned receipt"
        );
        drop(first);
        drop(first_stop);
        drop(second);
        drop(second_stop);
        drop(host);
        service.shutdown().await.unwrap();
    }
    #[tokio::test]
    #[cfg(unix)]
    async fn claude_busy_host_pre_submit_failure_releases_writer_without_receipt_fabrication() {
        use graphit_ai::{
            cli_claude::{ClaudeReason, ClaudeSink, run_claude},
            cli_claude_wire::ClaudeEventView,
        };
        struct Ignore;
        impl ClaudeSink for Ignore {
            async fn push<'a>(
                &'a mut self,
                _: ClaudeEventView<'a>,
                _: &'a SnapshotContext,
            ) -> Result<(), graphit_process::SinkFailure> {
                panic!("pre-submit busy must not emit a native event")
            }
        }
        let native = NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap();
        let stop = Cancellation::default();
        let host = ProcessHost::install(&stop, HostLimits::new(2, 2).unwrap()).unwrap();
        assert!(matches!(
            host.ready(Instant::now() + Duration::from_secs(5)).await,
            HostReadiness::Ready
        ));
        let service = AskService {
            program: "/usr/bin/python3".into(),
            provider: CliCompletionProvider::Claude,
            kiro_slots: Arc::new(tokio::sync::Semaphore::new(2)),
            kiro_processes: Mutex::new(Vec::new()),
            opencode: None,
            environment: Vec::new(),
            policy: AskLaunchPolicy::capture(native),
            stop,
            host: Mutex::new(Some(Arc::new(host))),
            shutdown: tokio::sync::Mutex::new(None),
            remaining: Mutex::new(Vec::new()),
            kimi: Mutex::new(Vec::new()),
            conversations: Mutex::new(Vec::new()),
            cleanup_failure: Mutex::new(None),
        };
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        let (_user, scope, turn) = conversation_fixture_engine(
            &original,
            service.program(),
            graphit_ai::knowledge_conversation::ConversationEngine::Claude,
        );
        let entry = service.hold_conversation_turn(turn);
        let host = service.host().unwrap();
        let (first_stop, first) = graphit_process::operation(&original, &host).unwrap();
        let (second_stop, second) = graphit_process::operation(&original, &host).unwrap();
        assert_writer_held(&scope);
        let mut request = service.claude_request(
            scope.user_root().unwrap(),
            "fixture-model",
            "fixture prompt".into(),
        );
        entry.configure_claude(&mut request, &original).unwrap();
        let mut failure = run_claude(
            &original,
            &host,
            request,
            service.claude_limits(),
            &mut Ignore,
        )
        .await
        .expect_err("actual host capacity denial");
        assert!(matches!(failure.reason(), ClaudeReason::Admission(_)));
        assert!(failure.receipt().process().is_none());
        entry.retain_claude(failure.receipt_mut());
        entry.release_pre_submit_claude_failure(&failure);
        assert!(entry.released());
        assert!(matches!(
            entry.finish(&original, true),
            crate::knowledge_conversation::ConversationPersistence::NotStarted
        ));
        assert!(
            !graphit_core::file_lock::FileLock::is_locked(
                scope.runtime_root().join("artifact-store.lock")
            )
            .unwrap()
        );
        assert!(
            failure.receipt().process().is_none(),
            "do not invent NotSpawned receipt"
        );
        drop(first);
        drop(first_stop);
        drop(second);
        drop(second_stop);
        drop(host);
        service.shutdown().await.unwrap();
    }
    #[tokio::test]
    #[cfg(unix)]
    async fn claude_conversation_guard_survives_future_drop_until_actual_host_join() {
        use graphit_ai::{
            cli_claude::{ClaudeSink, run_claude},
            cli_claude_wire::{ClaudeEventData, ClaudeEventView},
        };
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let program = temp.path().join("claude");
        std::fs::write(&program,"#!/usr/bin/python3\nimport json,time\nprint(json.dumps({'type':'system','subtype':'init','session_id':'private-fixture-id','model':'fixture-model'}),flush=True)\ntime.sleep(10)\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let native = NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap();
        let availability = AskAvailability::capture(
            CliCompletionProvider::Claude,
            OsStr::new("claude"),
            vec![("PATH".into(), temp.path().as_os_str().to_owned())],
            temp.path(),
            native,
        )
        .unwrap();
        let service = availability.service().unwrap();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        let (_user, scope, turn) = conversation_fixture_engine(
            &original,
            service.program(),
            graphit_ai::knowledge_conversation::ConversationEngine::Claude,
        );
        let entry = service.hold_conversation_turn(turn);
        let host = service.host().unwrap();
        assert!(matches!(
            host.ready(original.required_deadline().unwrap()).await,
            HostReadiness::Ready
        ));
        let started = tokio::sync::Notify::new();
        struct Observe<'a> {
            started: &'a tokio::sync::Notify,
            entry: Arc<crate::knowledge_conversation::ConversationCustodyEntry>,
            original: &'a Arc<SnapshotContext>,
        }
        impl ClaudeSink for Observe<'_> {
            async fn push<'a>(
                &'a mut self,
                event: ClaudeEventView<'a>,
                _: &'a SnapshotContext,
            ) -> Result<(), graphit_process::SinkFailure> {
                if let ClaudeEventData::Init(init) = &event.data {
                    assert!(self.entry.observe_claude(&init.session_id, self.original));
                    self.started.notify_one();
                }
                Ok(())
            }
        }
        let mut sink = Observe {
            started: &started,
            entry: Arc::clone(&entry),
            original: &original,
        };
        let mut decision =
            crate::knowledge_conversation::ConversationDecisionGuard::new(Arc::clone(&entry));
        let mut request =
            service.claude_request(temp.path(), "fixture-model", "fixture prompt".into());
        entry.configure_claude(&mut request, &original).unwrap();
        decision.submitted();
        let mut execution = Box::pin(run_claude(
            &original,
            &host,
            request,
            service.claude_limits(),
            &mut sink,
        ));
        tokio::select! { ()=started.notified()=>(), _=&mut execution=>panic!("transport unexpectedly returned before controlled drop") }
        drop(execution);
        drop(decision);
        drop(host);
        assert!(entry.remaining().is_none());
        assert!(!entry.released());
        assert_writer_held(&scope);
        service.reap().await;
        assert!(!entry.released());
        assert_writer_held(&scope);
        service.shutdown().await.unwrap();
        assert!(entry.released());
        assert!(service.conversations.lock().unwrap().is_empty());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn claude_remaining_receipt_keeps_writer_until_real_repair_observed() {
        use graphit_ai::{
            cli_claude::{ClaudeSink, run_claude},
            cli_claude_wire::{ClaudeEventData, ClaudeEventView},
        };
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let program = temp.path().join("claude");
        std::fs::write(&program, "#!/usr/bin/python3\nimport json,time,signal\nsignal.signal(signal.SIGTERM,signal.SIG_IGN)\nprint(json.dumps({'type':'system','subtype':'init','session_id':'remaining-fixture-id','model':'fixture-model'}),flush=True)\ntime.sleep(10)\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let availability = AskAvailability::capture(
            CliCompletionProvider::Claude,
            OsStr::new("claude"),
            vec![("PATH".into(), temp.path().as_os_str().to_owned())],
            temp.path(),
            NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap(),
        )
        .unwrap();
        let service = availability.service().unwrap();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(10),
            Cancellation::default(),
        ));
        let (_user, scope, turn) = conversation_fixture_engine(
            &original,
            service.program(),
            graphit_ai::knowledge_conversation::ConversationEngine::Claude,
        );
        let entry = service.hold_conversation_turn(turn);
        let mut decision =
            crate::knowledge_conversation::ConversationDecisionGuard::new(Arc::clone(&entry));
        let host = service.host().unwrap();
        assert!(matches!(
            host.ready(original.required_deadline().unwrap()).await,
            HostReadiness::Ready
        ));
        struct CancelAfterInit<'a> {
            entry: Arc<crate::knowledge_conversation::ConversationCustodyEntry>,
            original: &'a Arc<SnapshotContext>,
        }
        impl ClaudeSink for CancelAfterInit<'_> {
            async fn push<'a>(
                &'a mut self,
                event: ClaudeEventView<'a>,
                original: &'a SnapshotContext,
            ) -> Result<(), graphit_process::SinkFailure> {
                if let ClaudeEventData::Init(init) = &event.data {
                    assert!(self.entry.observe_claude(&init.session_id, self.original));
                    original.cancellation().cancel();
                }
                Ok(())
            }
        }
        let mut sink = CancelAfterInit {
            entry: Arc::clone(&entry),
            original: &original,
        };
        let mut request =
            service.claude_request(temp.path(), "fixture-model", "fixture prompt".into());
        request.limits.cleanup = Duration::from_nanos(2);
        request.limits.termination_grace = Duration::from_nanos(1);
        entry.configure_claude(&mut request, &original).unwrap();
        decision.submitted();
        let mut failure = run_claude(
            &original,
            &host,
            request,
            service.claude_limits(),
            &mut sink,
        )
        .await
        .expect_err("actual cancellation after Init");
        assert!(
            match failure.receipt().process() {
                Some(Ok(p)) => p.remaining.is_some(),
                Some(Err(p)) => p.remaining.is_some(),
                None => false,
            },
            "actual producer must return native Remaining"
        );
        entry.retain_claude(failure.receipt_mut());
        assert!(entry.remaining().is_some());
        assert!(matches!(
            entry.finish(&original, false),
            crate::knowledge_conversation::ConversationPersistence::Denied
        ));
        decision.decided();
        assert_writer_held(&scope);
        let retained = entry.remaining().unwrap();
        let mut process = retained.lock().await;
        assert!(matches!(
            process.repair(Instant::now()).await,
            RepairObservation::Pending { .. }
        ));
        assert_writer_held(&scope);
        assert!(!entry.released());
        assert!(matches!(
            process
                .repair(Instant::now() + Duration::from_secs(5))
                .await,
            RepairObservation::Observed { .. }
        ));
        entry.observed_terminal();
        drop(process);
        assert!(entry.released());
        assert!(
            !graphit_core::file_lock::FileLock::is_locked(
                scope.runtime_root().join("artifact-store.lock")
            )
            .unwrap()
        );
        drop(host);
        service.reap().await;
        service.shutdown().await.unwrap();
    }

    #[cfg(unix)]
    async fn paused_host_join(
        fail: bool,
    ) -> (AskService, tempfile::TempDir, Arc<tokio::sync::Notify>) {
        let temp = tempfile::tempdir().unwrap();
        let program = temp.path().join("codex");
        std::fs::write(&program, "#!/usr/bin/python3\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let native = NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap();
        let availability = AskAvailability::capture(
            CliCompletionProvider::Codex,
            OsStr::new("codex"),
            vec![("PATH".into(), temp.path().as_os_str().to_owned())],
            temp.path(),
            native,
        )
        .unwrap();
        let AskAvailability::Available(service) = availability else {
            panic!("controlled Ask fixture program must resolve");
        };
        let service = *service;
        let host = service.host.lock().unwrap().take().unwrap();
        assert!(matches!(
            host.ready(Instant::now() + Duration::from_secs(5)).await,
            HostReadiness::Ready
        ));
        let host = Arc::try_unwrap(host).unwrap();
        let release = Arc::new(tokio::sync::Notify::new());
        let released = Arc::clone(&release);
        *service.shutdown.lock().await = Some(HostOwner::Joining(tokio::spawn(async move {
            released.notified().await;
            let result = host.shutdown(Instant::now() + Duration::from_secs(5)).await;
            assert!(
                matches!(&result, HostShutdown::Joined(report) if report.failure().is_none()),
                "controlled fixture requires a physically drained Host"
            );
            assert!(!fail, "controlled join failure after actual Host shutdown");
            result
        })));
        (service, temp, release)
    }

    #[cfg(unix)]
    pub(crate) async fn with_terminal_host_join_failure() -> (AskService, tempfile::TempDir) {
        let (service, temp, release) = paused_host_join(true).await;
        release.notify_one();
        (service, temp)
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn cancelled_join_waiter_keeps_actual_host_owner_and_reports_terminal_failure_once() {
        for fail in [false, true] {
            let (service, _temp, release) = paused_host_join(fail).await;
            assert!(
                tokio::time::timeout(Duration::from_millis(20), service.shutdown())
                    .await
                    .is_err()
            );
            assert!(matches!(
                service.shutdown.lock().await.as_ref(),
                Some(HostOwner::Joining(_))
            ));
            release.notify_one();
            let result = service.shutdown().await;
            assert_eq!(result.is_err(), fail);
            if fail {
                let error = result.unwrap_err();
                let join = error
                    .get_ref()
                    .unwrap()
                    .downcast_ref::<tokio::task::JoinError>()
                    .unwrap();
                assert!(join.is_panic());
                assert!(
                    error
                        .to_string()
                        .contains("controlled join failure after actual Host shutdown")
                );
                assert!(matches!(
                    service.shutdown.lock().await.as_ref(),
                    Some(HostOwner::JoinFailed(None))
                ));
            } else {
                assert!(matches!(
                    service.shutdown.lock().await.as_ref(),
                    Some(HostOwner::Joined(None))
                ));
            }
            service.shutdown().await.unwrap();
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn normal_reap_removes_only_observed_records_from_real_process_receipts() {
        struct Ignore;
        impl graphit_process::IncrementalSink for Ignore {
            async fn push<'a>(
                &'a mut self,
                _: graphit_process::Chunk<'a>,
                original: &'a SnapshotContext,
            ) -> Result<(), graphit_process::SinkFailure> {
                original.cancellation().cancel();
                Ok(())
            }
        }
        let temp = tempfile::tempdir().unwrap();
        let native = NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap();
        let stop = Cancellation::default();
        let host = ProcessHost::install(&stop, HostLimits::new(2, 2).unwrap()).unwrap();
        assert!(matches!(
            host.ready(Instant::now() + Duration::from_secs(5)).await,
            HostReadiness::Ready
        ));
        let service = AskService {
            program: "/usr/bin/python3".into(),
            provider: CliCompletionProvider::Codex,
            kiro_slots: Arc::new(tokio::sync::Semaphore::new(2)),
            kiro_processes: Mutex::new(Vec::new()),
            opencode: None,
            environment: Vec::new(),
            policy: AskLaunchPolicy::capture(native),
            stop,
            host: Mutex::new(Some(Arc::new(host))),
            shutdown: tokio::sync::Mutex::new(None),
            remaining: Mutex::new(Vec::new()),
            kimi: Mutex::new(Vec::new()),
            conversations: Mutex::new(Vec::new()),
            cleanup_failure: Mutex::new(None),
        };
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(10),
            Cancellation::default(),
        ));
        let mut request = service.request(temp.path(), "unused-model");
        request.args = vec![
            "-c".into(),
            "import time; print('started', flush=True); time.sleep(2)".into(),
        ];
        request.limits.cleanup = Duration::from_nanos(2);
        request.limits.termination_grace = Duration::from_nanos(1);
        let host = service.host().unwrap();
        let (_stop, receiver) = graphit_process::operation(&original, &host).unwrap();
        let mut result = graphit_process::run(
            request,
            original.as_ref(),
            graphit_process::Retention::None,
            &mut Ignore,
            receiver,
        )
        .await;
        let remaining = match &mut result {
            Ok(process) => process.remaining.take(),
            Err(process) => process.remaining.take(),
        };
        assert!(
            remaining.is_some(),
            "fixture must observe a remaining native owner; observed: {result:?}"
        );
        if let Some(remaining) = remaining {
            service.remaining.lock().unwrap().push(RetainedProcess {
                process: Arc::new(tokio::sync::Mutex::new(remaining)),
                _prompt_file: None,
            });
        }
        drop(host);
        let joined = service.shutdown().await;
        service.reap().await;
        assert!(service.remaining.lock().unwrap().is_empty());
        // Report a real cleanup failure once, after its record has been reaped.
        if let Err(error) = joined {
            assert!(error.to_string().contains("Knowledge Ask Process cleanup:"));
        }
        assert!(service.cleanup_failure.lock().unwrap().is_none());
        service.shutdown().await.unwrap();
    }
    #[tokio::test]
    #[cfg(unix)]
    async fn kiro_internal_future_drop_keeps_user_writer_until_owned_child_join() {
        kiro_drop_stage(false).await;
    }
    #[tokio::test]
    #[cfg(unix)]
    async fn kiro_app_drop_after_receipt_keeps_user_writer_until_owned_child_join() {
        kiro_drop_stage(true).await;
    }
    #[cfg(unix)]
    async fn kiro_drop_stage(after_receipt: bool) {
        use agent_client_protocol::schema::v1::SessionNotification;
        use graphit_ai::cli_kiro::{KiroDeliveryPhase, KiroSink, run_kiro};
        use std::os::unix::fs::PermissionsExt;
        struct Sink(Arc<tokio::sync::Notify>);
        impl KiroSink for Sink {
            async fn push<'a>(
                &'a mut self,
                _: KiroDeliveryPhase,
                _: &'a SessionNotification,
                original: &'a SnapshotContext,
            ) -> io::Result<()> {
                original.check()?;
                self.0.notify_one();
                Ok(())
            }
        }
        let executable = std::env::current_exe().unwrap();
        let binary = executable
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("kiro-knowledge-fixture");
        assert!(
            binary.is_file(),
            "build the pinned SDK fixture target before this custody test"
        );
        let temp = tempfile::tempdir().unwrap();
        let program = temp.path().join("kiro-cli");
        let escaped = binary.to_str().unwrap().replace('\'', "'\\''");
        std::fs::write(
            &program,
            format!("#!/bin/sh\ntrap '' TERM\nexec '{escaped}' \"$@\"\n"),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let native = NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap();
        let availability = AskAvailability::capture(
            CliCompletionProvider::KiroV3,
            OsStr::new("kiro-cli"),
            vec![
                ("PATH".into(), temp.path().as_os_str().to_owned()),
                ("KIRO_CAPTURED".into(), "literal-captured".into()),
                ("KIRO_FIXTURE_MODEL".into(), "fixture-model".into()),
                (
                    "KIRO_FIXTURE_MODE".into(),
                    if after_receipt {
                        "delay_exit"
                    } else {
                        "cancel"
                    }
                    .into(),
                ),
            ],
            temp.path(),
            native,
        )
        .unwrap();
        let service = availability.service().unwrap();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        let (_user, scope, turn) = conversation_fixture_engine(
            &original,
            service.program(),
            graphit_ai::knowledge_conversation::ConversationEngine::KiroV3,
        );
        let custody = service.hold_conversation_turn(turn);
        let mut decision =
            crate::knowledge_conversation::ConversationDecisionGuard::new(Arc::clone(&custody));
        let (transport, owner) = service
            .kiro_process(temp.path(), &original, Some(Arc::clone(&custody)), &|| true)
            .await
            .unwrap();
        decision.submitted();
        let notified = Arc::new(tokio::sync::Notify::new());
        let mut sink = Sink(Arc::clone(&notified));
        let mut future=Box::pin(run_kiro(&original,transport,service.kiro_engine().unwrap(),custody.configure_kiro(service.program(),temp.path(),&original).unwrap(),service.kiro_prompt(serde_json::json!({"question":"bounded","pages":[{"slug":"Paris","body":"physical fixture"}]}).to_string(),"fixture-model"),service.kiro_limits(),&mut sink));
        if after_receipt {
            let receipt = future.as_mut().await.unwrap();
            assert!(receipt.response().is_some());
            assert!(
                !owner.public_observation().await["joined"]
                    .as_bool()
                    .unwrap()
            );
        } else {
            tokio::select! {
                ()=notified.notified()=>{},
                result=&mut future=>panic!("native fixture must stream before pending prompt: {result:?}"),
            }
        }
        assert!(owner.spawned());
        assert_writer_held(&scope);
        drop(future);
        drop(decision);
        // The native process ignores TERM: losing SDK future ownership must not
        // be mistaken for its actual kernel wait or release the USER writer.
        assert_writer_held(&scope);
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert_writer_held(&scope);
        assert!(service.join_kiro(&owner).await.unwrap());
        assert!(custody.released());
        assert!(
            !graphit_core::file_lock::FileLock::is_locked(
                scope.runtime_root().join("artifact-store.lock")
            )
            .unwrap()
        );
        assert!(
            owner.public_observation().await["joined"]
                .as_bool()
                .unwrap()
        );
        service.shutdown().await.unwrap();
    }
    #[tokio::test]
    #[cfg(unix)]
    async fn kiro_authority_changed_while_two_real_children_hold_slots_prevents_third_spawn() {
        use crate::knowledge_kiro_process::KiroProcessOwner;
        use agent_client_protocol::schema::v1::SessionNotification;
        use graphit_ai::cli_kiro::{KiroDeliveryPhase, KiroSink, run_kiro};
        use std::sync::atomic::{AtomicBool, Ordering};
        use tokio::sync::mpsc;
        struct Sink(Option<(mpsc::Sender<Arc<KiroProcessOwner>>, Arc<KiroProcessOwner>)>);
        impl KiroSink for Sink {
            async fn push<'a>(
                &'a mut self,
                _: KiroDeliveryPhase,
                _: &'a SessionNotification,
                original: &'a SnapshotContext,
            ) -> io::Result<()> {
                original.check()?;
                if let Some((ready, owner)) = self.0.take() {
                    ready
                        .send(owner)
                        .await
                        .map_err(|_| io::ErrorKind::BrokenPipe)?;
                }
                Ok(())
            }
        }
        let binary = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("kiro-knowledge-fixture");
        assert!(binary.is_file());
        let temp = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(binary, temp.path().join("kiro-cli")).unwrap();
        let availability = Arc::new(
            AskAvailability::capture(
                CliCompletionProvider::KiroV3,
                OsStr::new("kiro-cli"),
                vec![
                    ("PATH".into(), temp.path().as_os_str().to_owned()),
                    ("KIRO_CAPTURED".into(), "literal-captured".into()),
                    ("KIRO_FIXTURE_MODEL".into(), "fixture-model".into()),
                    ("KIRO_FIXTURE_MODE".into(), "cancel".into()),
                    (
                        "KIRO_FIXTURE_JOURNAL".into(),
                        temp.path().join("journal").into_os_string(),
                    ),
                ],
                temp.path(),
                NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap(),
            )
            .unwrap(),
        );
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        let (ready, mut receive) = mpsc::channel(2);
        let mut tasks = Vec::new();
        for _ in 0..2 {
            let availability = Arc::clone(&availability);
            let original = Arc::clone(&original);
            let cwd = temp.path().to_owned();
            let ready = ready.clone();
            tasks.push(tokio::spawn(async move {
                let service = availability.service().unwrap();
                let (transport,owner) = service.kiro_process(&cwd,&original,None,&||true).await.unwrap();
                let mut sink = Sink(Some((ready, Arc::clone(&owner))));
                let _result = run_kiro(&original,transport,service.kiro_engine().unwrap(),service.kiro_new_session(&cwd,"fixture-model"),service.kiro_prompt(serde_json::json!({"question":"bounded","pages":[{"slug":"Paris","body":"physical"}]}).to_string(),"fixture-model"),service.kiro_limits(),&mut sink).await;
                assert!(service.join_kiro(&owner).await.unwrap());
            }));
        }
        drop(ready);
        let owners = tokio::time::timeout_at(original.required_deadline().unwrap().into(), async {
            [receive.recv().await.unwrap(), receive.recv().await.unwrap()]
        })
        .await
        .expect("both native children must stream before the admission check");
        assert!(owners.iter().all(|o| o.spawned()));
        let service = availability.service().unwrap();
        let (_user, scope, turn) = conversation_fixture_engine(
            &original,
            service.program(),
            graphit_ai::knowledge_conversation::ConversationEngine::KiroV3,
        );
        let custody = service.hold_conversation_turn(turn);
        let decision =
            crate::knowledge_conversation::ConversationDecisionGuard::new(Arc::clone(&custody));
        let check = || true;
        let mut waiting = Box::pin(service.kiro_process(
            temp.path(),
            &original,
            Some(Arc::clone(&custody)),
            &check,
        ));
        assert!(futures_util::poll!(waiting.as_mut()).is_pending());
        assert_writer_held(&scope);
        drop(waiting);
        drop(decision);
        assert!(custody.released());
        assert!(
            !graphit_core::file_lock::FileLock::is_locked(
                scope.runtime_root().join("artifact-store.lock")
            )
            .unwrap()
        );
        assert_eq!(service.kiro_processes.lock().unwrap().len(), 2);
        let allowed = AtomicBool::new(true);
        let check = || allowed.load(Ordering::SeqCst);
        let mut third = Box::pin(service.kiro_process(temp.path(), &original, None, &check));
        assert!(futures_util::poll!(third.as_mut()).is_pending());
        allowed.store(false, Ordering::SeqCst);
        for owner in &owners {
            owner.stop();
        }
        let result = third.await;
        assert_eq!(
            result
                .err()
                .expect("authority must be rechecked after queue wait")
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(temp.path().join("journal"))
                .unwrap()
                .lines()
                .filter(|e| *e == "engine_v3")
                .count(),
            2
        );
        service.shutdown().await.unwrap();
    }
}

#[cfg(test)]
#[path = "knowledge_ask_service/grok_tests.rs"]
#[cfg(unix)]
mod grok_tests;
