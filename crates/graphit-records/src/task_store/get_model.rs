//! Current Task state and explicitly selected, generation-bound detail pages.
use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
use std::num::NonZeroUsize;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskGetInput {
    #[schemars(with = "super::query_model::QueryIdentifierSchema")]
    pub id: crate::task_spec::TaskId,
    pub history: Option<TaskDetailPageInput>,
    pub comments: Option<TaskDetailPageInput>,
}
impl TaskGetInput {
    pub fn current(id: crate::task_spec::TaskId) -> Self {
        Self {
            id,
            history: None,
            comments: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskDetailPageInput {
    pub page_size: NonZeroUsize,
    pub cursor: Option<TaskDetailCursor>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DetailBinding {
    pub corpus_key: String,
    pub root: graphit_core::native_path::NativePath,
    pub project_id: graphit_core::project_identity::ProjectId,
    pub brand: String,
    pub project_version: String,
    pub generation: TaskGeneration,
    pub branch_provenance: Option<graphit_data::lance_branch_store::BranchProvenance>,
    pub task_id: crate::task_spec::TaskId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DetailSection {
    History,
    Comments,
}
/// Continuation selects an admitted historical pin; it never supplies authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskDetailCursor {
    pub(super) binding: Box<DetailBinding>,
    pub(super) section: DetailSection,
    pub(super) after_revision: u64,
    pub(super) after_sequence: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorFields {
    binding: Box<DetailBinding>,
    section: DetailSection,
    after_revision: u64,
    after_sequence: u64,
}
impl Serialize for TaskDetailCursor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let fields = CursorFields {
            binding: self.binding.clone(),
            section: self.section,
            after_revision: self.after_revision,
            after_sequence: self.after_sequence,
        };
        serializer.serialize_str(
            &URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(&fields).map_err(serde::ser::Error::custom)?),
        )
    }
}
impl<'de> Deserialize<'de> for TaskDetailCursor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let bytes = URL_SAFE_NO_PAD.decode(&text).map_err(D::Error::custom)?;
        if URL_SAFE_NO_PAD.encode(&bytes) != text {
            return Err(D::Error::custom("noncanonical Task detail cursor"));
        }
        let fields: CursorFields = serde_json::from_slice(&bytes).map_err(D::Error::custom)?;
        Ok(Self {
            binding: fields.binding,
            section: fields.section,
            after_revision: fields.after_revision,
            after_sequence: fields.after_sequence,
        })
    }
}
impl JsonSchema for TaskDetailCursor {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TaskDetailCursor".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({"type":"string","pattern":"^[A-Za-z0-9_-]+$","description":"Opaque Task/section/publication-bound continuation; grants no authority."})
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TaskHistoryPage {
    pub before: Task,
    pub events: Vec<LifecycleEvent<Task>>,
    pub next_cursor: Option<TaskDetailCursor>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TaskCommentsPage {
    pub comments: Vec<crate::task_comment::TaskComment>,
    pub next_cursor: Option<TaskDetailCursor>,
}
/// Null sections were not requested; a present empty page is a completed read.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TaskGetRead {
    pub task: Task,
    pub creation: Creation<Task>,
    pub last_event: Option<LifecycleEvent<Task>>,
    pub generation: TaskGeneration,
    pub history_start_cursor: TaskDetailCursor,
    pub comments_start_cursor: TaskDetailCursor,
    pub history: Option<TaskHistoryPage>,
    pub comments: Option<TaskCommentsPage>,
}
