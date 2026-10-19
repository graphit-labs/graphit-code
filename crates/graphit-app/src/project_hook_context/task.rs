//! Explicit Graphit Task selection; it supplies data, never authority.
use super::*;
use graphit_records::{
    task_spec::TaskId,
    task_store::{
        NativeTaskStore, SelectedSessionRead, SessionGetInput, SessionId, TaskGetInput,
        TaskGetRead, TaskStoreReadLimits,
    },
};
use serde::Deserialize;
use std::collections::HashSet;

/// Caller-selected Graphit identifiers, unrelated to a host or MCP session.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskContextSelection {
    pub session_id: SessionId,
    #[serde(default)]
    pub task_ids: Vec<TaskId>,
}
impl TaskContextSelection {
    pub fn validate(&self, original: &RequestContext<'_>) -> Result<(), HookContextError> {
        original.remaining().map_err(HookContextError::Request)?;
        if self.task_ids.len() > original.content_limits().max_collection_entries {
            return Err(HookContextError::InvalidTaskSelection);
        }
        let mut seen = HashSet::new();
        seen.try_reserve(self.task_ids.len())
            .map_err(|_| HookContextError::Instruction(InstructionError::Allocation))?;
        for id in &self.task_ids {
            original
                .snapshot()
                .check()
                .map_err(HookContextError::Request)?;
            if !seen.insert(*id) {
                return Err(HookContextError::InvalidTaskSelection);
            }
        }
        Ok(())
    }
}

#[derive(Serialize)]
pub(super) struct TaskContext {
    session: SelectedSessionRead,
    tasks: Vec<TaskGetRead>,
}
pub(super) async fn read(
    original: &RequestContext<'_>,
    selection: &TaskContextSelection,
    admission: &ProjectReadAdmission<'_>,
    limits: TaskStoreReadLimits,
) -> Result<TaskContext, HookContextError> {
    let session = NativeTaskStore::read_session(
        original.snapshot(),
        admission,
        SessionGetInput::current(selection.session_id),
        limits,
    )
    .await
    .map_err(|e| HookContextError::Task(Box::new(e)))?
    .ok_or(HookContextError::MissingTaskSession)?;
    let mut tasks = Vec::new();
    tasks
        .try_reserve_exact(selection.task_ids.len())
        .map_err(|_| HookContextError::Instruction(InstructionError::Allocation))?;
    for id in &selection.task_ids {
        let detail = NativeTaskStore::read_task(
            original.snapshot(),
            admission,
            TaskGetInput::current(*id),
            limits,
        )
        .await
        .map_err(|e| HookContextError::Task(Box::new(e)))?
        .ok_or(HookContextError::MissingTask)?;
        if detail.task.session_id != selection.session_id {
            return Err(HookContextError::TaskSessionMismatch);
        }
        tasks.push(detail);
    }
    Ok(TaskContext { session, tasks })
}
