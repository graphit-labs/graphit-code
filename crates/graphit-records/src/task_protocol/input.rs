use crate::{
    task_spec::{CheckId, CheckKind, FieldUpdate, TaskId},
    task_store::*,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
use std::{
    num::{NonZeroU64, NonZeroUsize},
    time::Duration,
};

/// A participant fence encoded explicitly for transport, never ordinary diagnostics.
#[derive(Clone, Eq, PartialEq)]
pub struct TransportToken(ClaimToken);
impl TransportToken {
    pub fn from_claim(token: ClaimToken) -> Self {
        Self(token)
    }
    pub fn claim(&self) -> &ClaimToken {
        &self.0
    }
}
impl Serialize for TransportToken {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&URL_SAFE_NO_PAD.encode(self.0.as_bytes()))
    }
}
impl<'de> Deserialize<'de> for TransportToken {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text.len() != 43 {
            return Err(D::Error::custom("invalid claim token"));
        }
        let mut bytes = [0u8; 32];
        let decoded = URL_SAFE_NO_PAD
            .decode_slice(&text, &mut bytes)
            .map_err(|_| D::Error::custom("invalid claim token"))?;
        if decoded != bytes.len() {
            return Err(D::Error::custom("invalid claim token"));
        }
        if URL_SAFE_NO_PAD.encode(bytes) != text {
            return Err(D::Error::custom("noncanonical claim token"));
        }
        Ok(Self(ClaimToken::from_bytes(bytes)))
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LeaseMillis(pub NonZeroU64);
impl LeaseMillis {
    pub fn duration(self) -> Duration {
        Duration::from_millis(self.0.get())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fence {
    pub token: TransportToken,
    pub expected_revision: u64,
}
impl Fence {
    pub fn update(&self) -> ClaimUpdate<'_> {
        ClaimUpdate {
            token: self.token.claim(),
            expected_revision: self.expected_revision,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnedTarget<T> {
    pub id: T,
    pub claim: Fence,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Heartbeat<T> {
    pub target: OwnedTarget<T>,
    pub lease_ms: LeaseMillis,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressInput<T> {
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub references: FieldUpdate<serde_json::Value>,
    pub target: OwnedTarget<T>,
    pub lease_ms: LeaseMillis,
    pub summary: String,
    pub next_step: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseInput<T> {
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub references: FieldUpdate<serde_json::Value>,
    pub target: OwnedTarget<T>,
    pub summary: String,
    pub next_step: String,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimInput<T> {
    pub id: T,
    pub lease_ms: LeaseMillis,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exact<T> {
    pub id: T,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueInput {
    pub filter: CatalogueFilter,
    pub page_size: NonZeroUsize,
    pub cursor: Option<CatalogueCursor>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    content = "input",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ReadRequest {
    GetTask(TaskGetInput),
    GetSession(SessionGetInput),
    Catalogue(CatalogueInput),
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    Search(TaskLexicalQuery),
    Query(TaskQuery),
    Schema,
    ReadView,
    SessionHeads,
}
fn keep<T>(value: &FieldUpdate<T>) -> bool {
    matches!(value, FieldUpdate::Keep)
}
fn serialize_update<T: Serialize, S: Serializer>(
    value: &FieldUpdate<T>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        FieldUpdate::Keep => serializer.serialize_none(),
        FieldUpdate::Set(value) => value.serialize(serializer),
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionInput {
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub references: FieldUpdate<serde_json::Value>,
    pub target: OwnedTarget<TaskId>,
    pub lease_ms: LeaseMillis,
    pub reason: String,
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub title: FieldUpdate<String>,
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub description: FieldUpdate<String>,
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub task_type: FieldUpdate<String>,
    pub priority: Option<u8>,
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub schedule: FieldUpdate<Option<crate::task_schedule::TaskSchedule>>,
    #[serde(default)]
    pub add_checks: Vec<CreateCheck>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionRevisionInput {
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub references: FieldUpdate<serde_json::Value>,
    pub target: OwnedTarget<SessionId>,
    pub lease_ms: LeaseMillis,
    pub reason: String,
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub title: FieldUpdate<String>,
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub description: FieldUpdate<String>,
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub strategy: FieldUpdate<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckInput {
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub references: FieldUpdate<serde_json::Value>,
    pub target: OwnedTarget<TaskId>,
    pub lease_ms: LeaseMillis,
    pub check_id: CheckId,
    pub passed: bool,
    pub evidence: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replacement {
    pub text: String,
    pub kind: Option<CheckKind>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupersessionInput {
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub references: FieldUpdate<serde_json::Value>,
    pub target: OwnedTarget<TaskId>,
    pub lease_ms: LeaseMillis,
    pub check_id: CheckId,
    pub reason: String,
    pub replacement: Option<Replacement>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinksInput {
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub references: FieldUpdate<serde_json::Value>,
    pub target: OwnedTarget<TaskId>,
    pub lease_ms: LeaseMillis,
    pub reason: String,
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub parent_id: FieldUpdate<Option<TaskId>>,
    pub depends_on: Option<Vec<TaskId>>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlagInput {
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub references: FieldUpdate<serde_json::Value>,
    pub target: OwnedTarget<TaskId>,
    pub lease_ms: LeaseMillis,
    pub flagged: bool,
    pub reason: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelInput {
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub references: FieldUpdate<serde_json::Value>,
    pub id: TaskId,
    pub claim: Option<Fence>,
    pub reason: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCommitInput {
    pub id: TaskId,
    pub expected_revision: u64,
    pub claim: Option<Fence>,
    // Required on updates: null explicitly clears the association.
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub commit: FieldUpdate<Option<String>>,
    pub reason: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommentInput {
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub references: FieldUpdate<serde_json::Value>,
    pub target: OwnedTarget<TaskId>,
    pub lease_ms: LeaseMillis,
    pub idempotency_key: String,
    pub kind: crate::task_comment::TaskCommentKind,
    pub body: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionInput<T> {
    pub id: T,
    pub claim: Fence,
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub references: FieldUpdate<serde_json::Value>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCreationInput {
    pub commit: Option<String>,
    pub idempotency_key: String,
    pub session_id: SessionId,
    pub parent_id: Option<TaskId>,
    #[serde(default)]
    pub depends_on: Vec<TaskId>,
    pub title: String,
    pub description: String,
    pub task_type: String,
    pub priority: u8,
    pub schedule: Option<crate::task_schedule::TaskSchedule>,
    pub checks: Vec<CreateCheck>,
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub references: FieldUpdate<serde_json::Value>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCreationInput {
    pub idempotency_key: String,
    pub title: String,
    pub description: String,
    pub strategy: String,
    #[serde(
        default,
        skip_serializing_if = "keep",
        serialize_with = "serialize_update"
    )]
    pub references: FieldUpdate<serde_json::Value>,
}
impl TaskCreationInput {
    pub(super) fn into_native(self) -> CreateTask {
        CreateTask {
            commit: self.commit,
            idempotency_key: self.idempotency_key,
            session_id: self.session_id,
            parent_id: self.parent_id,
            depends_on: self.depends_on,
            title: self.title,
            description: self.description,
            task_type: self.task_type,
            priority: self.priority,
            schedule: self.schedule,
            checks: self.checks,
        }
    }
}
impl SessionCreationInput {
    pub(super) fn into_native(self) -> CreateSession {
        CreateSession {
            idempotency_key: self.idempotency_key,
            title: self.title,
            description: self.description,
            strategy: self.strategy,
        }
    }
}

/// Native operations only; inputs cannot choose an actor, scope, branch or admission class.
#[derive(Serialize, Deserialize)]
#[serde(
    tag = "operation",
    content = "input",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum WriteRequest {
    AddTaskComment(CommentInput),
    CreateTask(TaskCreationInput),
    CreateSession(SessionCreationInput),
    ClaimTask(ClaimInput<TaskId>),
    ClaimSession(ClaimInput<SessionId>),
    HeartbeatTask(Heartbeat<TaskId>),
    HeartbeatSession(Heartbeat<SessionId>),
    ProgressTask(ProgressInput<TaskId>),
    CheckpointSession(ProgressInput<SessionId>),
    ReleaseTask(ReleaseInput<TaskId>),
    ReleaseSession(ReleaseInput<SessionId>),
    ReviseTask(RevisionInput),
    ReviseSession(SessionRevisionInput),
    VerifyTaskCheck(CheckInput),
    SupersedeTaskCheck(SupersessionInput),
    UpdateTaskLinks(LinksInput),
    SetTaskFlag(FlagInput),
    SetTaskCommit(TaskCommitInput),
    CompleteTask(CompletionInput<TaskId>),
    CancelTask(CancelInput),
    CompleteSession(CompletionInput<SessionId>),
}
impl WriteRequest {
    pub(super) fn take_references(&mut self) -> FieldUpdate<serde_json::Value> {
        let field = match self {
            Self::AddTaskComment(input) => &mut input.references,
            Self::CreateTask(input) => &mut input.references,
            Self::CreateSession(input) => &mut input.references,
            Self::ProgressTask(input) => &mut input.references,
            Self::CheckpointSession(input) => &mut input.references,
            Self::ReleaseTask(input) => &mut input.references,
            Self::ReleaseSession(input) => &mut input.references,
            Self::ReviseTask(input) => &mut input.references,
            Self::ReviseSession(input) => &mut input.references,
            Self::VerifyTaskCheck(input) => &mut input.references,
            Self::SupersedeTaskCheck(input) => &mut input.references,
            Self::UpdateTaskLinks(input) => &mut input.references,
            Self::SetTaskFlag(input) => &mut input.references,
            Self::CompleteTask(input) => &mut input.references,
            Self::CancelTask(input) => &mut input.references,
            Self::CompleteSession(input) => &mut input.references,
            Self::SetTaskCommit(_)
            | Self::ClaimTask(_)
            | Self::ClaimSession(_)
            | Self::HeartbeatTask(_)
            | Self::HeartbeatSession(_) => return FieldUpdate::Keep,
        };
        std::mem::take(field)
    }

    pub fn required_operation(&self) -> graphit_auth::ProjectWriteOperation {
        use graphit_auth::ProjectWriteOperation as Op;
        match self {
            Self::CreateTask(_) | Self::CreateSession(_) => Op::Task,
            Self::ClaimSession(_)
            | Self::ReviseSession(_)
            | Self::HeartbeatSession(_)
            | Self::CheckpointSession(_)
            | Self::ReleaseSession(_)
            | Self::CompleteSession(_) => Op::CoordinateTask,
            Self::SetTaskCommit(input) if input.claim.is_none() => Op::Task,
            Self::CancelTask(input) if input.claim.is_none() => Op::Task,
            _ => Op::OwnTask,
        }
    }
}
