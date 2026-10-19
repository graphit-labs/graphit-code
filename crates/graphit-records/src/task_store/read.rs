use super::{
    NativeTaskStore, TaskStoreError, TaskStoreReadLimits,
    model::{self, *},
    native, rows,
};
use crate::{
    gates,
    task_schedule::TaskSchedule,
    task_spec::{CheckKind, CheckStatus, TaskId},
};
use futures::TryStreamExt;
use graphit_auth::ProjectReadAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_data::{lance_branch_store::BranchSnapshot, lance_module_store::StoreError};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub(super) async fn exact_rows(
    pin: &BranchSnapshot,
    filter: &str,
    original: &Arc<SnapshotContext>,
    limits: TaskStoreReadLimits,
    before: impl Fn() -> Result<(), TaskStoreError>,
) -> Result<Vec<rows::Row>, TaskStoreError> {
    before()?;
    let mut scan = pin.scan();
    scan.filter(filter)
        .map_err(StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize);
    let mut stream = crate::storage::read(original, scan.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    before()?;
    let mut output = Vec::new();
    loop {
        before()?;
        let next = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = next else {
            break;
        };
        for row in 0..batch.num_rows() {
            original.check()?;
            output.push(rows::decode(&batch, row, original)?);
        }
    }
    before()?;
    Ok(output)
}

pub(super) struct Projected {
    pub(super) summary: TaskSummary,
    pub(super) checks: Vec<rows::CheckProjection>,
}
pub(super) fn identifier<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, TaskStoreError> {
    Ok(T::deserialize(
        serde::de::value::BorrowedStrDeserializer::<serde_json::Error>::new(text),
    )?)
}
pub(super) fn project(
    batch: &arrow_array::RecordBatch,
    row: usize,
) -> Result<Projected, TaskStoreError> {
    let text = |name| rows::required(batch, name, row);
    let optional_id = |name| rows::text(batch, name, row)?.map(identifier).transpose();
    let date = |name| {
        rows::text(batch, name, row)?
            .map(|s| {
                s.parse()
                    .map_err(|_| TaskStoreError::Invalid("calendar projection"))
            })
            .transpose()
    };
    let (start, end, due) = (date("start_date")?, date("end_date")?, date("due_date")?);
    let schedule = if start.is_none() && end.is_none() && due.is_none() {
        None
    } else {
        Some(
            TaskSchedule::new(start, end, due)
                .map_err(|_| TaskStoreError::Invalid("calendar projection"))?,
        )
    };
    let summary = TaskSummary {
        id: identifier(text("record_id")?)?,
        session_id: identifier(text("session_id")?)?,
        title: text("title")?.into(),
        task_type: text("task_type")?.into(),
        priority: u8::try_from(rows::number(batch, "priority", row)?)
            .map_err(|_| TaskStoreError::Invalid("priority"))?,
        status: identifier(text("status")?)?,
        parent_id: optional_id("parent_id")?,
        depends_on: serde_json::from_str(text("dependencies")?)?,
        schedule,
        ownership: rows::ownership(batch, row)?,
        progress: rows::progress(batch, row)?,
        flagged: rows::boolean(batch, "flagged", row)?,
        ready: false,
        revision: rows::number(batch, "revision", row)?,
        created_at: rows::number(batch, "created_at", row)?,
        updated_at: rows::number(batch, "updated_at", row)?,
    };
    super::canonical(&summary.title)?;
    super::canonical(&summary.task_type)?;
    if summary.priority > 4 || summary.revision == 0 || summary.updated_at < summary.created_at {
        return Err(TaskStoreError::Invalid("task projection"));
    }
    Ok(Projected {
        summary,
        checks: serde_json::from_str(text("checks")?)?,
    })
}
fn with_graph<T>(
    tasks: &[Projected],
    evaluate: impl FnOnce(&gates::GateGraph<'_>, &[String]) -> Result<T, TaskStoreError>,
) -> Result<T, TaskStoreError> {
    let ids: Vec<_> = tasks.iter().map(|t| id(t.summary.id.get())).collect();
    let sessions: Vec<_> = tasks
        .iter()
        .map(|t| id(t.summary.session_id.get()))
        .collect();
    let parents: Vec<_> = tasks
        .iter()
        .map(|t| t.summary.parent_id.map(|p| id(p.get())))
        .collect();
    let deps: Vec<Vec<_>> = tasks
        .iter()
        .map(|t| t.summary.depends_on.iter().map(|d| id(d.get())).collect())
        .collect();
    let dep_refs: Vec<Vec<_>> = deps
        .iter()
        .map(|d| d.iter().map(String::as_str).collect())
        .collect();
    let checks: Vec<Vec<_>> = tasks
        .iter()
        .map(|t| {
            t.checks
                .iter()
                .map(|c| gates::CheckFact {
                    kind: match c.kind {
                        CheckKind::Acceptance => gates::CheckKind::Acceptance,
                        CheckKind::Test => gates::CheckKind::Test,
                    },
                    status: match c.status {
                        CheckStatus::Pending => gates::CheckStatus::Pending,
                        CheckStatus::Passed => gates::CheckStatus::Passed,
                        CheckStatus::Failed => gates::CheckStatus::Failed,
                        CheckStatus::Superseded => gates::CheckStatus::Superseded,
                    },
                    evidence: c.evidence.as_deref().unwrap_or(""),
                })
                .collect()
        })
        .collect();
    let facts: Vec<_> = tasks
        .iter()
        .enumerate()
        .map(|(i, t)| gates::TaskFact {
            id: &ids[i],
            session_id: &sessions[i],
            parent_id: parents[i].as_deref(),
            status: t.summary.status,
            flagged: t.summary.flagged,
            dependencies: &dep_refs[i],
            checks: &checks[i],
        })
        .collect();
    let graph = gates::GateGraph::new(&facts)?;
    evaluate(&graph, &ids)
}
pub(super) fn ready(tasks: &mut [Projected]) -> Result<(), TaskStoreError> {
    let ready = with_graph(tasks, |graph, ids| {
        ids.iter()
            .map(|id| graph.ready(id).map_err(TaskStoreError::from))
            .collect::<Result<Vec<_>, _>>()
    })?;
    for (task, is_ready) in tasks.iter_mut().zip(ready) {
        task.summary.ready = is_ready;
    }
    Ok(())
}

async fn scan_tasks(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    limits: TaskStoreReadLimits,
    before: impl Fn() -> Result<(), TaskStoreError>,
) -> Result<Vec<Projected>, TaskStoreError> {
    before()?;
    let mut scan = pin.scan();
    scan.filter("kind = 'task'")
        .map_err(StoreError::from)?
        .project(rows::COLUMNS)
        .map_err(StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize);
    let mut stream = crate::storage::read(original, scan.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    before()?;
    let mut tasks = Vec::new();
    loop {
        before()?;
        let next = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = next else { break };
        for row in 0..batch.num_rows() {
            original.check()?;
            tasks.push(project(&batch, row)?);
        }
    }
    before()?;
    Ok(tasks)
}
pub(super) fn project_task(task: &Task) -> Projected {
    Projected {
        summary: TaskSummary {
            id: task.specification.id,
            session_id: task.session_id,
            title: task.specification.title.clone(),
            task_type: task.specification.task_type.clone(),
            priority: task.specification.priority,
            status: task.status,
            parent_id: task.parent_id,
            depends_on: task.depends_on.clone(),
            schedule: task.specification.schedule,
            ownership: task.ownership.clone(),
            progress: task.progress.as_ref().map(|p| p.observation.clone()),
            flagged: task.flagged,
            ready: false,
            revision: task.specification.revision,
            created_at: task.created_at,
            updated_at: task.updated_at,
        },
        checks: task
            .specification
            .checks
            .iter()
            .map(|c| rows::CheckProjection {
                kind: c.kind,
                status: c.status,
                evidence: c.verification.as_ref().map(|v| v.evidence.clone()),
            })
            .collect(),
    }
}
/// Derive graph facts from already validated heads without capturing another pin.
pub(super) fn task_summaries(tasks: &[TaskDetail]) -> Result<Vec<TaskSummary>, TaskStoreError> {
    let mut projected: Vec<_> = tasks.iter().map(|t| project_task(&t.task)).collect();
    ready(&mut projected)?;
    Ok(projected.into_iter().map(|t| t.summary).collect())
}
pub(super) async fn head_summaries(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    limits: TaskStoreReadLimits,
    before: impl Fn() -> Result<(), TaskStoreError>,
) -> Result<Vec<TaskSummary>, TaskStoreError> {
    let mut projected = scan_tasks(pin, original, limits, &before).await?;
    ready(&mut projected)?;
    before()?;
    Ok(projected.into_iter().map(|p| p.summary).collect())
}
pub(super) async fn validate_relations(
    pin: &BranchSnapshot,
    task: &Task,
    original: &Arc<SnapshotContext>,
    limits: TaskStoreReadLimits,
    before: impl Fn() -> Result<(), TaskStoreError>,
) -> Result<(), TaskStoreError> {
    let mut tasks = scan_tasks(pin, original, limits, &before).await?;
    for related in task.depends_on.iter().copied().chain(task.parent_id) {
        let found = tasks
            .iter()
            .find(|t| t.summary.id == related)
            .ok_or(TaskStoreError::MissingTask)?;
        if task.parent_id == Some(related) && found.summary.session_id != task.session_id {
            return Err(TaskStoreError::Invalid("parent belongs to another session"));
        }
        if task.parent_id == Some(related)
            && !matches!(
                found.summary.status,
                gates::TaskStatus::Open | gates::TaskStatus::InProgress
            )
        {
            return Err(TaskStoreError::Invalid("terminal parent"));
        }
    }
    if let Some(existing) = tasks
        .iter_mut()
        .find(|t| t.summary.id == task.specification.id)
    {
        *existing = project_task(task);
    } else {
        tasks.push(project_task(task));
    }
    with_graph(&tasks, |_, _| Ok(()))?;
    before()
}
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub(super) async fn require_completion(
    pin: &BranchSnapshot,
    task: Option<TaskId>,
    session: Option<SessionId>,
    original: &Arc<SnapshotContext>,
    limits: TaskStoreReadLimits,
    before: impl Fn() -> Result<(), TaskStoreError>,
) -> Result<(), TaskStoreError> {
    let tasks = scan_tasks(pin, original, limits, &before).await?;
    let allowed = with_graph(&tasks, |graph, _| match (task, session) {
        (Some(id), None) => Ok(graph.completion(&model::id(id.get()))?),
        (None, Some(id)) => Ok(graph.session_closable(&model::id(id.get()))?),
        _ => Err(TaskStoreError::Invalid("completion target")),
    })?;
    before()?;
    if allowed {
        Ok(())
    } else {
        Err(TaskStoreError::Invalid("completion gates are pending"))
    }
}

impl NativeTaskStore {
    /// Observe every task/session head from one pin, returning only after EOF.
    pub async fn read_view(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        limits: TaskStoreReadLimits,
    ) -> Result<TaskReadView, TaskStoreError> {
        let Some(capture) = native::read_pin(original, admission, limits).await? else {
            return Ok(TaskReadView::NoCorpus);
        };
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, TaskStoreError>(())
        };
        before()?;
        let mut scan = capture.pin.scan();
        let columns: Vec<_> = std::iter::once("kind")
            .chain(rows::COLUMNS.iter().copied())
            .collect();
        scan.filter("kind IN ('task', 'session')")
            .map_err(StoreError::from)?
            .project(&columns)
            .map_err(StoreError::from)?
            .batch_size(limits.batch_rows.get() as usize);
        let mut stream = crate::storage::read(original, scan.try_into_stream())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let mut tasks = Vec::new();
        let mut sessions = Vec::new();
        loop {
            before()?;
            let next = crate::storage::read(original, stream.try_next())
                .await?
                .map_err(StoreError::from)?;
            before()?;
            let Some(batch) = next else {
                break;
            };
            for row in 0..batch.num_rows() {
                original.check()?;
                if rows::required(&batch, "kind", row)? == "task" {
                    tasks.push(project(&batch, row)?);
                } else {
                    sessions.push(SessionHead {
                        id: identifier(rows::required(&batch, "record_id", row)?)?,
                        title: rows::required(&batch, "title", row)?.into(),
                        status: identifier(rows::required(&batch, "status", row)?)?,
                        revision: rows::number(&batch, "revision", row)?,
                        ownership: rows::ownership(&batch, row)?,
                        progress: rows::progress(&batch, row)?,
                        completed_tasks: 0,
                        total_tasks: 0,
                        updated_at: rows::number(&batch, "updated_at", row)?,
                    });
                }
            }
        }
        before()?;
        ready(&mut tasks)?;
        let mut session_ids = HashSet::new();
        for s in &sessions {
            super::canonical(&s.title)?;
            if s.revision == 0 || !session_ids.insert(s.id) {
                return Err(TaskStoreError::Invalid("session projection"));
            }
        }
        for task in &tasks {
            if !session_ids.contains(&task.summary.session_id) {
                return Err(TaskStoreError::MissingSession);
            }
        }
        let mut counts: HashMap<SessionId, (usize, usize)> = HashMap::new();
        for task in &tasks {
            if task.summary.status == gates::TaskStatus::Cancelled {
                continue;
            }
            let count = counts.entry(task.summary.session_id).or_default();
            count.0 += 1;
            count.1 += usize::from(task.summary.status == gates::TaskStatus::Completed);
        }
        for session in &mut sessions {
            let (total, completed) = counts.get(&session.id).copied().unwrap_or_default();
            session.total_tasks = total;
            session.completed_tasks = completed;
        }
        tasks.sort_unstable_by(|a, b| {
            b.summary
                .created_at
                .cmp(&a.summary.created_at)
                .then_with(|| b.summary.id.get().cmp(&a.summary.id.get()))
        });
        sessions.sort_unstable_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| b.id.get().cmp(&a.id.get()))
        });
        before()?;
        Ok(TaskReadView::Published {
            generation: TaskGeneration {
                version: capture.pin.version(),
                physical_branch: capture.pin.physical_branch().map(str::to_owned),
                inherited_target: capture.pin.inherited_target().map(str::to_owned),
            },
            tasks: tasks.into_iter().map(|t| t.summary).collect(),
            sessions,
        })
    }
    pub async fn session_heads(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        limits: TaskStoreReadLimits,
    ) -> Result<Vec<SessionHead>, TaskStoreError> {
        match Self::read_view(original, admission, limits).await? {
            TaskReadView::NoCorpus => Ok(vec![]),
            TaskReadView::Published { sessions, .. } => Ok(sessions),
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub(super) async fn require_ready(
    pin: &BranchSnapshot,
    id: TaskId,
    original: &Arc<SnapshotContext>,
    limits: TaskStoreReadLimits,
    before: impl Fn() -> Result<(), TaskStoreError>,
) -> Result<(), TaskStoreError> {
    before()?;
    let mut scan = pin.scan();
    scan.filter("kind = 'task'")
        .map_err(StoreError::from)?
        .project(rows::COLUMNS)
        .map_err(StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize);
    let mut stream = crate::storage::read(original, scan.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    before()?;
    let mut tasks = Vec::new();
    loop {
        before()?;
        let batch = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = batch else { break };
        for row in 0..batch.num_rows() {
            original.check()?;
            let mut p = project(&batch, row)?;
            if p.summary.id == id {
                p.summary.status = gates::TaskStatus::Open;
            }
            tasks.push(p);
        }
    }
    ready(&mut tasks)?;
    before()?;
    if tasks.iter().any(|t| t.summary.id == id && t.summary.ready) {
        Ok(())
    } else {
        Err(TaskStoreError::Invalid("task is not ready"))
    }
}

pub(super) fn validate_history<T: PartialEq>(
    current: &T,
    creation: &Creation<T>,
    events: &[LifecycleEvent<T>],
    revision: impl Fn(&T) -> u64,
    references: impl Fn(&T) -> &[super::references::TaskReference],
) -> Result<(), TaskStoreError> {
    let expected = revision(current)
        .checked_sub(1)
        .ok_or(TaskStoreError::Invalid("history revision"))?;
    if u64::try_from(events.len()).ok() != Some(expected)
        || events.iter().enumerate().any(|(i, e)| {
            u64::try_from(i).ok().and_then(|i| i.checked_add(2)) != Some(revision(&e.record))
        })
        || events.last().map(|e| &e.record).unwrap_or(&creation.record) != current
    {
        return Err(TaskStoreError::Invalid("incomplete lifecycle history"));
    }
    let mut prior = &creation.record;
    if creation.references_update.initial() != references(prior) {
        return Err(TaskStoreError::Invalid("creation reference identity"));
    }
    for event in events {
        let expected = match &event.references_update {
            super::references::TaskReferenceUpdate::Preserve => references(prior),
            super::references::TaskReferenceUpdate::Replace(next) => next.as_slice(),
        };
        if expected != references(&event.record) {
            return Err(TaskStoreError::Invalid("reference lifecycle history"));
        }
        prior = &event.record;
    }
    Ok(())
}

#[cfg(test)]
fn validate_session_specification_history(
    original: &SnapshotContext,
    creation: &Creation<Session>,
    events: &[LifecycleEvent<Session>],
) -> Result<(), TaskStoreError> {
    let mut prior = &creation.record;
    for event in events {
        original.check()?;
        validate_session_transition(prior, event)?;
        prior = &event.record;
    }
    Ok(())
}

pub(super) fn validate_session_transition(
    prior: &Session,
    event: &LifecycleEvent<Session>,
) -> Result<(), TaskStoreError> {
    let next = &event.record;
    if next.id != prior.id
        || next.input.idempotency_key != prior.input.idempotency_key
        || next.created_at != prior.created_at
        || next.updated_at < prior.updated_at
    {
        return Err(TaskStoreError::Invalid("session history identity"));
    }
    if let LifecycleKind::Revised { reason } = &event.kind {
        super::canonical(reason)?;
        if prior.status != gates::TaskStatus::InProgress
            || next.status != gates::TaskStatus::InProgress
            || next.progress != prior.progress
            || next.ownership.owner != prior.ownership.owner
            || next.ownership.claim_epoch != prior.ownership.claim_epoch
            || next.ownership.claimed_at != prior.ownership.claimed_at
            || prior.ownership.owner.as_deref() != Some(event.actor.as_str())
            || !prior.ownership.live(event.observed_at)
            || !next.ownership.live(event.observed_at)
            || next.ownership.heartbeat_at != Some(event.observed_at)
            || next.ownership.lease_expires_at < prior.ownership.lease_expires_at
            || (next.input == prior.input && next.references == prior.references)
        {
            return Err(TaskStoreError::Invalid("session revision history"));
        }
    } else if next.input != prior.input {
        return Err(TaskStoreError::Invalid("session specification history"));
    }
    Ok(())
}

pub(super) async fn task_detail_at(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    id: TaskId,
    limits: TaskStoreReadLimits,
    before: impl Fn() -> Result<(), TaskStoreError>,
) -> Result<Option<TaskDetail>, TaskStoreError> {
    let filter = format!(
        "record_id = '{}' AND kind IN ('task','task_creation','task_event','task_comment')",
        model::id(id.get())
    );
    let mut task = None;
    let mut creation = None;
    let mut events = Vec::new();
    let mut comments = Vec::new();
    for r in exact_rows(pin, &filter, original, limits, &before).await? {
        match r {
            rows::Row::Task(t) => {
                if task.replace(t.record).is_some() {
                    return Err(TaskStoreError::Invalid("duplicate task"));
                }
            }
            rows::Row::TaskEvent(e) => events.push(e),
            rows::Row::TaskComment(c) => comments.push(c),
            rows::Row::TaskCreation(c) => {
                if creation.replace(c).is_some() {
                    return Err(TaskStoreError::Invalid("duplicate task creation"));
                }
            }
            _ => return Err(TaskStoreError::Invalid("task row")),
        }
    }
    before()?;
    events.sort_unstable_by_key(|e| e.record.specification.revision);
    match (task, creation) {
        (None, None) if events.is_empty() && comments.is_empty() => Ok(None),
        (Some(task), Some(creation)) => {
            validate_history(
                &task,
                &creation,
                &events,
                |t| t.specification.revision,
                |t| &t.references,
            )?;
            super::comments::assemble(&task, &events, &mut comments)?;
            before()?;
            Ok(Some(TaskDetail {
                task,
                creation,
                events,
                comments: comments.into_iter().map(|c| c.record).collect(),
            }))
        }
        _ => Err(TaskStoreError::Invalid("incomplete task history")),
    }
}

#[cfg(test)]
mod session_revision_history_tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::{
        num::NonZeroU128,
        time::{Duration, Instant},
    };

    fn history() -> (Creation<Session>, Vec<LifecycleEvent<Session>>) {
        let initial = Session {
            id: SessionId::new(NonZeroU128::new(1).unwrap()),
            input: CreateSession {
                idempotency_key: "creation".into(),
                title: "Demand".into(),
                description: "Accepted request".into(),
                strategy: "Original strategy".into(),
            },
            revision: 1,
            references: vec![],
            reference_generation: 1,
            status: gates::TaskStatus::Open,
            ownership: Ownership::default(),
            progress: None,
            created_at: 10,
            updated_at: 10,
        };
        let actor = "a".repeat(64);
        let creation = Creation {
            actor: actor.clone(),
            record: initial.clone(),
            references_update: super::super::references::TaskReferenceUpdate::Preserve,
        };
        let mut claimed = initial;
        claimed.revision = 2;
        claimed.reference_generation = 2;
        claimed.updated_at = 11;
        claimed.status = gates::TaskStatus::InProgress;
        claimed.ownership = Ownership {
            owner: Some(actor.clone()),
            claim_epoch: 1,
            claimed_at: Some(11),
            heartbeat_at: Some(11),
            lease_expires_at: Some(100),
        };
        let claim = LifecycleEvent {
            actor: actor.clone(),
            kind: LifecycleKind::Claimed,
            observed_at: 11,
            record: claimed.clone(),
            references_update: super::super::references::TaskReferenceUpdate::Preserve,
        };
        let mut revised = claimed;
        revised.input.strategy = "Current strategy".into();
        revised.revision = 3;
        revised.reference_generation = 3;
        revised.updated_at = 12;
        revised.ownership.heartbeat_at = Some(12);
        revised.ownership.lease_expires_at = Some(110);
        let revision = LifecycleEvent {
            actor,
            kind: LifecycleKind::Revised {
                reason: "Accepted change".into(),
            },
            observed_at: 12,
            record: revised,
            references_update: super::super::references::TaskReferenceUpdate::Preserve,
        };
        (creation, vec![claim, revision])
    }

    #[test]
    fn session_specification_predecessor_rejects_coherent_but_invalid_events() {
        let cancellation = Cancellation::default();
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            cancellation.clone(),
        );
        let (creation, events) = history();
        assert!(validate_session_specification_history(&original, &creation, &events).is_ok());
        for corruption in 0..9 {
            let mut altered = events.clone();
            match corruption {
                0 => altered[1].kind = LifecycleKind::Renewed,
                1 => {
                    altered[1].kind = LifecycleKind::Revised {
                        reason: " padded ".into(),
                    }
                }
                2 => altered[1].record.input.idempotency_key = "different key".into(),
                3 => altered[1].actor = "b".repeat(64),
                4 => altered[1].record.ownership.claim_epoch = 2,
                5 => altered[1].record.ownership.heartbeat_at = Some(11),
                6 => altered[1].record.input = altered[0].record.input.clone(),
                7 => {
                    altered[1].record.progress = Some(Progress {
                        observation: ProgressObservation {
                            kind: ProgressKind::Checkpoint,
                            sequence: 1,
                            observed_at: 12,
                        },
                        summary: "Unrelated checkpoint".into(),
                        next_step: "Continue".into(),
                    })
                }
                8 => altered[1].record.status = gates::TaskStatus::Completed,
                _ => unreachable!(),
            }
            // Revision sequence, final head and references alone cannot detect these changes.
            validate_history(
                &altered[1].record,
                &creation,
                &altered,
                |record| record.revision,
                |record| &record.references,
            )
            .unwrap();
            assert!(
                validate_session_specification_history(&original, &creation, &altered).is_err(),
                "corruption {corruption}"
            );
        }
        cancellation.cancel();
        assert!(validate_session_specification_history(&original, &creation, &events).is_err());
    }
}
