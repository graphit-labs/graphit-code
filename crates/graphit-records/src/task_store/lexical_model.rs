//! Native Task lexical discovery and closed continuation data.
use super::{SessionId, TaskCommentId, TaskGeneration, TaskSummary};
use crate::gates::TaskStatus;
use graphit_core::{native_path::NativePath, project_identity::ProjectId};
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSearchFilter {
    pub status: Option<TaskStatus>,
    pub session_id: Option<SessionId>,
    pub flagged: Option<bool>,
    pub blocked: Option<bool>,
}
impl TaskSearchFilter {
    pub(super) fn matches(&self, task: &TaskSummary) -> bool {
        self.status.is_none_or(|v| v == task.status)
            && self.session_id.is_none_or(|v| v == task.session_id)
            && self.flagged.is_none_or(|v| v == task.flagged)
            && self
                .blocked
                .is_none_or(|v| v == (task.status == TaskStatus::Open && !task.ready))
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskSearchMode {
    #[default]
    Fts,
    Semantic,
    Hybrid,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskLexicalQuery {
    #[serde(default)]
    pub mode: TaskSearchMode,
    pub text: String,
    pub filter: TaskSearchFilter,
    pub page_size: NonZeroUsize,
    pub top_k: NonZeroUsize,
    pub cursor: Option<TaskSearchCursor>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SearchBinding {
    pub mode: TaskSearchMode,
    pub semantic: Option<super::embeddings::SemanticBinding>,
    pub corpus_key: String,
    pub project_id: ProjectId,
    pub root: NativePath,
    pub project_version: String,
    #[serde(with = "closed_generation")]
    pub generation: TaskGeneration,
    pub branch_provenance: Option<graphit_data::lance_branch_store::BranchProvenance>,
    pub text: String,
    pub filter: TaskSearchFilter,
    pub page_size: NonZeroUsize,
    pub top_k: NonZeroUsize,
}
/// Opaque continuation data, bound to one query and generation; it grants no authority.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSearchCursor {
    pub(super) binding: Box<SearchBinding>,
    pub(super) offset: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum TaskLexicalMatchLocation {
    Specification {
        revision: u64,
    },
    Comment {
        comment_id: TaskCommentId,
        sequence: u64,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskLexicalHit {
    pub task: TaskSummary,
    pub score: f32,
    pub location: TaskLexicalMatchLocation,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum TaskLexicalSearchRead {
    NoCorpus,
    PublishedEmpty {
        generation: TaskGeneration,
    },
    Published {
        generation: TaskGeneration,
        total_hits: usize,
        results: Vec<TaskLexicalHit>,
        next_cursor: Option<TaskSearchCursor>,
    },
}

// The shared observation DTO remains unchanged; only continuation data is closed.
mod closed_generation {
    use super::TaskGeneration;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        version: u64,
        physical_branch: Option<String>,
        inherited_target: Option<String>,
    }
    pub(super) fn serialize<S: Serializer>(
        value: &TaskGeneration,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.serialize(serializer)
    }
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<TaskGeneration, D::Error> {
        let value = Fields::deserialize(deserializer)?;
        Ok(TaskGeneration {
            version: value.version,
            physical_branch: value.physical_branch,
            inherited_target: value.inherited_target,
        })
    }
}
