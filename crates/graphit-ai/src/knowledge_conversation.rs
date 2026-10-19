//! Private USER binding storage; App authorization remains a separate gate.
use graphit_core::{
    artifact_store::{
        ArtifactEntry, ArtifactKey, ArtifactLimits, ArtifactMode, ArtifactSnapshot, ArtifactStore,
        ArtifactWriter, CapturedPublicationScope, CopyPolicy, GeneratedNode, PayloadMode,
        PayloadPolicy, ScopeContext, StoreOwner, ValidatedPayloadPlan,
    },
    file_io::SnapshotContext,
    native_document_file::{CapturedUserDocumentDirectory, NativeDocumentFileLimits},
};
use graphit_fs::NodeKind;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    ffi::OsString,
    fmt, io,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
};

const ADAPTER: &str = "graphit-ai";
const TYPE: &str = "knowledge-conversation";

#[derive(Clone, Copy)]
pub struct ConversationLimits {
    pub artifacts: ArtifactLimits,
    pub native: NativeDocumentFileLimits,
    pub record_bytes: NonZeroUsize,
    pub field_bytes: NonZeroUsize,
    pub sources: NonZeroUsize,
    pub list_records: NonZeroUsize,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationSource {
    pub slug: String,
    pub source_path: String,
    pub physical_digest: Option<[u8; 32]>,
    pub compilation_digest: [u8; 32],
    pub recipe: String,
    pub fallback_title: String,
    pub kind_on_absent: String,
    pub max_summary_bytes: NonZeroUsize,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationEngine {
    Antigravity,
    Kimi,
    #[serde(rename = "opencode")]
    OpenCode,
    Copilot,
    Codex,
    Claude,
    Qwen,
    Cursor,
    Grok,
    KiroV3,
}
/// Observed selection data. No corpus-wide generation or authority is encoded.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationBinding {
    pub provider: String,
    pub engine: ConversationEngine,
    pub model: String,
    pub program: PathBuf,
    pub project_id: String,
    pub project_version: String,
    pub sources: Vec<ConversationSource>,
}
/// Actual selected native session target; source revisions are irrelevant to deletion.
/// These fields are selection data, never USER or PROJECT authority.
pub struct KiroConversationTarget<'a> {
    pub engine: ConversationEngine,
    pub model: &'a str,
    pub program: &'a Path,
    pub project_id: &'a str,
    pub project_version: &'a str,
}
#[derive(Serialize, PartialEq, Eq)]
pub struct PublicConversation {
    pub conversation_id: String,
    pub provider: String,
    pub engine: ConversationEngine,
    pub model: String,
    pub project_id: String,
    pub project_version: String,
    pub selected_slugs: Vec<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    conversation_id: String,
    binding: ConversationBinding,
    native_id: String,
}
impl Record {
    fn public(&self) -> PublicConversation {
        PublicConversation {
            conversation_id: self.conversation_id.clone(),
            provider: self.binding.provider.clone(),
            engine: self.binding.engine,
            model: self.binding.model.clone(),
            project_id: self.binding.project_id.clone(),
            project_version: self.binding.project_version.clone(),
            selected_slugs: self
                .binding
                .sources
                .iter()
                .map(|s| s.slug.clone())
                .collect(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversationErrorKind {
    Request,
    InvalidInput,
    InvalidRecord,
    Limit,
    Missing,
    Conflict,
    BindingChanged,
    NativeSession,
    Store,
}
/// Formatting is sanitized; original owned causes require explicit inspection.
pub struct ConversationError {
    kind: ConversationErrorKind,
    cause: Option<Box<dyn Error + Send + Sync>>,
}
impl ConversationError {
    fn new(kind: ConversationErrorKind) -> Self {
        Self { kind, cause: None }
    }
    fn caused(kind: ConversationErrorKind, cause: impl Error + Send + Sync + 'static) -> Self {
        Self {
            kind,
            cause: Some(Box::new(cause)),
        }
    }
    pub fn kind(&self) -> ConversationErrorKind {
        self.kind
    }
    /// Operational diagnostics, never public wire output or default logging.
    pub fn cause(&self) -> Option<&(dyn Error + Send + Sync + 'static)> {
        self.cause.as_deref()
    }
}
impl fmt::Debug for ConversationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConversationError")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for ConversationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Knowledge conversation: {:?}", self.kind)
    }
}
impl Error for ConversationError {}
type Result<T> = std::result::Result<T, ConversationError>;
fn request(e: io::Error) -> ConversationError {
    ConversationError::caused(ConversationErrorKind::Request, e)
}
fn artifact(e: graphit_core::artifact_store::ArtifactError) -> ConversationError {
    ConversationError::caused(ConversationErrorKind::Store, e)
}
fn original(expected: &SnapshotContext, current: &Arc<SnapshotContext>) -> Result<()> {
    if !expected.same_request(current) {
        return Err(ConversationError::new(ConversationErrorKind::Request));
    }
    current.check().map_err(request)
}
fn context(current: &SnapshotContext) -> ScopeContext {
    ScopeContext::from_snapshot(current)
}
fn path(scope: &CapturedPublicationScope, id: &str) -> Result<PathBuf> {
    let mut name = String::from("graphit-ai-conversation-");
    for byte in Sha256::digest(id.as_bytes()) {
        name.push(char::from(b"0123456789abcdef"[(byte >> 4) as usize]));
        name.push(char::from(b"0123456789abcdef"[(byte & 15) as usize]));
    }
    name.push_str(".json");
    Ok(scope.user_root().map_err(request)?.join(name))
}
fn find<'a>(
    snapshot: Option<&'a ArtifactSnapshot>,
    key: &ArtifactKey,
) -> Result<&'a ArtifactEntry> {
    snapshot
        .and_then(|s| s.entries().iter().find(|e| e.key() == key))
        .ok_or_else(|| ConversationError::new(ConversationErrorKind::Missing))
}
fn text(value: &str, limits: ConversationLimits, required: bool) -> Result<()> {
    if (required && value.is_empty()) || value.chars().any(char::is_control) {
        return Err(ConversationError::new(ConversationErrorKind::InvalidInput));
    }
    if value.len() > limits.field_bytes.get() {
        return Err(ConversationError::new(ConversationErrorKind::Limit));
    }
    Ok(())
}
fn binding(value: &ConversationBinding, limits: ConversationLimits) -> Result<()> {
    for field in [
        &value.provider,
        &value.model,
        &value.project_id,
        &value.project_version,
    ] {
        text(field, limits, true)?;
    }
    if !value.program.is_absolute() {
        return Err(ConversationError::new(ConversationErrorKind::InvalidInput));
    }
    if value.program.as_os_str().as_encoded_bytes().len() > limits.field_bytes.get()
        || value.sources.len() > limits.sources.get()
    {
        return Err(ConversationError::new(ConversationErrorKind::Limit));
    }
    for (index, source) in value.sources.iter().enumerate() {
        for field in [&source.slug, &source.source_path, &source.recipe] {
            text(field, limits, true)?;
        }
        for field in [&source.fallback_title, &source.kind_on_absent] {
            text(field, limits, false)?;
        }
        if value.sources[..index].iter().any(|s| s.slug == source.slug) {
            return Err(ConversationError::new(ConversationErrorKind::InvalidInput));
        }
    }
    Ok(())
}
struct Encoder {
    bytes: Vec<u8>,
    limit: usize,
}
impl io::Write for Encoder {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(io::Error::other)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encode(record: &Record, limits: ConversationLimits) -> Result<Vec<u8>> {
    let mut writer = Encoder {
        bytes: Vec::new(),
        limit: limits.record_bytes.get(),
    };
    serde_json::to_writer(&mut writer, record)
        .map_err(|e| ConversationError::caused(ConversationErrorKind::Limit, e))?;
    Ok(writer.bytes)
}

/// Mechanical persistence over a caller's real capture. The caller must pair
/// the capture with its Original and separately obtain current USER admissions.
pub struct KnowledgeConversationStore {
    store: ArtifactStore,
    original: Arc<SnapshotContext>,
    limits: ConversationLimits,
}
impl KnowledgeConversationStore {
    pub fn capture(
        scope: CapturedPublicationScope,
        current: Arc<SnapshotContext>,
        limits: ConversationLimits,
    ) -> Result<Self> {
        current.check().map_err(request)?;
        if !matches!(scope.owner(), StoreOwner::User { .. }) {
            return Err(ConversationError::new(ConversationErrorKind::InvalidInput));
        }
        scope
            .revalidate_bounded(&context(&current))
            .map_err(request)?;
        Ok(Self {
            store: ArtifactStore::new(scope, limits.artifacts),
            original: current,
            limits,
        })
    }
    fn check(&self, current: &Arc<SnapshotContext>) -> Result<()> {
        original(&self.original, current)?;
        self.store
            .scope()
            .revalidate_bounded(&context(current))
            .map_err(request)
    }
    fn key(&self, id: &str) -> Result<ArtifactKey> {
        let key = ArtifactKey::new(ADAPTER, TYPE, id)
            .map_err(|e| ConversationError::caused(ConversationErrorKind::InvalidInput, e))?;
        self.limits
            .artifacts
            .validate_key(&key)
            .map_err(|e| ConversationError::caused(ConversationErrorKind::Limit, e))?;
        Ok(key)
    }
    fn load(&self, current: &Arc<SnapshotContext>) -> Result<Option<ArtifactSnapshot>> {
        self.check(current)?;
        let observed = self.store.load().map_err(artifact);
        self.check(current)?;
        observed
    }
    fn directory(&self, current: &Arc<SnapshotContext>) -> Result<CapturedUserDocumentDirectory> {
        CapturedUserDocumentDirectory::capture(
            self.store.scope(),
            Path::new(""),
            self.limits.native,
            current,
        )
        .map_err(request)
    }
    fn read_entry(&self, entry: &ArtifactEntry, current: &Arc<SnapshotContext>) -> Result<Record> {
        self.check(current)?;
        let key = self.key(entry.key().id())?;
        let destination = path(self.store.scope(), key.id())?;
        if entry.key() != &key
            || entry.destination() != destination
            || entry.kind() != NodeKind::File
            || entry.mode() != ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
        {
            return Err(ConversationError::new(ConversationErrorKind::InvalidRecord));
        }
        let directory = self.directory(current)?;
        let name = destination
            .file_name()
            .ok_or_else(|| ConversationError::new(ConversationErrorKind::InvalidRecord))?;
        let observed = directory.read(name, current).map_err(request)?;
        let bytes = observed
            .bytes()
            .ok_or_else(|| ConversationError::new(ConversationErrorKind::InvalidRecord))?;
        if bytes.len() > self.limits.record_bytes.get() {
            return Err(ConversationError::new(ConversationErrorKind::Limit));
        }
        let record: Record = serde_json::from_slice(bytes)
            .map_err(|e| ConversationError::caused(ConversationErrorKind::InvalidRecord, e))?;
        if record.version != 1 || record.conversation_id != key.id() {
            return Err(ConversationError::new(ConversationErrorKind::InvalidRecord));
        }
        binding(&record.binding, self.limits)?;
        text(&record.native_id, self.limits, true)?;
        directory
            .revalidate_observation(&observed, current)
            .map_err(request)?;
        self.check(current)?;
        Ok(record)
    }
    pub fn read(&self, id: &str, current: &Arc<SnapshotContext>) -> Result<PublicConversation> {
        let key = self.key(id)?;
        let snapshot = self.load(current)?;
        Ok(self
            .read_entry(find(snapshot.as_ref(), &key)?, current)?
            .public())
    }
    pub fn list(&self, current: &Arc<SnapshotContext>) -> Result<Vec<PublicConversation>> {
        let snapshot = self.load(current)?;
        let mut output = Vec::new();
        if let Some(snapshot) = snapshot {
            for entry in snapshot
                .entries()
                .iter()
                .filter(|e| e.key().adapter() == ADAPTER && e.key().artifact_type() == TYPE)
            {
                current.check().map_err(request)?;
                if output.len() == self.limits.list_records.get() {
                    return Err(ConversationError::new(ConversationErrorKind::Limit));
                }
                output
                    .try_reserve(1)
                    .map_err(|e| ConversationError::caused(ConversationErrorKind::Limit, e))?;
                output.push(self.read_entry(entry, current)?.public());
            }
        }
        output.sort_by(|a, b| a.conversation_id.cmp(&b.conversation_id));
        self.check(current)?;
        Ok(output)
    }
    /// Reserve a new Graphit selector before any native execution, without a
    /// provisional persisted record or vendor-generated identity.
    pub fn begin_create(
        &self,
        id: &str,
        metadata: ConversationBinding,
        current: &Arc<SnapshotContext>,
    ) -> Result<ConversationTurn> {
        let key = self.key(id)?;
        binding(&metadata, self.limits)?;
        encode(
            &Record {
                version: 1,
                conversation_id: id.into(),
                binding: metadata.clone(),
                native_id: String::new(),
            },
            self.limits,
        )?;
        self.check(current)?;
        let writer = self.store.writer_with_context(current).map_err(artifact)?;
        let snapshot = writer.load().map_err(artifact)?;
        if snapshot
            .as_ref()
            .is_some_and(|s| s.entries().iter().any(|e| e.key() == &key))
        {
            return Err(ConversationError::new(ConversationErrorKind::Conflict));
        }
        let destination = path(self.store.scope(), id)?;
        let name = destination
            .file_name()
            .ok_or_else(|| ConversationError::new(ConversationErrorKind::InvalidInput))?;
        let directory = self.directory(current)?;
        let absent = directory.read(name, current).map_err(request)?;
        if absent.bytes().is_some() {
            return Err(ConversationError::new(ConversationErrorKind::Conflict));
        }
        directory
            .revalidate_observation(&absent, current)
            .map_err(request)?;
        self.check(current)?;
        Ok(ConversationTurn {
            writer,
            store: self.store.clone(),
            original: Arc::clone(current),
            limits: self.limits,
            key,
            binding: metadata,
            expected: None,
            observed: None,
            session_failed: false,
        })
    }
    pub fn begin_turn(
        &self,
        id: &str,
        metadata: &ConversationBinding,
        current: &Arc<SnapshotContext>,
    ) -> Result<ConversationTurn> {
        let key = self.key(id)?;
        binding(metadata, self.limits)?;
        self.check(current)?;
        let writer = self.store.writer_with_context(current).map_err(artifact)?;
        let snapshot = writer.load().map_err(artifact)?;
        let record = self.read_entry(find(snapshot.as_ref(), &key)?, current)?;
        if &record.binding != metadata {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        Ok(ConversationTurn {
            writer,
            store: self.store.clone(),
            original: Arc::clone(current),
            limits: self.limits,
            key,
            binding: record.binding,
            expected: Some(record.native_id),
            observed: None,
            session_failed: false,
        })
    }
    /// Resolve the private Kiro ID under the real USER writer without reading a corpus.
    /// The App must supply current USER/PROJECT admissions and retain custody until child join.
    pub fn begin_kiro_delete(
        &self,
        id: &str,
        target: KiroConversationTarget<'_>,
        current: &Arc<SnapshotContext>,
    ) -> Result<ConversationTurn> {
        current.required_deadline().map_err(request)?;
        self.check(current)?;
        let key = self.key(id)?;
        for field in [target.model, target.project_id, target.project_version] {
            text(field, self.limits, true)?;
        }
        if !matches!(target.engine, ConversationEngine::KiroV3) || !target.program.is_absolute() {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        if target.program.as_os_str().as_encoded_bytes().len() > self.limits.field_bytes.get() {
            return Err(ConversationError::new(ConversationErrorKind::Limit));
        }
        let writer = self.store.writer_with_context(current).map_err(artifact)?;
        let snapshot = writer.load().map_err(artifact)?;
        let record = self.read_entry(find(snapshot.as_ref(), &key)?, current)?;
        if record.binding.provider != "kiro"
            || record.binding.engine != target.engine
            || record.binding.model != target.model
            || record.binding.program != target.program
            || record.binding.project_id != target.project_id
            || record.binding.project_version != target.project_version
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        Ok(ConversationTurn {
            writer,
            store: self.store.clone(),
            original: Arc::clone(current),
            limits: self.limits,
            key,
            binding: record.binding,
            expected: Some(record.native_id),
            observed: None,
            session_failed: false,
        })
    }
    pub fn delete(&self, id: &str, current: &Arc<SnapshotContext>) -> Result<()> {
        let key = self.key(id)?;
        self.check(current)?;
        let mut writer = self.store.writer_with_context(current).map_err(artifact)?;
        let snapshot = writer.load().map_err(artifact)?;
        self.read_entry(find(snapshot.as_ref(), &key)?, current)?;
        writer.remove(&key).map_err(artifact)?;
        self.check(current)
    }
}

/// Owns the actual USER kernel writer for the entire turn. App must transfer
/// it to durable custody before the first execution poll, preserving it while
/// any RemainingProcess still owns unfinished native execution.
pub struct ConversationTurn {
    writer: ArtifactWriter,
    store: ArtifactStore,
    original: Arc<SnapshotContext>,
    limits: ConversationLimits,
    key: ArtifactKey,
    binding: ConversationBinding,
    expected: Option<String>,
    observed: Option<String>,
    session_failed: bool,
}
impl ConversationTurn {
    /// Select Create or Load of the held private OpenCode identity without a stored-ID getter.
    pub fn configure_opencode_session(
        &self,
        program: &Path,
        model: &str,
        cwd: &Path,
        current: &Arc<SnapshotContext>,
    ) -> Result<crate::cli_opencode::OpencodeSessionStart> {
        original(&self.original, current)?;
        current.required_deadline().map_err(request)?;
        if self.binding.engine != ConversationEngine::OpenCode
            || self.binding.provider != "opencode"
            || program != self.binding.program
            || model != self.binding.model
            || self.session_failed
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        if !cwd.is_absolute() {
            return Err(ConversationError::new(ConversationErrorKind::InvalidInput));
        }
        if cwd.as_os_str().as_encoded_bytes().len() > self.limits.field_bytes.get() {
            return Err(ConversationError::new(ConversationErrorKind::Limit));
        }
        Ok(match &self.expected {
            Some(id) => crate::cli_opencode::OpencodeSessionStart::Load(
                agent_client_protocol::schema::v1::SessionId::new(id.as_str()),
            ),
            None => crate::cli_opencode::OpencodeSessionStart::Create,
        })
    }
    /// Create confirms the native response; Load confirms the SDK ACK bound to
    /// the already observed ID, without an invented response ID or interpretation of ACP metadata.
    pub fn observe_opencode_session(
        &mut self,
        id: &str,
        loaded: bool,
        current: &Arc<SnapshotContext>,
    ) -> Result<()> {
        if self.binding.engine != ConversationEngine::OpenCode
            || self.binding.provider != "opencode"
            || loaded != self.expected.is_some()
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        self.observe_session(id, current)
    }
    /// Select the exact private native session, without exposing a stored-ID getter.
    pub fn configure_kimi_session(
        &self,
        program: &Path,
        model: &str,
        cwd: &Path,
        current: &Arc<SnapshotContext>,
    ) -> Result<crate::cli_kimi::KimiSessionStart> {
        original(&self.original, current)?;
        current.required_deadline().map_err(request)?;
        if self.binding.engine != ConversationEngine::Kimi
            || self.binding.provider != "kimi"
            || program != self.binding.program
            || model != self.binding.model
            || self.session_failed
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        if !cwd.is_absolute() {
            return Err(ConversationError::new(ConversationErrorKind::InvalidInput));
        }
        if cwd.as_os_str().as_encoded_bytes().len() > self.limits.field_bytes.get() {
            return Err(ConversationError::new(ConversationErrorKind::Limit));
        }
        Ok(match &self.expected {
            Some(id) => crate::cli_kimi::KimiSessionStart::Resume(id.clone()),
            None => crate::cli_kimi::KimiSessionStart::Create,
        })
    }
    /// The native session response and correlated main turn establish this identity.
    pub fn observe_kimi_session(&mut self, id: &str, current: &Arc<SnapshotContext>) -> Result<()> {
        if self.binding.engine != ConversationEngine::Kimi || self.binding.provider != "kimi" {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        self.observe_session(id, current)
    }
    /// Select Create or resume of the held private Copilot identity without a stored-ID getter.
    pub fn configure_copilot_session(
        &self,
        program: &Path,
        model: &str,
        cwd: &Path,
        current: &Arc<SnapshotContext>,
    ) -> Result<crate::cli_copilot::CopilotSessionStart> {
        original(&self.original, current)?;
        current.required_deadline().map_err(request)?;
        if self.binding.engine != ConversationEngine::Copilot
            || self.binding.provider != "copilot"
            || program != self.binding.program
            || model != self.binding.model
            || self.session_failed
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        if !cwd.is_absolute() {
            return Err(ConversationError::new(ConversationErrorKind::InvalidInput));
        }
        if cwd.as_os_str().as_encoded_bytes().len() > self.limits.field_bytes.get() {
            return Err(ConversationError::new(ConversationErrorKind::Limit));
        }
        Ok(match &self.expected {
            Some(id) => crate::cli_copilot::CopilotSessionStart::Resume(id.to_owned()),
            None => crate::cli_copilot::CopilotSessionStart::Create,
        })
    }
    /// Observe the exact native CLI result session after a successful turn.
    /// Resume must confirm the previously held ID; flags alone are not confirmation.
    pub fn observe_copilot_session(
        &mut self,
        id: &str,
        resumed: bool,
        current: &Arc<SnapshotContext>,
    ) -> Result<()> {
        if self.binding.engine != ConversationEngine::Copilot
            || self.binding.provider != "copilot"
            || resumed != self.expected.is_some()
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        self.observe_session(id, current)
    }
    /// Build supported literal Codex exec/resume arguments without a native ID getter.
    pub fn configure_codex_request(
        &self,
        command: &mut graphit_process::Request,
        current: &Arc<SnapshotContext>,
    ) -> Result<()> {
        original(&self.original, current)?;
        if self.binding.engine != ConversationEngine::Codex
            || command.program != self.binding.program
            || command.stdin.is_some()
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        let models: Vec<_> = command
            .args
            .windows(2)
            .filter(|pair| pair[0] == "--model")
            .collect();
        if command.args.first().is_none_or(|arg| arg != "exec")
            || command.args.last().is_none_or(|arg| arg != "-")
            || models.len() != 1
            || models[0][1] != self.binding.model.as_str()
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        if let Some(id) = &self.expected {
            let position = command.args.len() - 1;
            command.args.insert(position, OsString::from("resume"));
            command.args.insert(position + 1, OsString::from(id));
        }
        Ok(())
    }
    /// Configure the captured print request with the expected private Claude session.
    pub fn configure_claude_request(
        &self,
        command: &mut graphit_process::Request,
        current: &Arc<SnapshotContext>,
    ) -> Result<()> {
        original(&self.original, current)?;
        if self.binding.engine != ConversationEngine::Claude
            || self.binding.provider != "claude"
            || command.program != self.binding.program
            || command.stdin.is_none()
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        if !command.cwd.is_absolute() {
            return Err(ConversationError::new(ConversationErrorKind::InvalidInput));
        }
        if command.cwd.as_os_str().as_encoded_bytes().len() > self.limits.field_bytes.get() {
            return Err(ConversationError::new(ConversationErrorKind::Limit));
        }
        let args = &command.args;
        if args.len() != 7
            || args[0] != "-p"
            || args[1] != "--output-format"
            || args[2] != "stream-json"
            || args[3] != "--verbose"
            || args[4] != "--include-partial-messages"
            || args[5] != "--model"
            || args[6] != self.binding.model.as_str()
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        if let Some(id) = &self.expected {
            command
                .args
                .try_reserve(2)
                .map_err(|error| ConversationError::caused(ConversationErrorKind::Limit, error))?;
            command
                .args
                .extend([OsString::from("--resume"), OsString::from(id)]);
        }
        Ok(())
    }
    /// Configure the captured native stdin request with the exact private Antigravity conversation.
    pub fn configure_antigravity_request(
        &self,
        command: &mut graphit_process::Request,
        current: &Arc<SnapshotContext>,
    ) -> Result<()> {
        original(&self.original, current)?;
        if self.binding.engine != ConversationEngine::Antigravity
            || self.binding.provider != "antigravity"
            || command.program != self.binding.program
            || command.stdin.is_none()
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        if !command.cwd.is_absolute() {
            return Err(ConversationError::new(ConversationErrorKind::InvalidInput));
        }
        if command.cwd.as_os_str().as_encoded_bytes().len() > self.limits.field_bytes.get() {
            return Err(ConversationError::new(ConversationErrorKind::Limit));
        }
        let args = &command.args;
        if args.len() != 6
            || args[0] != "--input-format"
            || args[1] != "stream-json"
            || args[2] != "--output-format"
            || args[3] != "stream-json"
            || args[4] != "--model"
            || args[5] != self.binding.model.as_str()
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        if let Some(id) = &self.expected {
            command
                .args
                .try_reserve(2)
                .map_err(|error| ConversationError::caused(ConversationErrorKind::Limit, error))?;
            command
                .args
                .extend([OsString::from("--conversation"), OsString::from(id)]);
        }
        Ok(())
    }
    /// Configure the captured print request with the expected private Qwen session.
    pub fn configure_qwen_request(
        &self,
        command: &mut graphit_process::Request,
        current: &Arc<SnapshotContext>,
    ) -> Result<()> {
        original(&self.original, current)?;
        if self.binding.engine != ConversationEngine::Qwen
            || self.binding.provider != "qwen"
            || command.program != self.binding.program
            || command.stdin.is_none()
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        if !command.cwd.is_absolute() {
            return Err(ConversationError::new(ConversationErrorKind::InvalidInput));
        }
        if command.cwd.as_os_str().as_encoded_bytes().len() > self.limits.field_bytes.get() {
            return Err(ConversationError::new(ConversationErrorKind::Limit));
        }
        let args = &command.args;
        if args.len() != 7
            || args[0] != "--input-format"
            || args[1] != "text"
            || args[2] != "--output-format"
            || args[3] != "stream-json"
            || args[4] != "--include-partial-messages"
            || args[5] != "--model"
            || args[6] != self.binding.model.as_str()
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        if let Some(id) = &self.expected {
            command
                .args
                .try_reserve(2)
                .map_err(|error| ConversationError::caused(ConversationErrorKind::Limit, error))?;
            command
                .args
                .extend([OsString::from("--resume"), OsString::from(id)]);
        }
        Ok(())
    }
    /// Configure the captured print request with the expected private Cursor session.
    pub fn configure_cursor_request(
        &self,
        command: &mut graphit_process::Request,
        current: &Arc<SnapshotContext>,
    ) -> Result<()> {
        original(&self.original, current)?;
        if self.binding.engine != ConversationEngine::Cursor
            || self.binding.provider != "cursor"
            || command.program != self.binding.program
            || command.stdin.is_some()
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        if !command.cwd.is_absolute() {
            return Err(ConversationError::new(ConversationErrorKind::InvalidInput));
        }
        if command.cwd.as_os_str().as_encoded_bytes().len() > self.limits.field_bytes.get() {
            return Err(ConversationError::new(ConversationErrorKind::Limit));
        }
        let args = &command.args;
        if args.len() != 8
            || args[0] != "--print"
            || args[1] != "--output-format"
            || args[2] != "stream-json"
            || args[3] != "--model"
            || args[4] != self.binding.model.as_str()
            || args[5] != "--mode"
            || args[6] != "ask"
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        if let Some(id) = &self.expected {
            command
                .args
                .try_reserve(2)
                .map_err(|error| ConversationError::caused(ConversationErrorKind::Limit, error))?;
            command.args.insert(7, OsString::from("--resume"));
            command.args.insert(8, OsString::from(id));
        }
        Ok(())
    }
    /// Configure the captured print request with the expected private Grok session.
    pub fn configure_grok_request(
        &self,
        command: &mut graphit_process::Request,
        prompt_file: &Path,
        current: &Arc<SnapshotContext>,
    ) -> Result<()> {
        original(&self.original, current)?;
        if self.binding.engine != ConversationEngine::Grok
            || self.binding.provider != "grok"
            || command.program != self.binding.program
            || command.stdin.is_some()
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        if !command.cwd.is_absolute() || !prompt_file.is_absolute() {
            return Err(ConversationError::new(ConversationErrorKind::InvalidInput));
        }
        if command.cwd.as_os_str().as_encoded_bytes().len() > self.limits.field_bytes.get() {
            return Err(ConversationError::new(ConversationErrorKind::Limit));
        }
        let args = &command.args;
        if args.len() != 9
            || args[0] != "--prompt-file"
            || args[1] != prompt_file.as_os_str()
            || args[2] != "--output-format"
            || args[3] != "streaming-json"
            || args[4] != "--model"
            || args[5] != self.binding.model.as_str()
            || args[6] != "--verbatim"
            || args[7] != "--deny"
            || args[8] != "*"
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        if let Some(id) = &self.expected {
            if id.len() != 36 || uuid::Uuid::try_parse(id).is_err() {
                return Err(ConversationError::new(ConversationErrorKind::NativeSession));
            }
            command
                .args
                .try_reserve(2)
                .map_err(|error| ConversationError::caused(ConversationErrorKind::Limit, error))?;
            command
                .args
                .extend([OsString::from("--resume"), OsString::from(id)]);
        }
        Ok(())
    }
    /// Build New or the expected private Load request. The App supplies the
    /// admitted PROJECT cwd and must still own and join the native child.
    pub fn configure_kiro_session(
        &self,
        program: &Path,
        cwd: &Path,
        current: &Arc<SnapshotContext>,
    ) -> Result<crate::cli_kiro::KiroSessionStart> {
        use crate::cli_kiro::KiroSessionStart;
        use agent_client_protocol::schema::v1::{LoadSessionRequest, NewSessionRequest};
        original(&self.original, current)?;
        current.required_deadline().map_err(request)?;
        if !matches!(self.binding.engine, ConversationEngine::KiroV3)
            || program != self.binding.program
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        if !cwd.is_absolute() {
            return Err(ConversationError::new(ConversationErrorKind::InvalidInput));
        }
        if cwd.as_os_str().as_encoded_bytes().len() > self.limits.field_bytes.get() {
            return Err(ConversationError::new(ConversationErrorKind::Limit));
        }
        // Kiro receives its initial model on both New and Load.
        let meta = Some({
            let mut model = serde_json::Map::new();
            model.insert("modelId".into(), self.binding.model.clone().into());
            let mut meta = serde_json::Map::new();
            meta.insert("kiro".into(), model.into());
            meta
        });
        Ok(match &self.expected {
            Some(id) => KiroSessionStart::Load(LoadSessionRequest::new(id.clone(), cwd).meta(meta)),
            None => KiroSessionStart::New(NewSessionRequest::new(cwd).meta(meta)),
        })
    }
    /// Build the SDK deletion request from the expected private persisted identity.
    pub fn configure_kiro_delete(
        &self,
        program: &Path,
        current: &Arc<SnapshotContext>,
    ) -> Result<agent_client_protocol::schema::v1::DeleteSessionRequest> {
        original(&self.original, current)?;
        current.required_deadline().map_err(request)?;
        if !matches!(self.binding.engine, ConversationEngine::KiroV3)
            || self.binding.provider != "kiro"
            || self.binding.program != program
            || self.session_failed
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        let id = self
            .expected
            .as_ref()
            .ok_or_else(|| ConversationError::new(ConversationErrorKind::NativeSession))?;
        Ok(agent_client_protocol::schema::v1::DeleteSessionRequest::new(id.clone()))
    }
    /// Consume the real native ThreadStarted view. Mismatch remains failure
    /// even when the vendor executed a newly created session.
    pub fn observe_thread_started(
        &mut self,
        id: &str,
        current: &Arc<SnapshotContext>,
    ) -> Result<()> {
        if self.binding.engine != ConversationEngine::Codex {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        self.observe_session(id, current)
    }
    /// Observe the decoded Claude Init without exposing the persisted native identity.
    pub fn observe_claude_session(
        &mut self,
        id: &str,
        current: &Arc<SnapshotContext>,
    ) -> Result<()> {
        if self.binding.engine != ConversationEngine::Claude || self.binding.provider != "claude" {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        self.observe_session(id, current)
    }
    /// Observe the decoded Qwen Init without exposing the persisted native identity.
    pub fn observe_antigravity_session(
        &mut self,
        id: &str,
        current: &Arc<SnapshotContext>,
    ) -> Result<()> {
        if self.binding.engine != ConversationEngine::Antigravity
            || self.binding.provider != "antigravity"
        {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        self.observe_session(id, current)
    }
    /// Observe the decoded Cursor Init without exposing the persisted native identity.
    pub fn observe_qwen_session(&mut self, id: &str, current: &Arc<SnapshotContext>) -> Result<()> {
        if self.binding.engine != ConversationEngine::Qwen || self.binding.provider != "qwen" {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        self.observe_session(id, current)
    }
    /// Observe the decoded Cursor Init without exposing the persisted native identity.
    pub fn observe_cursor_session(
        &mut self,
        id: &str,
        current: &Arc<SnapshotContext>,
    ) -> Result<()> {
        if self.binding.engine != ConversationEngine::Cursor || self.binding.provider != "cursor" {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        self.observe_session(id, current)
    }
    /// Observe the decoded Grok End without exposing the persisted native identity.
    pub fn observe_grok_session(&mut self, id: &str, current: &Arc<SnapshotContext>) -> Result<()> {
        if self.binding.engine != ConversationEngine::Grok || self.binding.provider != "grok" {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        original(&self.original, current)?;
        if id.len() != 36 || uuid::Uuid::try_parse(id).is_err() {
            self.session_failed = true;
            return Err(ConversationError::new(ConversationErrorKind::InvalidInput));
        }
        self.observe_session(id, current)
    }
    /// Consume the actual ACP session identity without exposing a stored ID.
    pub fn observe_kiro_session(
        &mut self,
        id: &agent_client_protocol::schema::v1::SessionId,
        current: &Arc<SnapshotContext>,
    ) -> Result<()> {
        if !matches!(self.binding.engine, ConversationEngine::KiroV3) {
            return Err(ConversationError::new(
                ConversationErrorKind::BindingChanged,
            ));
        }
        self.observe_session(&id.0, current)
    }
    fn observe_session(&mut self, id: &str, current: &Arc<SnapshotContext>) -> Result<()> {
        original(&self.original, current)?;
        if self.session_failed {
            return Err(ConversationError::new(ConversationErrorKind::NativeSession));
        }
        if let Err(error) = text(id, self.limits, true) {
            self.session_failed = true;
            return Err(error);
        }
        if self.expected.as_deref().is_some_and(|e| e != id)
            || self.observed.as_deref().is_some_and(|o| o != id)
        {
            self.session_failed = true;
            return Err(ConversationError::new(ConversationErrorKind::NativeSession));
        }
        if self.observed.is_none() {
            self.observed = Some(id.into());
        }
        Ok(())
    }
    /// Remove the registered Graphit copy while retaining this writer.
    /// The caller first proves SDK deletion, authorization and actual native child cleanup.
    pub fn delete_kiro_record(&mut self, current: &Arc<SnapshotContext>) -> Result<()> {
        original(&self.original, current)?;
        current.required_deadline().map_err(request)?;
        if self.expected.is_none()
            || !matches!(self.binding.engine, ConversationEngine::KiroV3)
            || self.binding.provider != "kiro"
            || self.session_failed
        {
            return Err(ConversationError::new(ConversationErrorKind::NativeSession));
        }
        self.writer.remove(&self.key).map_err(artifact)?;
        original(&self.original, current)
    }
    /// Commit observed binding; an error leaves the owned guard with the caller.
    /// This operation cannot establish that Process cleanup is terminal.
    pub fn commit(&mut self, current: &Arc<SnapshotContext>) -> Result<PublicConversation> {
        original(&self.original, current)?;
        if self.session_failed {
            return Err(ConversationError::new(ConversationErrorKind::NativeSession));
        }
        let native = self
            .observed
            .as_ref()
            .ok_or_else(|| ConversationError::new(ConversationErrorKind::NativeSession))?;
        let record = Record {
            version: 1,
            conversation_id: self.key.id().into(),
            binding: self.binding.clone(),
            native_id: native.clone(),
        };
        let bytes = encode(&record, self.limits)?;
        let mode = PayloadMode::private_file();
        let policy = PayloadPolicy::new(NodeKind::File, mode, self.limits.artifacts.copy_limits());
        let plan = ValidatedPayloadPlan::generated(
            [GeneratedNode::file(PathBuf::new(), bytes, mode).map_err(request)?],
            &policy,
            current,
        )
        .map_err(request)?;
        self.writer
            .publish_generated(
                &self.key,
                &plan,
                &path(self.store.scope(), self.key.id())?,
                NodeKind::File,
                ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
            )
            .map_err(artifact)?;
        original(&self.original, current)?;
        Ok(record.public())
    }
}
