use super::references::{TaskReference, TaskReferenceUpdate};
pub use crate::task_comment::{TaskComment, TaskCommentId, TaskCommentKind};
use crate::{
    gates::TaskStatus,
    task_schedule::TaskSchedule,
    task_spec::{CheckId, CheckKind, FieldUpdate, TaskId, TaskSpecification},
};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::num::NonZeroU128;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SessionId(NonZeroU128);
impl SessionId {
    pub const fn new(value: NonZeroU128) -> Self {
        Self(value)
    }
    pub const fn get(self) -> NonZeroU128 {
        self.0
    }
}
impl Serialize for SessionId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&ulid::Ulid::from(self.0.get()))
    }
}
impl<'de> Deserialize<'de> for SessionId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        crate::task_spec::deserialize_identifier(deserializer).map(Self)
    }
}
pub(crate) fn id(value: NonZeroU128) -> String {
    ulid::Ulid::from(value.get()).to_string()
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSession {
    pub idempotency_key: String,
    pub title: String,
    pub description: String,
    pub strategy: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCheck {
    pub kind: CheckKind,
    pub text: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTask {
    /// Full Git object ID associated with this delivery, when applicable.
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
    pub schedule: Option<TaskSchedule>,
    pub checks: Vec<CreateCheck>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub id: SessionId,
    pub input: CreateSession,
    pub revision: u64,
    pub references: Vec<TaskReference>,
    pub reference_generation: u64,
    pub status: TaskStatus,
    pub ownership: Ownership,
    pub progress: Option<Progress>,
    pub created_at: u64,
    pub updated_at: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    /// Latest associated commit; older associations remain in lifecycle history.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub specification: TaskSpecification,
    pub session_id: SessionId,
    pub parent_id: Option<TaskId>,
    pub depends_on: Vec<TaskId>,
    pub references: Vec<TaskReference>,
    pub reference_generation: u64,
    pub idempotency_key: String,
    pub status: TaskStatus,
    pub flagged: bool,
    pub ownership: Ownership,
    pub progress: Option<Progress>,
    pub created_at: u64,
    pub updated_at: u64,
}
/// Server-derived attribution and the immutable initial state, committed together.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Creation<T> {
    pub actor: String,
    pub record: T,
    pub references_update: TaskReferenceUpdate,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaskDetail {
    pub task: Task,
    pub creation: Creation<Task>,
    pub events: Vec<LifecycleEvent<Task>>,
    pub comments: Vec<TaskComment>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaskSummary {
    pub id: TaskId,
    pub session_id: SessionId,
    pub title: String,
    pub task_type: String,
    pub priority: u8,
    pub status: TaskStatus,
    pub parent_id: Option<TaskId>,
    pub depends_on: Vec<TaskId>,
    pub schedule: Option<TaskSchedule>,
    pub ownership: Ownership,
    pub progress: Option<ProgressObservation>,
    pub flagged: bool,
    pub ready: bool,
    pub revision: u64,
    pub created_at: u64,
    pub updated_at: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionHead {
    pub id: SessionId,
    pub title: String,
    pub status: TaskStatus,
    pub revision: u64,
    pub ownership: Ownership,
    pub progress: Option<ProgressObservation>,
    pub completed_tasks: usize,
    /// Progress denominator: linked tasks other than Cancelled.
    pub total_tasks: usize,
    pub updated_at: u64,
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogueFilter {
    pub query: Option<String>,
    #[schemars(default, schema_with = "super::query_model::optional_status_schema")]
    pub status: Option<TaskStatus>,
    #[schemars(with = "Option<super::query_model::QueryIdentifierSchema>")]
    pub session_id: Option<SessionId>,
    pub flagged: Option<bool>,
    pub blocked: Option<bool>,
}
/// Opaque continuation data; it grants no authority and cannot select a branch.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogueCursor {
    pub(crate) corpus_key: String,
    #[schemars(with = "super::query_model::QueryIdentifierSchema")]
    pub(crate) project_id: graphit_core::project_identity::ProjectId,
    pub(crate) brand: String,
    #[schemars(schema_with = "native_path_schema")]
    pub(crate) root: graphit_core::native_path::NativePath,
    pub(crate) project_version: String,
    pub(crate) version: u64,
    pub(crate) filter: CatalogueFilter,
    pub(crate) physical_branch: Option<String>,
    pub(crate) inherited_target: Option<String>,
    pub(crate) after_created_at: u64,
    #[schemars(with = "super::query_model::QueryIdentifierSchema")]
    pub(crate) after_id: TaskId,
}

// Core enforces the UTF-8 byte bound and absolute-path constraints at decoding.
// JSON Schema maxLength is a character bound, so the runtime remains authoritative.
fn native_path_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({
        "type": "string", "minLength": 1,
        "maxLength": graphit_core::native_path::MAX_PATH_BYTES,
        "pattern": "^[^\\u0000]+$"
    })
}

#[cfg(test)]
mod catalogue_schema_tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn catalogue_cursor_schema_tracks_keyset_wire() {
        let schema = schemars::generate::SchemaSettings::draft2020_12()
            .with(|settings| settings.inline_subschemas = true)
            .into_generator()
            .into_root_schema_for::<CatalogueCursor>();
        let schema = serde_json::to_value(schema).unwrap();
        let properties = schema["properties"].as_object().unwrap();
        assert_eq!(schema["additionalProperties"], false);
        assert!(!properties.contains_key("offset"));
        for status in [
            TaskStatus::Open,
            TaskStatus::InProgress,
            TaskStatus::Completed,
            TaskStatus::Cancelled,
        ] {
            for branched in [false, true] {
                let cursor = CatalogueCursor {
                    corpus_key: "test-corpus".into(),
                    project_id: "00000000000000000000000001".parse().unwrap(),
                    brand: "graphit".into(),
                    root: graphit_core::native_path::NativePath::new(
                        std::env::current_dir().unwrap(),
                    )
                    .unwrap(),
                    project_version: "main".into(),
                    version: 23,
                    filter: CatalogueFilter {
                        query: Some("compiler".into()),
                        status: Some(status),
                        session_id: Some(SessionId::new(NonZeroU128::new(2).unwrap())),
                        flagged: Some(true),
                        blocked: Some(false),
                    },
                    physical_branch: branched.then(|| "branch-23".into()),
                    inherited_target: branched.then(|| "main".into()),
                    after_created_at: 1234,
                    after_id: TaskId::new(NonZeroU128::new(3).unwrap()),
                };
                let wire = serde_json::to_value(&cursor).unwrap();
                assert_eq!(
                    wire.as_object().unwrap().keys().collect::<Vec<_>>(),
                    properties.keys().collect::<Vec<_>>()
                );
                assert_eq!(
                    serde_json::from_value::<CatalogueCursor>(wire.clone()).unwrap(),
                    cursor
                );
                assert!(
                    properties["filter"]["properties"]["status"]["enum"]
                        .as_array()
                        .unwrap()
                        .contains(&wire["filter"]["status"])
                );
                for field in ["project_id", "after_id"] {
                    assert_eq!(
                        properties[field]["not"]["const"],
                        "00000000000000000000000000"
                    );
                }
                let mut legacy = wire;
                legacy["offset"] = json!(1);
                assert!(serde_json::from_value::<CatalogueCursor>(legacy).is_err());
            }
        }
        let required = schema["required"].as_array().unwrap();
        for field in [
            "project_id",
            "brand",
            "root",
            "project_version",
            "version",
            "filter",
            "after_created_at",
            "after_id",
        ] {
            assert!(required.contains(&Value::String(field.into())));
        }
        assert!(schema["properties"]["filter"]["required"].is_null());
        assert_eq!(
            serde_json::from_value::<CatalogueFilter>(json!({})).unwrap(),
            CatalogueFilter::default()
        );
    }

    #[test]
    fn catalogue_cursor_native_root_schema_is_a_bounded_unicode_string() {
        let schema = native_path_schema(&mut SchemaGenerator::default());
        let schema = serde_json::to_value(schema).unwrap();
        assert_eq!(schema["type"], "string");
        assert_eq!(schema["minLength"], 1);
        assert_eq!(
            schema["maxLength"],
            graphit_core::native_path::MAX_PATH_BYTES
        );
        assert!(schema["properties"].is_null());
        let path = if cfg!(windows) {
            "C:\\Projetos\\ação 日本語"
        } else {
            "/projetos/ação 日本語"
        };
        let root = graphit_core::native_path::NativePath::new(path.into()).unwrap();
        assert_eq!(serde_json::to_value(&root).unwrap(), json!(path));
        assert_eq!(
            serde_json::from_value::<graphit_core::native_path::NativePath>(json!(path)).unwrap(),
            root
        );
        assert!(
            serde_json::from_value::<graphit_core::native_path::NativePath>(
                json!({"os":"Unix","units":[47]})
            )
            .is_err()
        );
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CataloguePage {
    pub results: Vec<TaskSummary>,
    pub next_cursor: Option<CatalogueCursor>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskGeneration {
    pub version: u64,
    pub physical_branch: Option<String>,
    pub inherited_target: Option<String>,
}
/// A real strict SDK index publication and its recovery identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskLexicalIndexPublication {
    pub generation: TaskGeneration,
    pub recovery_key: graphit_data::lance_branch_store::DeltaRecoveryKey,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskLexicalIndexCreation {
    NoCorpus,
    Published(TaskLexicalIndexPublication),
}
/// A complete lightweight observation from one SDK pin; cancellation returns an error.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TaskReadView {
    NoCorpus,
    Published {
        generation: TaskGeneration,
        tasks: Vec<TaskSummary>,
        sessions: Vec<SessionHead>,
    },
}

/// Persisted ownership observations. Readers never expire or repair them.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ownership {
    pub owner: Option<String>,
    pub claim_epoch: u64,
    pub claimed_at: Option<u64>,
    pub heartbeat_at: Option<u64>,
    pub lease_expires_at: Option<u64>,
}
impl Ownership {
    pub(super) fn validate(&self) -> Result<(), super::TaskStoreError> {
        match (
            &self.owner,
            self.claimed_at,
            self.heartbeat_at,
            self.lease_expires_at,
        ) {
            (None, None, None, None) => Ok(()),
            (Some(owner), Some(claimed), Some(heartbeat), Some(expires))
                if crate::actor::valid_identifier(owner)
                    && self.claim_epoch > 0
                    && claimed <= heartbeat
                    && heartbeat < expires =>
            {
                Ok(())
            }
            _ => Err(super::TaskStoreError::Invalid("ownership observation")),
        }
    }
    pub(super) fn live(&self, at: u64) -> bool {
        self.owner.is_some() && self.lease_expires_at.is_some_and(|end| end > at)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressKind {
    Progress,
    Checkpoint,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressObservation {
    pub kind: ProgressKind,
    pub sequence: u64,
    pub observed_at: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    pub observation: ProgressObservation,
    pub summary: String,
    pub next_step: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum LifecycleKind {
    CommentAdded {
        id: TaskCommentId,
        sequence: std::num::NonZeroU64,
    },
    Claimed,
    Renewed,
    Progress,
    Released,
    Revised {
        reason: String,
    },
    CheckVerified {
        check_id: CheckId,
    },
    CheckSuperseded {
        check_id: CheckId,
        replacement: Option<CheckId>,
        reason: String,
    },
    LinksUpdated {
        reason: String,
    },
    FlagUpdated {
        flagged: bool,
        reason: String,
    },
    CommitUpdated {
        reason: String,
    },
    Completed,
    Cancelled {
        reason: String,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleEvent<T> {
    pub actor: String,
    pub kind: LifecycleKind,
    pub observed_at: u64,
    pub record: T,
    pub references_update: TaskReferenceUpdate,
}
/// Opaque participant credential, deliberately without Debug or Serde.
#[derive(Clone, Eq, PartialEq)]
pub struct ClaimToken(pub(super) [u8; 32]);
impl ClaimToken {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
pub struct Claim<T> {
    pub record: T,
    pub token: ClaimToken,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaskActivityHead {
    pub id: TaskId,
    pub session_id: SessionId,
    pub title: String,
    pub status: TaskStatus,
    pub ownership: Ownership,
    pub progress: Option<ProgressObservation>,
    pub revision: u64,
    pub created_at: u64,
    pub updated_at: u64,
}
/// All current heads from one pin. Local section failures keep their causes.
#[derive(Debug)]
pub enum TaskHeadObservation {
    NoCorpus,
    Published {
        generation: TaskGeneration,
        tasks: Result<Vec<TaskActivityHead>, Box<super::TaskStoreError>>,
        sessions: Result<Vec<SessionHead>, Box<super::TaskStoreError>>,
    },
}

/// Fence and revision required for a live owner mutation.
pub struct ClaimUpdate<'a> {
    pub token: &'a ClaimToken,
    pub expected_revision: u64,
}
pub struct TaskCommentUpdate<'a> {
    pub claim: ClaimUpdate<'a>,
    pub lease: std::time::Duration,
    pub idempotency_key: String,
    pub kind: TaskCommentKind,
    pub body: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "result",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum TaskCommentOutcome {
    Created { task: Task, comment: TaskComment },
    Existing { task: Task, comment: TaskComment },
}
pub struct ProgressUpdate<'a> {
    pub claim: ClaimUpdate<'a>,
    pub lease: std::time::Duration,
    pub summary: String,
    pub next_step: String,
}
pub struct ReleaseUpdate<'a> {
    pub claim: ClaimUpdate<'a>,
    pub summary: String,
    pub next_step: String,
}

/// Specification changes for a live owner; new check IDs are generated by the store.
pub struct RevisionUpdate<'a> {
    pub claim: ClaimUpdate<'a>,
    pub lease: std::time::Duration,
    pub reason: &'a str,
    pub title: FieldUpdate<&'a str>,
    pub description: FieldUpdate<&'a str>,
    pub task_type: FieldUpdate<&'a str>,
    pub priority: Option<u8>,
    pub schedule: FieldUpdate<Option<TaskSchedule>>,
    pub add_checks: &'a [CreateCheck],
}
/// Partial specification revision by the current Session coordinator.
pub struct SessionRevisionUpdate<'a> {
    pub claim: ClaimUpdate<'a>,
    pub lease: std::time::Duration,
    pub reason: &'a str,
    pub title: FieldUpdate<&'a str>,
    pub description: FieldUpdate<&'a str>,
    pub strategy: FieldUpdate<&'a str>,
}
pub struct CheckUpdate<'a> {
    pub claim: ClaimUpdate<'a>,
    pub lease: std::time::Duration,
    pub check_id: CheckId,
    pub passed: bool,
    pub evidence: &'a str,
}
pub struct ReplacementInput<'a> {
    pub text: &'a str,
    pub kind: Option<CheckKind>,
}
pub struct SupersessionUpdate<'a> {
    pub claim: ClaimUpdate<'a>,
    pub lease: std::time::Duration,
    pub check_id: CheckId,
    pub reason: &'a str,
    pub replacement: Option<ReplacementInput<'a>>,
}
pub struct TaskLinksUpdate<'a> {
    pub claim: ClaimUpdate<'a>,
    pub lease: std::time::Duration,
    pub parent_id: FieldUpdate<Option<TaskId>>,
    pub depends_on: Option<&'a [TaskId]>,
    pub reason: &'a str,
}
pub struct TaskFlagUpdate<'a> {
    pub claim: ClaimUpdate<'a>,
    pub lease: std::time::Duration,
    pub flagged: bool,
    pub reason: &'a str,
}
/// Open cancellation requires broad Task admission; owned tasks require the live fence.
pub struct TaskCancellation<'a> {
    pub claim: Option<ClaimUpdate<'a>>,
    pub reason: &'a str,
}

/// Commit metadata can be updated after completion without reopening the Task.
pub struct TaskCommitUpdate<'a> {
    pub expected_revision: u64,
    pub claim: Option<ClaimUpdate<'a>>,
    pub commit: Option<&'a str>,
    pub reason: &'a str,
}
