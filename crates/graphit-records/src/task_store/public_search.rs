//! Public, derived JSON input; private Task payloads are never indexed.
use super::{TaskStoreError, TaskStoreReadLimits, comments, model::*, read, rows};
use futures::TryStreamExt;
use graphit_core::file_io::SnapshotContext;
use graphit_data::{
    lance_branch_store::{BranchError, BranchSnapshot},
    lance_module_store::StoreError,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

pub(super) const COLUMN: &str = "public_search";
pub(super) const INDEX_NAME: &str = "task-public-json-content-v1";

#[derive(Serialize)]
struct SpecificationText<'a> {
    title: &'a str,
    description: &'a str,
    check_text: Vec<&'a str>,
    evidence: Vec<&'a str>,
}
#[derive(Serialize)]
struct CommentText<'a> {
    comment_body: &'a str,
}

pub(super) fn encode(row: &rows::Row) -> Result<String, TaskStoreError> {
    Ok(match row {
        rows::Row::Task(stored) => {
            let s = &stored.record.specification;
            serde_json::to_string(&SpecificationText {
                title: &s.title,
                description: &s.description,
                check_text: s.checks.iter().map(|c| c.text.as_str()).collect(),
                evidence: s
                    .checks
                    .iter()
                    .filter_map(|c| c.verification.as_ref().map(|v| v.evidence.as_str()))
                    .collect(),
            })?
        }
        rows::Row::TaskComment(c) => serde_json::to_string(&CommentText {
            comment_body: &c.body,
        })?,
        _ => "{}".to_owned(),
    })
}

struct History<T> {
    head: Option<T>,
    creation: Option<Creation<T>>,
    events: Vec<LifecycleEvent<T>>,
}
type CompleteHistory<T> = (T, Creation<T>, Vec<LifecycleEvent<T>>);
impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            head: None,
            creation: None,
            events: vec![],
        }
    }
}
impl<T: PartialEq> History<T> {
    fn assemble(
        mut self,
        revision: impl Fn(&T) -> u64,
        references: impl Fn(&T) -> &[super::references::TaskReference],
    ) -> Result<CompleteHistory<T>, TaskStoreError> {
        let head = self.head.ok_or(TaskStoreError::IncompleteCorpus)?;
        let creation = self.creation.ok_or(TaskStoreError::IncompleteCorpus)?;
        self.events.sort_unstable_by_key(|e| revision(&e.record));
        read::validate_history(&head, &creation, &self.events, revision, references)?;
        Ok((head, creation, self.events))
    }
}
#[derive(Default)]
struct TaskRows {
    history: History<Task>,
    comments: Vec<rows::StoredComment>,
}

fn assemble(
    found: Vec<rows::Row>,
    original: &SnapshotContext,
) -> Result<Vec<TaskDetail>, TaskStoreError> {
    let mut tasks: HashMap<crate::task_spec::TaskId, TaskRows> = HashMap::new();
    let mut sessions: HashMap<SessionId, History<Session>> = HashMap::new();
    for row in found {
        original.check()?;
        match row {
            rows::Row::ReferenceProjection(_) | rows::Row::Embedding(_) => {}
            rows::Row::Task(t) => {
                if tasks
                    .entry(t.record.specification.id)
                    .or_default()
                    .history
                    .head
                    .replace(t.record)
                    .is_some()
                {
                    return Err(TaskStoreError::Invalid("duplicate Task head"));
                }
            }
            rows::Row::TaskCreation(c) => {
                if tasks
                    .entry(c.record.specification.id)
                    .or_default()
                    .history
                    .creation
                    .replace(c)
                    .is_some()
                {
                    return Err(TaskStoreError::Invalid("duplicate Task creation"));
                }
            }
            rows::Row::TaskEvent(e) => tasks
                .entry(e.record.specification.id)
                .or_default()
                .history
                .events
                .push(e),
            rows::Row::TaskComment(c) => tasks.entry(c.task_id).or_default().comments.push(c),
            rows::Row::Session(s) => {
                if sessions
                    .entry(s.record.id)
                    .or_default()
                    .head
                    .replace(s.record)
                    .is_some()
                {
                    return Err(TaskStoreError::Invalid("duplicate Session head"));
                }
            }
            rows::Row::SessionCreation(c) => {
                if sessions
                    .entry(c.record.id)
                    .or_default()
                    .creation
                    .replace(c)
                    .is_some()
                {
                    return Err(TaskStoreError::Invalid("duplicate Session creation"));
                }
            }
            rows::Row::SessionEvent(e) => sessions.entry(e.record.id).or_default().events.push(e),
        }
    }
    let mut session_ids = HashSet::new();
    for (id, history) in sessions {
        original.check()?;
        history.assemble(|s| s.revision, |s| &s.references)?;
        session_ids.insert(id);
    }
    let mut result = Vec::new();
    for group in tasks.into_values() {
        original.check()?;
        let (task, creation, events) = group
            .history
            .assemble(|t| t.specification.revision, |t| &t.references)?;
        if !session_ids.contains(&task.session_id) {
            return Err(TaskStoreError::MissingSession);
        }
        let mut comments = group.comments;
        comments::assemble(&task, &events, &mut comments)?;
        result.push(TaskDetail {
            task,
            creation,
            events,
            comments: comments.into_iter().map(|c| c.record).collect(),
        });
    }
    // The established graph also checks missing dependencies, cycles and parent sessions.
    read::task_summaries(&result)?;
    original.check()?;
    Ok(result)
}

/// Validate every physical row, public projection and history from this exact pin.
pub(super) async fn scan_validate(
    pin: &BranchSnapshot,
    original: &SnapshotContext,
    read_limits: TaskStoreReadLimits,
    before: impl Fn() -> Result<(), TaskStoreError>,
) -> Result<Vec<TaskDetail>, TaskStoreError> {
    before()?;
    let mut scan = pin.scan();
    scan.filter("kind != 'embedding'")
        .map_err(StoreError::from)?;
    let base = rows::schema();
    let fields: Vec<_> = base.fields().iter().map(|f| f.name().as_str()).collect();
    scan.project(&fields).map_err(StoreError::from)?;
    scan.batch_size(read_limits.batch_rows.get() as usize);
    let mut stream = crate::storage::read(original, scan.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    before()?;
    let mut keys = HashSet::new();
    let mut found = Vec::new();
    loop {
        before()?;
        let next = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = next else { break };
        if batch.schema() != rows::schema() {
            return Err(BranchError::SchemaMismatch.into());
        }
        for row in 0..batch.num_rows() {
            original.check()?;
            let decoded = rows::decode(&batch, row, original)?;
            let key = rows::required(&batch, "row_key", row)?;
            if !keys.insert(key.to_owned()) {
                return Err(TaskStoreError::Invalid("duplicate Task physical row"));
            }
            found.push(decoded);
        }
    }
    before()?;
    let details = assemble(found, original)?;
    before()?;
    Ok(details)
}
