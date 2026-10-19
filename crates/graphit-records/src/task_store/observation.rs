use super::{
    NativeTaskStore, TaskStoreError, TaskStoreReadLimits, model::*, native, read::identifier, rows,
};
use crate::{gates::TaskStatus, task_spec::TaskId};
use futures::TryStreamExt;
use graphit_auth::ProjectReadAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_data::lance_module_store::StoreError;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
const HEAD_COLUMNS: &[&str] = &[
    "kind",
    "record_id",
    "session_id",
    "title",
    "status",
    "owner",
    "claim_epoch",
    "claimed_at",
    "heartbeat_at",
    "lease_expires_at",
    "progress",
    "revision",
    "created_at",
    "updated_at",
];
fn timestamps(
    batch: &arrow_array::RecordBatch,
    row: usize,
) -> Result<(u64, u64, u64), TaskStoreError> {
    let revision = rows::number(batch, "revision", row)?;
    let created = rows::number(batch, "created_at", row)?;
    let updated = rows::number(batch, "updated_at", row)?;
    if revision == 0 || updated < created {
        return Err(TaskStoreError::Invalid("head timestamps"));
    }
    Ok((revision, created, updated))
}
fn task_head(
    batch: &arrow_array::RecordBatch,
    row: usize,
    id: TaskId,
    session_id: SessionId,
) -> Result<TaskActivityHead, TaskStoreError> {
    let title = rows::required(batch, "title", row)?;
    super::canonical(title)?;
    let (revision, created_at, updated_at) = timestamps(batch, row)?;
    Ok(TaskActivityHead {
        id,
        session_id,
        title: title.into(),
        status: identifier(rows::required(batch, "status", row)?)?,
        ownership: rows::ownership(batch, row)?,
        progress: progress_at(batch, row, ProgressKind::Progress, created_at, updated_at)?,
        revision,
        created_at,
        updated_at,
    })
}
fn session_head(
    batch: &arrow_array::RecordBatch,
    row: usize,
    id: SessionId,
) -> Result<SessionHead, TaskStoreError> {
    let title = rows::required(batch, "title", row)?;
    super::canonical(title)?;
    let (revision, created_at, updated_at) = timestamps(batch, row)?;
    Ok(SessionHead {
        id,
        title: title.into(),
        status: identifier(rows::required(batch, "status", row)?)?,
        ownership: rows::ownership(batch, row)?,
        progress: progress_at(batch, row, ProgressKind::Checkpoint, created_at, updated_at)?,
        revision,
        completed_tasks: 0,
        total_tasks: 0,
        updated_at,
    })
}
impl NativeTaskStore {
    /// Observe all current activity headers at one generation, without payload/history.
    /// Stream/authority/identity/linkage failures are global. Row-local presentation
    /// failures remain section errors after EOF and a final original-request recheck.
    pub async fn observe_heads(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        limits: TaskStoreReadLimits,
    ) -> Result<TaskHeadObservation, TaskStoreError> {
        let Some(capture) = native::read_pin(original, admission, limits).await? else {
            return Ok(TaskHeadObservation::NoCorpus);
        };
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, TaskStoreError>(())
        };
        before()?;
        let mut scan = capture.pin.scan();
        scan.filter("kind IN ('task','session')")
            .map_err(StoreError::from)?
            .project(HEAD_COLUMNS)
            .map_err(StoreError::from)?
            .batch_size(limits.batch_rows.get() as usize);
        let mut stream = crate::storage::read(original, scan.try_into_stream())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let (mut tasks, mut sessions) = (Vec::new(), Vec::new());
        let (mut task_error, mut session_error) = (None, None);
        let mut ids = HashSet::new();
        let mut session_ids = HashSet::new();
        let mut task_sessions = Vec::new();
        loop {
            before()?;
            let batch = crate::storage::read(original, stream.try_next())
                .await?
                .map_err(StoreError::from)?;
            before()?;
            let Some(batch) = batch else { break };
            for row in 0..batch.num_rows() {
                original.check()?;
                let kind = rows::required(&batch, "kind", row)?;
                let raw_id = rows::required(&batch, "record_id", row)?;
                match kind {
                    "task" => {
                        let id: TaskId = identifier(raw_id)?;
                        let session_id: SessionId =
                            identifier(rows::required(&batch, "session_id", row)?)?;
                        if !ids.insert(id.get()) {
                            return Err(TaskStoreError::Invalid("duplicate current identity"));
                        }
                        task_sessions.push(session_id);
                        match task_head(&batch, row, id, session_id) {
                            Ok(t) => tasks.push(t),
                            Err(e) => {
                                task_error.get_or_insert(e);
                            }
                        }
                    }
                    "session" => {
                        let id: SessionId = identifier(raw_id)?;
                        let linked: SessionId =
                            identifier(rows::required(&batch, "session_id", row)?)?;
                        if linked != id || !ids.insert(id.get()) || !session_ids.insert(id) {
                            return Err(TaskStoreError::Invalid("session identity/linkage"));
                        }
                        match session_head(&batch, row, id) {
                            Ok(s) => sessions.push(s),
                            Err(e) => {
                                session_error.get_or_insert(e);
                            }
                        }
                    }
                    _ => return Err(TaskStoreError::Invalid("current kind")),
                }
            }
        }
        before()?;
        if task_sessions.iter().any(|s| !session_ids.contains(s)) {
            return Err(TaskStoreError::MissingSession);
        }
        let tasks = match task_error {
            Some(e) => Err(Box::new(e)),
            None => Ok(tasks),
        };
        let sessions = match (&tasks, session_error) {
            (Err(_), _) => Err(Box::new(TaskStoreError::CountsUnavailable)),
            (Ok(_), Some(e)) => Err(Box::new(e)),
            (Ok(valid_tasks), None) => {
                let mut counts: HashMap<SessionId, (usize, usize)> = HashMap::new();
                for task in valid_tasks {
                    if task.status == TaskStatus::Cancelled {
                        continue;
                    }
                    let count = counts.entry(task.session_id).or_default();
                    count.0 += 1;
                    count.1 += usize::from(task.status == TaskStatus::Completed);
                }
                for session in &mut sessions {
                    let (total, completed) = counts.get(&session.id).copied().unwrap_or_default();
                    session.total_tasks = total;
                    session.completed_tasks = completed;
                }
                Ok(sessions)
            }
        };
        before()?;
        Ok(TaskHeadObservation::Published {
            generation: TaskGeneration {
                version: capture.pin.version(),
                physical_branch: capture.pin.physical_branch().map(str::to_owned),
                inherited_target: capture.pin.inherited_target().map(str::to_owned),
            },
            tasks,
            sessions,
        })
    }
}

fn progress_at(
    batch: &arrow_array::RecordBatch,
    row: usize,
    kind: ProgressKind,
    created: u64,
    updated: u64,
) -> Result<Option<ProgressObservation>, TaskStoreError> {
    let p = rows::progress(batch, row)?;
    if p.as_ref()
        .is_some_and(|p| p.kind != kind || p.observed_at < created || p.observed_at > updated)
    {
        return Err(TaskStoreError::Invalid("progress observation"));
    }
    Ok(p)
}
