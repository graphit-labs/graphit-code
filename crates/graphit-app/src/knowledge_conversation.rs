//! Configured Incoming MCP access and service-owned native conversation turns.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]
use graphit_ai::{
    cli_codex::CodexReceipt,
    knowledge_conversation::{
        ConversationError, ConversationLimits, ConversationTurn, PublicConversation,
    },
};
use graphit_auth::broker::{BrokerVerifier, project_access::BrokerProjectAccess};
use graphit_core::file_io::SnapshotContext;
use graphit_process::{Cleanup, RemainingProcess, Request};
use std::sync::{Arc, Mutex};

/// Startup configuration selects existing clients; it does not authenticate a request.
pub struct ConversationAccess {
    pub providers: Vec<(String, Arc<BrokerVerifier>)>,
    pub backend: Arc<BrokerProjectAccess>,
    pub limits: ConversationLimits,
}
struct Custody {
    opencode: Option<Arc<crate::acp_process::AcpProcessOwner>>,
    kimi: Option<Arc<graphit_ai::cli_kimi::KimiServer>>,
    prompt_file: Option<tempfile::TempPath>,
    turn: Option<ConversationTurn>,
    remaining: Option<Arc<tokio::sync::Mutex<RemainingProcess>>>,
    terminal: bool,
    persistence_decided: bool,
    failure: Option<ConversationError>,
}
/// Service custody precedes polling the transport; no Host Arc is retained here.
pub(crate) struct ConversationCustodyEntry(Mutex<Custody>);
impl ConversationCustodyEntry {
    pub(crate) fn new(turn: ConversationTurn) -> Self {
        Self(Mutex::new(Custody {
            opencode: None,
            kimi: None,
            prompt_file: None,
            turn: Some(turn),
            remaining: None,
            terminal: false,
            persistence_decided: false,
            failure: None,
        }))
    }
    pub(crate) fn configure_opencode(
        &self,
        program: &std::path::Path,
        model: &str,
        cwd: &std::path::Path,
        original: &Arc<SnapshotContext>,
    ) -> Result<graphit_ai::cli_opencode::OpencodeSessionStart, ConversationError> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .turn
            .as_ref()
            .expect("held turn")
            .configure_opencode_session(program, model, cwd, original)
    }
    pub(crate) fn observe_opencode_session(
        &self,
        id: &str,
        loaded: bool,
        original: &Arc<SnapshotContext>,
    ) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.failure.is_some() {
            return false;
        }
        if let Err(error) = state
            .turn
            .as_mut()
            .expect("held turn")
            .observe_opencode_session(id, loaded, original)
        {
            state.failure = Some(error);
            return false;
        }
        true
    }
    pub(crate) fn retain_opencode(&self, owner: Arc<crate::acp_process::AcpProcessOwner>) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        assert!(state.opencode.is_none());
        if state.persistence_decided {
            owner.stop();
        }
        state.opencode = Some(owner);
    }
    pub(crate) fn opencode_owner(&self) -> Option<Arc<crate::acp_process::AcpProcessOwner>> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .opencode
            .clone()
    }
    /// The existing ACP owner joins only after its SDK input returned, native pipes
    /// closed and the retained native process tree actually retired.
    pub(crate) async fn observe_opencode(&self, deadline: std::time::Instant) -> bool {
        let owner = {
            let state = self.0.lock().unwrap_or_else(|e| e.into_inner());
            match &state.opencode {
                Some(owner) => Arc::clone(owner),
                None => return state.terminal,
            }
        };
        if !owner.observe(deadline).await {
            return false;
        }
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(current) = &state.opencode {
            assert!(Arc::ptr_eq(current, &owner));
        }
        state.terminal = true;
        state.opencode = None;
        if state.persistence_decided {
            state.turn = None;
        }
        true
    }
    pub(crate) fn configure_kimi(
        &self,
        program: &std::path::Path,
        model: &str,
        cwd: &std::path::Path,
        original: &Arc<SnapshotContext>,
    ) -> Result<graphit_ai::cli_kimi::KimiSessionStart, ConversationError> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .turn
            .as_ref()
            .expect("held turn")
            .configure_kimi_session(program, model, cwd, original)
    }
    pub(crate) fn observe_kimi(&self, id: &str, original: &Arc<SnapshotContext>) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.failure.is_some() {
            return false;
        }
        if let Err(error) = state
            .turn
            .as_mut()
            .expect("held turn")
            .observe_kimi_session(id, original)
        {
            state.failure = Some(error);
            return false;
        }
        true
    }
    /// Refer to the registered native owner before the next await, including
    /// startup failures. This does not create a second Process owner.
    pub(crate) fn retain_kimi(&self, owner: Arc<graphit_ai::cli_kimi::KimiServer>) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        assert!(state.kimi.is_none());
        if state.persistence_decided {
            owner.stop();
        }
        state.kimi = Some(owner);
    }
    pub(crate) fn kimi_server(&self) -> Option<Arc<graphit_ai::cli_kimi::KimiServer>> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .kimi
            .clone()
    }
    /// Only the actual native observation can release this owner reference.
    pub(crate) fn observed_kimi(&self, observation: &graphit_ai::cli_kimi::KimiServerObservation) {
        if !observation.joined {
            return;
        }
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.terminal = true;
        state.kimi = None;
        if state.persistence_decided {
            state.turn = None;
        }
    }
    pub(crate) fn configure_copilot(
        &self,
        program: &std::path::Path,
        model: &str,
        cwd: &std::path::Path,
        original: &Arc<SnapshotContext>,
    ) -> Result<graphit_ai::cli_copilot::CopilotSessionStart, ConversationError> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .turn
            .as_ref()
            .expect("held turn")
            .configure_copilot_session(program, model, cwd, original)
    }
    pub(crate) fn observe_copilot(
        &self,
        id: &str,
        resumed: bool,
        original: &Arc<SnapshotContext>,
    ) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.failure.is_some() {
            return false;
        }
        if let Err(error) = state
            .turn
            .as_mut()
            .expect("held turn")
            .observe_copilot_session(id, resumed, original)
        {
            state.failure = Some(error);
            return false;
        }
        true
    }
    /// Transfer the actual CLI Process receipt before projection or persistence.
    pub(crate) fn retain_copilot(&self, receipt: &mut graphit_ai::cli_copilot::CopilotReceipt) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(process) = receipt.process_mut() {
            let (cleanup, remaining) = match process {
                Ok(p) => (p.cleanup, p.remaining.take()),
                Err(p) => (p.cleanup, p.remaining.take()),
            };
            state.terminal =
                remaining.is_none() && matches!(cleanup, Cleanup::Complete | Cleanup::NotSpawned);
            state.remaining = remaining.map(|p| Arc::new(tokio::sync::Mutex::new(p)));
        } else {
            state.terminal = true;
        }
    }
    pub(crate) fn configure(
        &self,
        request: &mut Request,
        original: &Arc<SnapshotContext>,
    ) -> Result<(), ConversationError> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .turn
            .as_ref()
            .expect("held turn")
            .configure_codex_request(request, original)
    }
    pub(crate) fn configure_claude(
        &self,
        request: &mut Request,
        original: &Arc<SnapshotContext>,
    ) -> Result<(), ConversationError> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .turn
            .as_ref()
            .expect("held turn")
            .configure_claude_request(request, original)
    }
    pub(crate) fn observe_claude(&self, id: &str, original: &Arc<SnapshotContext>) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.failure.is_some() {
            return false;
        }
        if let Err(error) = state
            .turn
            .as_mut()
            .expect("held turn")
            .observe_claude_session(id, original)
        {
            state.failure = Some(error);
            return false;
        }
        true
    }
    /// Take the actual Claude Process receipt before projection, persistence or another await.
    pub(crate) fn retain_claude(&self, receipt: &mut graphit_ai::cli_claude::ClaudeReceipt) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(process) = receipt.process_mut() {
            let (cleanup, remaining) = match process {
                Ok(p) => (p.cleanup, p.remaining.take()),
                Err(p) => (p.cleanup, p.remaining.take()),
            };
            state.terminal =
                remaining.is_none() && matches!(cleanup, Cleanup::Complete | Cleanup::NotSpawned);
            state.remaining = remaining.map(|p| Arc::new(tokio::sync::Mutex::new(p)));
        }
    }
    pub(crate) fn release_pre_submit_claude_failure(
        &self,
        failure: &graphit_ai::cli_claude::ClaudeFailure,
    ) {
        use graphit_ai::cli_claude::ClaudeReason;
        if failure.receipt().process().is_none()
            && matches!(
                failure.reason(),
                ClaudeReason::Context(_) | ClaudeReason::Policy | ClaudeReason::Admission(_)
            )
        {
            self.release_unsubmitted();
        }
    }
    pub(crate) fn configure_antigravity(
        &self,
        request: &mut Request,
        original: &Arc<SnapshotContext>,
    ) -> Result<(), ConversationError> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .turn
            .as_ref()
            .expect("held turn")
            .configure_antigravity_request(request, original)
    }
    pub(crate) fn observe_antigravity(&self, id: &str, original: &Arc<SnapshotContext>) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.failure.is_some() {
            return false;
        }
        if let Err(error) = state
            .turn
            .as_mut()
            .expect("held turn")
            .observe_antigravity_session(id, original)
        {
            state.failure = Some(error);
            return false;
        }
        true
    }
    /// Take the actual Antigravity Process receipt before projection, persistence or another await.
    pub(crate) fn retain_antigravity(
        &self,
        receipt: &mut graphit_ai::cli_antigravity::AntigravityReceipt,
    ) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(process) = receipt.process_mut() {
            let (cleanup, remaining) = match process {
                Ok(p) => (p.cleanup, p.remaining.take()),
                Err(p) => (p.cleanup, p.remaining.take()),
            };
            state.terminal =
                remaining.is_none() && matches!(cleanup, Cleanup::Complete | Cleanup::NotSpawned);
            state.remaining = remaining.map(|p| Arc::new(tokio::sync::Mutex::new(p)));
        }
    }
    pub(crate) fn release_pre_submit_antigravity_failure(
        &self,
        failure: &graphit_ai::cli_antigravity::AntigravityFailure,
    ) {
        use graphit_ai::cli_antigravity::AntigravityReason;
        if failure.receipt().process().is_none()
            && matches!(
                failure.reason(),
                AntigravityReason::Context(_)
                    | AntigravityReason::Policy
                    | AntigravityReason::Admission(_)
            )
        {
            self.release_unsubmitted();
        }
    }
    pub(crate) fn configure_qwen(
        &self,
        request: &mut Request,
        original: &Arc<SnapshotContext>,
    ) -> Result<(), ConversationError> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .turn
            .as_ref()
            .expect("held turn")
            .configure_qwen_request(request, original)
    }
    pub(crate) fn observe_qwen(&self, id: &str, original: &Arc<SnapshotContext>) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.failure.is_some() {
            return false;
        }
        if let Err(error) = state
            .turn
            .as_mut()
            .expect("held turn")
            .observe_qwen_session(id, original)
        {
            state.failure = Some(error);
            return false;
        }
        true
    }
    /// Take the actual Qwen Process receipt before projection, persistence or another await.
    pub(crate) fn retain_qwen(&self, receipt: &mut graphit_ai::cli_qwen::QwenReceipt) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(process) = receipt.process_mut() {
            let (cleanup, remaining) = match process {
                Ok(p) => (p.cleanup, p.remaining.take()),
                Err(p) => (p.cleanup, p.remaining.take()),
            };
            state.terminal =
                remaining.is_none() && matches!(cleanup, Cleanup::Complete | Cleanup::NotSpawned);
            state.remaining = remaining.map(|p| Arc::new(tokio::sync::Mutex::new(p)));
        }
    }
    pub(crate) fn release_pre_submit_qwen_failure(
        &self,
        failure: &graphit_ai::cli_qwen::QwenFailure,
    ) {
        use graphit_ai::cli_qwen::QwenReason;
        if failure.receipt().process().is_none()
            && matches!(
                failure.reason(),
                QwenReason::Context(_) | QwenReason::Policy | QwenReason::Admission(_)
            )
        {
            self.release_unsubmitted();
        }
    }
    pub(crate) fn configure_cursor(
        &self,
        request: &mut Request,
        original: &Arc<SnapshotContext>,
    ) -> Result<(), ConversationError> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .turn
            .as_ref()
            .expect("held turn")
            .configure_cursor_request(request, original)
    }
    pub(crate) fn observe_cursor(&self, id: &str, original: &Arc<SnapshotContext>) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.failure.is_some() {
            return false;
        }
        if let Err(error) = state
            .turn
            .as_mut()
            .expect("held turn")
            .observe_cursor_session(id, original)
        {
            state.failure = Some(error);
            return false;
        }
        true
    }
    /// Take the actual Cursor Process receipt before projection, persistence or another await.
    pub(crate) fn retain_cursor(&self, receipt: &mut graphit_ai::cli_cursor::CursorReceipt) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(process) = receipt.process_mut() {
            let (cleanup, remaining) = match process {
                Ok(p) => (p.cleanup, p.remaining.take()),
                Err(p) => (p.cleanup, p.remaining.take()),
            };
            state.terminal =
                remaining.is_none() && matches!(cleanup, Cleanup::Complete | Cleanup::NotSpawned);
            state.remaining = remaining.map(|p| Arc::new(tokio::sync::Mutex::new(p)));
        }
    }
    pub(crate) fn release_pre_submit_cursor_failure(
        &self,
        failure: &graphit_ai::cli_cursor::CursorFailure,
    ) {
        use graphit_ai::cli_cursor::CursorReason;
        if failure.receipt().process().is_none()
            && matches!(
                failure.reason(),
                CursorReason::Context(_) | CursorReason::Policy | CursorReason::Admission(_)
            )
        {
            self.release_unsubmitted();
        }
    }
    pub(crate) fn configure_grok(
        &self,
        request: &mut Request,
        prompt_file: tempfile::TempPath,
        original: &Arc<SnapshotContext>,
    ) -> Result<(), ConversationError> {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state
            .turn
            .as_ref()
            .expect("held turn")
            .configure_grok_request(request, prompt_file.as_ref(), original)?;
        assert!(state.prompt_file.is_none());
        state.prompt_file = Some(prompt_file);
        Ok(())
    }
    pub(crate) fn observe_grok(&self, id: &str, original: &Arc<SnapshotContext>) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.failure.is_some() {
            return false;
        }
        if let Err(error) = state
            .turn
            .as_mut()
            .expect("held turn")
            .observe_grok_session(id, original)
        {
            state.failure = Some(error);
            return false;
        }
        true
    }
    /// Take the actual Grok Process receipt before projection, persistence or another await.
    pub(crate) fn retain_grok(&self, receipt: &mut graphit_ai::cli_grok::GrokReceipt) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(process) = receipt.process_mut() {
            let (cleanup, remaining) = match process {
                Ok(p) => (p.cleanup, p.remaining.take()),
                Err(p) => (p.cleanup, p.remaining.take()),
            };
            state.terminal =
                remaining.is_none() && matches!(cleanup, Cleanup::Complete | Cleanup::NotSpawned);
            state.remaining = remaining.map(|p| Arc::new(tokio::sync::Mutex::new(p)));
            if state.terminal {
                state.prompt_file = None;
            }
        }
    }
    pub(crate) fn release_pre_submit_grok_failure(
        &self,
        failure: &graphit_ai::cli_grok::GrokFailure,
    ) {
        use graphit_ai::cli_grok::GrokReason;
        if failure.receipt().process().is_none()
            && matches!(
                failure.reason(),
                GrokReason::Context(_) | GrokReason::Policy | GrokReason::Admission(_)
            )
        {
            self.release_unsubmitted();
        }
    }
    pub(crate) fn observe(&self, id: &str, original: &Arc<SnapshotContext>) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.failure.is_some() {
            return false;
        }
        if let Err(error) = state
            .turn
            .as_mut()
            .expect("held turn")
            .observe_thread_started(id, original)
        {
            state.failure = Some(error);
            return false;
        }
        true
    }
    pub(crate) fn configure_kiro(
        &self,
        program: &std::path::Path,
        cwd: &std::path::Path,
        original: &Arc<SnapshotContext>,
    ) -> Result<graphit_ai::cli_kiro::KiroSessionStart, ConversationError> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .turn
            .as_ref()
            .expect("held turn")
            .configure_kiro_session(program, cwd, original)
    }
    pub(crate) fn configure_kiro_delete(
        &self,
        program: &std::path::Path,
        original: &Arc<SnapshotContext>,
    ) -> Result<agent_client_protocol::schema::v1::DeleteSessionRequest, ConversationError> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .turn
            .as_ref()
            .expect("held turn")
            .configure_kiro_delete(program, original)
    }
    pub(crate) fn observe_kiro(
        &self,
        id: &agent_client_protocol::schema::v1::SessionId,
        original: &Arc<SnapshotContext>,
    ) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.failure.is_some() {
            return false;
        }
        if let Err(error) = state
            .turn
            .as_mut()
            .expect("held turn")
            .observe_kiro_session(id, original)
        {
            state.failure = Some(error);
            return false;
        }
        true
    }
    /// Called synchronously on the actual receipt, before persistence or any await.
    pub(crate) fn retain(&self, receipt: &mut CodexReceipt<'_>) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(process) = receipt.process_mut() {
            let (cleanup, remaining) = match process {
                Ok(p) => (p.cleanup, p.remaining.take()),
                Err(p) => (p.cleanup, p.remaining.take()),
            };
            state.terminal =
                remaining.is_none() && matches!(cleanup, Cleanup::Complete | Cleanup::NotSpawned);
            state.remaining = remaining.map(|p| Arc::new(tokio::sync::Mutex::new(p)));
        }
    }
    /// In the current Codex producer these reasons with no Process return before run.
    pub(crate) fn release_pre_submit_failure(
        &self,
        failure: &graphit_ai::cli_codex::CodexFailure<'_>,
    ) {
        use graphit_ai::cli_codex::CodexReason;
        if failure.receipt().process().is_none()
            && matches!(
                failure.reason(),
                CodexReason::Context(_)
                    | CodexReason::Policy
                    | CodexReason::Admission(_)
                    | CodexReason::State
            )
        {
            self.release_unsubmitted();
        }
    }
    pub(crate) fn remaining(&self) -> Option<Arc<tokio::sync::Mutex<RemainingProcess>>> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remaining
            .clone()
    }
    /// The caller proves the transport was never polled/submitted in this branch.
    pub(crate) fn release_unsubmitted(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        assert!(state.remaining.is_none() && state.opencode.is_none() && state.kimi.is_none());
        state.prompt_file = None;
        state.persistence_decided = true;
        state.turn = None;
    }
    pub(crate) fn observed_terminal(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.terminal = true;
        state.prompt_file = None;
        state.remaining = None;
        if state.persistence_decided {
            state.turn = None;
        }
    }
    /// An internally dropped native future cannot decide to publish later. Keep
    /// its writer until the owned native child actually joins.
    pub(crate) fn abandon_native(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(owner) = &state.opencode {
            owner.stop();
        }
        if let Some(owner) = &state.kimi {
            owner.stop();
        }
        state.persistence_decided = true;
        if state.terminal {
            state.turn = None;
        }
    }
    /// An actual Host join also covers an internally dropped future without a receipt.
    pub(crate) fn host_joined(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        // A Host join does not prove that installed SDK transport halves returned.
        if state.opencode.is_some() || state.kimi.is_some() {
            return;
        }
        state.terminal = true;
        state.prompt_file = None;
        state.persistence_decided = true;
        state.turn = None;
    }
    pub(crate) fn finish(
        &self,
        original: &Arc<SnapshotContext>,
        allowed: bool,
    ) -> ConversationPersistence {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.persistence_decided = true;
        let result = if let Some(error) = state.failure.take() {
            ConversationPersistence::Failed(error)
        } else if state.turn.is_none() && !state.terminal && state.remaining.is_none() {
            ConversationPersistence::NotStarted
        } else if !allowed || state.turn.is_none() {
            ConversationPersistence::Denied
        } else if !state.terminal {
            ConversationPersistence::Pending
        } else {
            match state.turn.as_mut().expect("held turn").commit(original) {
                Ok(record) => ConversationPersistence::Persisted(record),
                Err(error) => ConversationPersistence::Failed(error),
            }
        };
        if state.terminal {
            state.turn = None;
        }
        result
    }
    pub(crate) fn finish_delete(
        &self,
        original: &Arc<SnapshotContext>,
        allowed: bool,
    ) -> ConversationDeletion {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.persistence_decided = true;
        let result = if let Some(error) = state.failure.take() {
            ConversationDeletion::Failed(error)
        } else if state.turn.is_none() && !state.terminal && state.remaining.is_none() {
            ConversationDeletion::NotStarted
        } else if !allowed || state.turn.is_none() {
            ConversationDeletion::Denied
        } else if !state.terminal {
            ConversationDeletion::Pending
        } else {
            match state
                .turn
                .as_mut()
                .expect("held turn")
                .delete_kiro_record(original)
            {
                Ok(()) => ConversationDeletion::Deleted,
                Err(error) => ConversationDeletion::Failed(error),
            }
        };
        if state.terminal {
            state.turn = None;
        }
        result
    }
    pub(crate) fn released(&self) -> bool {
        let state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.turn.is_none()
            && state.remaining.is_none()
            && state.kimi.is_none()
            && state.prompt_file.is_none()
            && state.opencode.is_none()
    }
}
pub(crate) enum ConversationPersistence {
    Persisted(PublicConversation),
    Failed(ConversationError),
    Pending,
    NotStarted,
    Denied,
}
pub(crate) enum ConversationDeletion {
    Deleted,
    Failed(ConversationError),
    Pending,
    NotStarted,
    Denied,
}

/// Retains the publication decision across every await in a native conversation caller.
pub(crate) struct ConversationDecisionGuard {
    custody: Option<Arc<ConversationCustodyEntry>>,
    submitted: bool,
}
impl ConversationDecisionGuard {
    pub(crate) fn new(custody: Arc<ConversationCustodyEntry>) -> Self {
        Self {
            custody: Some(custody),
            submitted: false,
        }
    }
    /// Called synchronously after the owned transport is returned, before its first poll.
    pub(crate) fn submitted(&mut self) {
        self.submitted = true;
    }
    pub(crate) fn decided(&mut self) {
        self.custody = None;
    }
}
impl Drop for ConversationDecisionGuard {
    fn drop(&mut self) {
        if let Some(custody) = &self.custody {
            if self.submitted {
                custody.abandon_native();
            } else {
                custody.release_unsubmitted();
            }
        }
    }
}
