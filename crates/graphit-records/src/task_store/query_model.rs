//! Finite public Task queries; continuation data never grants authority.
use super::{SessionId, TaskGeneration};
use crate::{gates::TaskStatus, task_spec::TaskId};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
use std::num::NonZeroUsize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "table", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskQuerySelection {
    Tasks {
        #[schemars(with = "Option<Vec<QueryIdentifierSchema>>")]
        ids: Option<Vec<TaskId>>,
        #[schemars(schema_with = "optional_status_schema")]
        status: Option<TaskStatus>,
        #[schemars(with = "Option<QueryIdentifierSchema>")]
        session_id: Option<SessionId>,
    },
    Sessions {
        #[schemars(with = "Option<Vec<QueryIdentifierSchema>>")]
        ids: Option<Vec<SessionId>>,
        #[schemars(schema_with = "optional_status_schema")]
        status: Option<TaskStatus>,
    },
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskQueryColumn {
    Id,
    Status,
    Title,
    Revision,
    CreatedAt,
    UpdatedAt,
    SessionId,
    ParentId,
    TaskType,
    Priority,
    Flagged,
}
impl TaskQueryColumn {
    pub(super) fn physical(self) -> &'static str {
        match self {
            Self::Id => "record_id",
            Self::Status => "status",
            Self::Title => "title",
            Self::Revision => "revision",
            Self::CreatedAt => "created_at",
            Self::UpdatedAt => "updated_at",
            Self::SessionId => "session_id",
            Self::ParentId => "parent_id",
            Self::TaskType => "task_type",
            Self::Priority => "priority",
            Self::Flagged => "flagged",
        }
    }
    pub(super) fn task_only(self) -> bool {
        matches!(
            self,
            Self::SessionId | Self::ParentId | Self::TaskType | Self::Priority | Self::Flagged
        )
    }
    pub(super) fn data_type(self) -> TaskQueryType {
        match self {
            Self::Id | Self::SessionId | Self::ParentId => TaskQueryType::Ulid,
            Self::Status => TaskQueryType::TaskStatus,
            Self::Revision | Self::CreatedAt | Self::UpdatedAt | Self::Priority => {
                TaskQueryType::UnsignedInteger
            }
            Self::Flagged => TaskQueryType::Boolean,
            _ => TaskQueryType::String,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskQuery {
    pub selection: TaskQuerySelection,
    #[schemars(schema_with = "columns_schema")]
    pub columns: Vec<TaskQueryColumn>,
    pub page_size: NonZeroUsize,
    pub top_k: NonZeroUsize,
    pub cursor: Option<TaskQueryCursor>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct QueryBinding {
    pub corpus_key: String,
    pub project_id: graphit_core::project_identity::ProjectId,
    pub root: graphit_core::native_path::NativePath,
    pub brand: String,
    pub project_version: String,
    pub version: u64,
    pub physical_branch: Option<String>,
    pub inherited_target: Option<String>,
    pub selection: TaskQuerySelection,
    pub columns: Vec<TaskQueryColumn>,
    pub page_size: NonZeroUsize,
    pub top_k: NonZeroUsize,
}
/// Opaque query continuation; deserialization alone does not validate its scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskQueryCursor {
    pub(super) binding: Box<QueryBinding>,
    pub(super) after: TaskId,
    pub(super) consumed: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorFields {
    binding: Box<QueryBinding>,
    after: TaskId,
    consumed: usize,
}
impl Serialize for TaskQueryCursor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value = CursorFields {
            binding: self.binding.clone(),
            after: self.after,
            consumed: self.consumed,
        };
        let bytes = serde_json::to_vec(&value).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&URL_SAFE_NO_PAD.encode(bytes))
    }
}
impl<'de> Deserialize<'de> for TaskQueryCursor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let bytes = URL_SAFE_NO_PAD.decode(&text).map_err(D::Error::custom)?;
        if URL_SAFE_NO_PAD.encode(&bytes) != text {
            return Err(D::Error::custom("noncanonical Task query cursor"));
        }
        let fields: CursorFields = serde_json::from_slice(&bytes).map_err(D::Error::custom)?;
        Ok(Self {
            binding: fields.binding,
            after: fields.after,
            consumed: fields.consumed,
        })
    }
}
impl JsonSchema for TaskQueryCursor {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TaskQueryCursor".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({"type":"string", "pattern":"^[A-Za-z0-9_-]+$", "description":"Opaque continuation returned by this exact query; grants no authority."})
    }
}
pub(super) struct QueryIdentifierSchema;
impl JsonSchema for QueryIdentifierSchema {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TaskQueryIdentifier".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({"type":"string", "pattern":"^[0-7][0-9A-HJKMNP-TV-Z]{25}$", "not":{"const":"00000000000000000000000000"}})
    }
}
pub(super) fn optional_status_schema(_: &mut SchemaGenerator) -> Schema {
    let values: Vec<_> = [
        TaskStatus::Open,
        TaskStatus::InProgress,
        TaskStatus::Completed,
        TaskStatus::Cancelled,
    ]
    .into_iter()
    .map(|status| serde_json::to_value(status).expect("TaskStatus serialization is scalar"))
    .chain(std::iter::once(serde_json::Value::Null))
    .collect();
    schemars::json_schema!({"enum": values})
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskQueryRead {
    NoCorpus,
    Published {
        generation: TaskGeneration,
        columns: Vec<TaskQueryColumn>,
        rows: Vec<Vec<serde_json::Value>>,
        next_cursor: Option<TaskQueryCursor>,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskQueryType {
    Ulid,
    TaskStatus,
    String,
    UnsignedInteger,
    Boolean,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskQueryColumnSchema {
    pub name: TaskQueryColumn,
    pub data_type: TaskQueryType,
    pub nullable: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskQueryTable {
    Tasks,
    Sessions,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskQueryTableSchema {
    pub table: TaskQueryTable,
    pub columns: Vec<TaskQueryColumnSchema>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskQuerySchema {
    pub tables: Vec<TaskQueryTableSchema>,
}

fn columns_schema(generator: &mut SchemaGenerator) -> Schema {
    let item = generator.subschema_for::<TaskQueryColumn>();
    schemars::json_schema!({"type":"array", "items":item, "minItems":1, "uniqueItems":true})
}
