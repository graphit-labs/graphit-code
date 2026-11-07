use super::{TaskStoreError, model::*};
use crate::{
    gates::TaskStatus,
    task_schedule::TaskSchedule,
    task_spec::{CheckKind, CheckStatus},
};
use arrow_array::{Array, ArrayRef, BooleanArray, RecordBatch, StringArray, UInt64Array};
use arrow_schema::{DataType, Field, Schema};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock};

pub(super) const COLUMNS: &[&str] = &[
    "record_id",
    "session_id",
    "title",
    "task_type",
    "priority",
    "status",
    "parent_id",
    "dependencies",
    "start_date",
    "end_date",
    "due_date",
    "owner",
    "claim_epoch",
    "claimed_at",
    "heartbeat_at",
    "lease_expires_at",
    "progress",
    "flagged",
    "revision",
    "created_at",
    "updated_at",
    "checks",
];
pub(super) fn schema() -> Arc<Schema> {
    static SCHEMA: LazyLock<Arc<Schema>> = LazyLock::new(|| {
        let mut fields = vec![
            Field::new("row_key", DataType::Utf8, false),
            Field::new("kind", DataType::Utf8, false),
            Field::new("idempotency_key", DataType::Utf8, false),
        ];
        for name in COLUMNS {
            let data = if [
                "priority",
                "revision",
                "created_at",
                "updated_at",
                "claim_epoch",
                "claimed_at",
                "heartbeat_at",
                "lease_expires_at",
            ]
            .contains(name)
            {
                DataType::UInt64
            } else if *name == "flagged" {
                DataType::Boolean
            } else {
                DataType::Utf8
            };
            fields.push(Field::new(
                *name,
                data,
                [
                    "parent_id",
                    "start_date",
                    "end_date",
                    "due_date",
                    "owner",
                    "claimed_at",
                    "heartbeat_at",
                    "lease_expires_at",
                    "progress",
                ]
                .contains(name),
            ));
        }
        fields.push(Field::new("payload", DataType::Utf8, false));
        fields.push(Field::new("public_search", DataType::Utf8, false));
        Arc::new(Schema::new(fields))
    });
    Arc::clone(&SCHEMA)
}
pub(super) fn vector_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![Field::new(
        "embedding_vector",
        DataType::List(Arc::new(Field::new("item", DataType::Float32, true))),
        true,
    )]))
}
pub(super) fn validate_schema(actual: &Schema) -> Result<(), TaskStoreError> {
    let base = schema();
    let mut expected = base.fields().to_vec();
    if actual.field_with_name("embedding_vector").is_ok() {
        expected.extend(vector_schema().fields().iter().cloned());
    }
    if *actual != Schema::new(expected) {
        return Err(graphit_data::lance_branch_store::BranchError::SchemaMismatch.into());
    }
    Ok(())
}
/// Existing source mutations write NULL semantic fields; derived rows retain
/// their independent keys and are not overwritten by source-head updates.
pub(super) fn batch_for(rows: &[Row], actual: &Schema) -> Result<RecordBatch, TaskStoreError> {
    validate_schema(actual)?;
    let base = batch(rows)?;
    if actual == base.schema().as_ref() {
        return Ok(base);
    }
    let mut columns = base.columns().to_vec();
    columns.push(arrow_array::new_null_array(
        &vector_schema().field(0).data_type().clone(),
        rows.len(),
    ));
    Ok(RecordBatch::try_new(Arc::new(actual.clone()), columns)?)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckProjection {
    pub kind: CheckKind,
    pub status: CheckStatus,
    pub evidence: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Stored<T> {
    pub record: T,
    pub fence: Option<[u8; 32]>,
}
#[derive(Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "record",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(super) enum Row {
    Embedding(super::embeddings::StoredTaskEmbedding),
    TaskComment(StoredComment),
    ReferenceProjection(super::references::ReferenceProjection),
    Task(Stored<Task>),
    Session(Stored<Session>),
    TaskEvent(LifecycleEvent<Task>),
    SessionEvent(LifecycleEvent<Session>),
    TaskCreation(Creation<Task>),
    SessionCreation(Creation<Session>),
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredComment {
    pub record: TaskComment,
    pub references_update: super::references::TaskReferenceUpdate,
}
impl std::ops::Deref for StoredComment {
    type Target = TaskComment;
    fn deref(&self) -> &TaskComment {
        &self.record
    }
}
impl std::ops::DerefMut for StoredComment {
    fn deref_mut(&mut self) -> &mut TaskComment {
        &mut self.record
    }
}
impl Row {
    fn comment(&self) -> Option<&TaskComment> {
        if let Self::TaskComment(comment) = self {
            Some(&comment.record)
        } else {
            None
        }
    }
    pub fn key(&self) -> String {
        if let Self::Embedding(e) = self {
            return format!("embedding/{}", e.document.source_key);
        }
        if let Self::ReferenceProjection(p) = self {
            return p.key();
        }
        if let Self::TaskComment(c) = self {
            return format!("task_comment/{}/{}", id(c.task_id.get()), c.sequence);
        }
        let (kind, id) = match self {
            Self::TaskComment(_) | Self::ReferenceProjection(_) | Self::Embedding(_) => {
                unreachable!("special key handled")
            }
            Self::Task(x) => ("task", id(x.record.specification.id.get())),
            Self::Session(x) => ("session", id(x.record.id.get())),
            Self::TaskEvent(x) => ("task_event", id(x.record.specification.id.get())),
            Self::SessionEvent(x) => ("session_event", id(x.record.id.get())),
            Self::TaskCreation(x) => ("task_creation", id(x.record.specification.id.get())),
            Self::SessionCreation(x) => ("session_creation", id(x.record.id.get())),
        };
        match self {
            Self::TaskEvent(e) => format!("task_event/{id}/{}", e.record.specification.revision),
            Self::SessionEvent(e) => format!("session_event/{id}/{}", e.record.revision),
            _ => format!("{kind}/{id}"),
        }
    }
    fn task(&self) -> Option<&Task> {
        match self {
            Self::Task(x) => Some(&x.record),
            Self::TaskEvent(x) => Some(&x.record),
            Self::TaskCreation(x) => Some(&x.record),
            _ => None,
        }
    }
    fn session(&self) -> Option<&Session> {
        match self {
            Self::Session(x) => Some(&x.record),
            Self::SessionEvent(x) => Some(&x.record),
            Self::SessionCreation(x) => Some(&x.record),
            _ => None,
        }
    }
}
fn status(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Open => "open",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Completed => "completed",
        TaskStatus::Cancelled => "cancelled",
    }
}
pub(super) fn batch(rows: &[Row]) -> Result<RecordBatch, TaskStoreError> {
    let mut columns: Vec<ArrayRef> = vec![];
    columns.push(Arc::new(StringArray::from(
        rows.iter().map(Row::key).collect::<Vec<_>>(),
    )));
    columns.push(Arc::new(StringArray::from(
        rows.iter()
            .map(|r| match r {
                Row::Embedding(_) => "embedding",
                Row::TaskComment(_) => "task_comment",
                Row::ReferenceProjection(_) => "reference_projection",
                Row::Task(_) => "task",
                Row::Session(_) => "session",
                Row::TaskEvent(_) => "task_event",
                Row::SessionEvent(_) => "session_event",
                Row::TaskCreation(_) => "task_creation",
                Row::SessionCreation(_) => "session_creation",
            })
            .collect::<Vec<_>>(),
    )));
    columns.push(Arc::new(StringArray::from(
        rows.iter()
            .map(|r| {
                if matches!(r, Row::ReferenceProjection(_) | Row::Embedding(_)) {
                    return "";
                }
                r.comment()
                    .map(|c| c.idempotency_key.as_str())
                    .or_else(|| r.task().map(|t| t.idempotency_key.as_str()))
                    .unwrap_or_else(|| r.session().unwrap().input.idempotency_key.as_str())
            })
            .collect::<Vec<_>>(),
    )));
    for name in COLUMNS {
        let string = |r: &Row| -> Result<Option<String>, TaskStoreError> {
            if let Row::Embedding(e) = r {
                return Ok(match *name {
                    "record_id" => Some(id(e.document.task_id.get())),
                    "session_id" | "title" | "task_type" => Some(String::new()),
                    "status" => Some("embedding".into()),
                    "dependencies" | "checks" => Some("[]".into()),
                    _ => None,
                });
            }
            if let Row::ReferenceProjection(p) = r {
                return Ok(match *name {
                    "record_id" => Some(p.source.identifier()),
                    "session_id" | "title" | "task_type" => Some(String::new()),
                    "status" => Some("references".into()),
                    "dependencies" | "checks" => Some("[]".into()),
                    _ => None,
                });
            }
            if let Some(c) = r.comment() {
                return Ok(match *name {
                    "record_id" => Some(id(c.task_id.get())),
                    "session_id" => Some(id(c.session_id.get())),
                    "title" | "task_type" => Some(String::new()),
                    "status" => Some("comment".into()),
                    "dependencies" | "checks" => Some("[]".into()),
                    _ => None,
                });
            }
            let t = r.task();
            let s = r.session();
            Ok(match *name {
                "record_id" => Some(
                    t.map(|x| id(x.specification.id.get()))
                        .unwrap_or_else(|| id(s.unwrap().id.get())),
                ),
                "session_id" => Some(
                    t.map(|x| id(x.session_id.get()))
                        .unwrap_or_else(|| id(s.unwrap().id.get())),
                ),
                "title" => Some(
                    t.map(|x| x.specification.title.clone())
                        .unwrap_or_else(|| s.unwrap().input.title.clone()),
                ),
                "task_type" => Some(
                    t.map(|x| x.specification.task_type.clone())
                        .unwrap_or_default(),
                ),
                "status" => {
                    Some(status(t.map(|x| x.status).unwrap_or_else(|| s.unwrap().status)).into())
                }
                "parent_id" => t.and_then(|x| x.parent_id).map(|x| id(x.get())),
                "dependencies" => Some(serde_json::to_string(
                    &t.map(|x| x.depends_on.as_slice()).unwrap_or_default(),
                )?),
                "start_date" => t
                    .and_then(|x| x.specification.schedule)
                    .and_then(TaskSchedule::start_date)
                    .map(|d| d.to_string()),
                "end_date" => t
                    .and_then(|x| x.specification.schedule)
                    .and_then(TaskSchedule::end_date)
                    .map(|d| d.to_string()),
                "due_date" => t
                    .and_then(|x| x.specification.schedule)
                    .and_then(TaskSchedule::due_date)
                    .map(|d| d.to_string()),
                "owner" => t
                    .map(|x| &x.ownership)
                    .unwrap_or_else(|| &s.unwrap().ownership)
                    .owner
                    .clone(),
                "progress" => t
                    .and_then(|x| x.progress.as_ref())
                    .or_else(|| s.and_then(|x| x.progress.as_ref()))
                    .map(|x| serde_json::to_string(&x.observation))
                    .transpose()?,
                "checks" => Some(serde_json::to_string(
                    &t.map(|x| {
                        x.specification
                            .checks
                            .iter()
                            .map(|c| CheckProjection {
                                kind: c.kind,
                                status: c.status,
                                evidence: c.verification.as_ref().map(|v| v.evidence.clone()),
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
                )?),
                _ => None,
            })
        };
        if *name == "flagged" {
            columns.push(Arc::new(BooleanArray::from(
                rows.iter()
                    .map(|r| r.task().is_some_and(|x| x.flagged))
                    .collect::<Vec<_>>(),
            )));
        } else if [
            "priority",
            "revision",
            "created_at",
            "updated_at",
            "claim_epoch",
            "claimed_at",
            "heartbeat_at",
            "lease_expires_at",
        ]
        .contains(name)
        {
            columns.push(Arc::new(UInt64Array::from(
                rows.iter()
                    .map(|r| {
                        if let Row::Embedding(_) = r {
                            return Some(0);
                        }
                        if let Row::ReferenceProjection(p) = r {
                            return match *name {
                                "revision" => Some(p.revision),
                                "priority" | "claim_epoch" | "created_at" | "updated_at" => Some(0),
                                _ => None,
                            };
                        }
                        if let Some(c) = r.comment() {
                            return match *name {
                                "priority" | "claim_epoch" => Some(0),
                                "revision" => Some(c.revision),
                                "created_at" | "updated_at" => Some(c.at),
                                _ => None,
                            };
                        }
                        let t = r.task();
                        let s = r.session();
                        match *name {
                            "priority" => {
                                Some(t.map(|x| u64::from(x.specification.priority)).unwrap_or(0))
                            }
                            "claim_epoch" => Some(
                                t.map(|x| x.ownership.claim_epoch)
                                    .unwrap_or_else(|| s.unwrap().ownership.claim_epoch),
                            ),
                            "claimed_at" => {
                                t.map(|x| &x.ownership)
                                    .unwrap_or_else(|| &s.unwrap().ownership)
                                    .claimed_at
                            }
                            "heartbeat_at" => {
                                t.map(|x| &x.ownership)
                                    .unwrap_or_else(|| &s.unwrap().ownership)
                                    .heartbeat_at
                            }
                            "lease_expires_at" => {
                                t.map(|x| &x.ownership)
                                    .unwrap_or_else(|| &s.unwrap().ownership)
                                    .lease_expires_at
                            }
                            "revision" => Some(
                                t.map(|x| x.specification.revision)
                                    .unwrap_or_else(|| s.unwrap().revision),
                            ),
                            "created_at" => Some(
                                t.map(|x| x.created_at)
                                    .unwrap_or_else(|| s.unwrap().created_at),
                            ),
                            _ => Some(
                                t.map(|x| x.updated_at)
                                    .unwrap_or_else(|| s.unwrap().updated_at),
                            ),
                        }
                    })
                    .collect::<Vec<_>>(),
            )));
        } else {
            columns.push(Arc::new(StringArray::from(
                rows.iter().map(string).collect::<Result<Vec<_>, _>>()?,
            )));
        }
    }
    columns.push(Arc::new(StringArray::from(
        rows.iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()?,
    )));
    columns.push(Arc::new(StringArray::from(
        rows.iter()
            .map(super::public_search::encode)
            .collect::<Result<Vec<_>, _>>()?,
    )));
    Ok(RecordBatch::try_new(schema(), columns)?)
}
pub(super) fn text<'a>(
    batch: &'a RecordBatch,
    name: &str,
    row: usize,
) -> Result<Option<&'a str>, TaskStoreError> {
    let array = batch
        .column_by_name(name)
        .and_then(|a| a.as_any().downcast_ref::<StringArray>())
        .ok_or(TaskStoreError::Invalid("text column"))?;
    Ok((!array.is_null(row)).then(|| array.value(row)))
}
pub(super) fn required<'a>(
    batch: &'a RecordBatch,
    name: &str,
    row: usize,
) -> Result<&'a str, TaskStoreError> {
    text(batch, name, row)?.ok_or(TaskStoreError::Invalid("null column"))
}
pub(super) fn number(batch: &RecordBatch, name: &str, row: usize) -> Result<u64, TaskStoreError> {
    let a = batch
        .column_by_name(name)
        .and_then(|a| a.as_any().downcast_ref::<UInt64Array>())
        .ok_or(TaskStoreError::Invalid("integer column"))?;
    if a.is_null(row) {
        Err(TaskStoreError::Invalid("null integer"))
    } else {
        Ok(a.value(row))
    }
}
pub(super) fn boolean(batch: &RecordBatch, name: &str, row: usize) -> Result<bool, TaskStoreError> {
    let a = batch
        .column_by_name(name)
        .and_then(|a| a.as_any().downcast_ref::<BooleanArray>())
        .ok_or(TaskStoreError::Invalid("boolean column"))?;
    if a.is_null(row) {
        Err(TaskStoreError::Invalid("null boolean"))
    } else {
        Ok(a.value(row))
    }
}
pub(super) fn decode(
    batch: &RecordBatch,
    row: usize,
    original: &graphit_core::file_io::SnapshotContext,
) -> Result<Row, TaskStoreError> {
    let value: Row = serde_json::from_str(required(batch, "payload", row)?)?;
    let validate_task = |task: &Task| -> Result<(), TaskStoreError> {
        task.specification.validate(original)?;
        if let Some(commit) = &task.commit {
            graphit_core::git_record_event::validate_commit_id(commit)
                .map_err(|_| TaskStoreError::Invalid("invalid task commit"))?;
        }
        super::references::validate_authored(original, &task.references)?;
        if task.reference_generation != task.specification.revision {
            return Err(TaskStoreError::Invalid("task reference generation"));
        }
        task.ownership.validate()?;
        validate_progress(task.progress.as_ref())?;
        super::canonical(&task.idempotency_key)?;
        if task.updated_at < task.created_at
            || task.parent_id == Some(task.specification.id)
            || task.depends_on.contains(&task.specification.id)
            || task
                .depends_on
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != task.depends_on.len()
        {
            return Err(TaskStoreError::Invalid("task record"));
        }
        Ok(())
    };
    let validate_session = |session: &Session| -> Result<(), TaskStoreError> {
        super::references::validate_authored(original, &session.references)?;
        if session.reference_generation != session.revision {
            return Err(TaskStoreError::Invalid("session reference generation"));
        }
        session.ownership.validate()?;
        validate_progress(session.progress.as_ref())?;
        for text in [
            &session.input.idempotency_key,
            &session.input.title,
            &session.input.description,
            &session.input.strategy,
        ] {
            super::canonical(text)?;
        }
        if session.revision == 0 || session.updated_at < session.created_at {
            return Err(TaskStoreError::Invalid("session record"));
        }
        Ok(())
    };
    let validate_actor = |actor: &str| -> Result<(), TaskStoreError> {
        if !crate::actor::valid_identifier(actor) {
            Err(TaskStoreError::Invalid("creation actor"))
        } else {
            Ok(())
        }
    };
    let validate_event = |kind: &LifecycleKind, status: TaskStatus| -> Result<(), TaskStoreError> {
        match kind {
            LifecycleKind::Revised { reason }
            | LifecycleKind::CheckSuperseded { reason, .. }
            | LifecycleKind::LinksUpdated { reason }
            | LifecycleKind::FlagUpdated { reason, .. }
            | LifecycleKind::CommitUpdated { reason }
            | LifecycleKind::Cancelled { reason } => super::canonical(reason)?,
            _ => {}
        }
        if matches!(kind, LifecycleKind::Completed) && status != TaskStatus::Completed
            || matches!(kind, LifecycleKind::Cancelled { .. }) && status != TaskStatus::Cancelled
        {
            return Err(TaskStoreError::Invalid("terminal event status"));
        }
        Ok(())
    };
    match &value {
        Row::Embedding(e) => e.validate()?,
        Row::ReferenceProjection(p) => p.validate(original)?,
        Row::TaskComment(c) => {
            c.validate().map_err(TaskStoreError::Invalid)?;
            validate_update(original, &c.references_update)?;
        }
        Row::Task(t) => {
            validate_task(&t.record)?;
            validate_fence(&t.record.ownership, t.fence.as_ref())?;
        }
        Row::Session(s) => {
            validate_session(&s.record)?;
            validate_fence(&s.record.ownership, s.fence.as_ref())?;
        }
        Row::TaskEvent(e) => {
            validate_update(original, &e.references_update)?;
            validate_task(&e.record)?;
            validate_actor(&e.actor)?;
            validate_event(&e.kind, e.record.status)?;
            if e.observed_at != e.record.updated_at {
                return Err(TaskStoreError::Invalid("event timestamp"));
            }
        }
        Row::SessionEvent(e) => {
            validate_update(original, &e.references_update)?;
            validate_session(&e.record)?;
            validate_actor(&e.actor)?;
            validate_event(&e.kind, e.record.status)?;
            if !matches!(
                &e.kind,
                LifecycleKind::Claimed
                    | LifecycleKind::Renewed
                    | LifecycleKind::Progress
                    | LifecycleKind::Released
                    | LifecycleKind::Revised { .. }
                    | LifecycleKind::Completed
            ) {
                return Err(TaskStoreError::Invalid("non-session lifecycle event"));
            }
            if e.observed_at != e.record.updated_at {
                return Err(TaskStoreError::Invalid("event timestamp"));
            }
        }
        Row::TaskCreation(c) => {
            validate_update(original, &c.references_update)?;
            validate_task(&c.record)?;
            validate_actor(&c.actor)?;
            if c.record.specification.revision != 1
                || c.references_update.initial() != c.record.references
            {
                return Err(TaskStoreError::Invalid("initial task revision"));
            }
        }
        Row::SessionCreation(c) => {
            validate_update(original, &c.references_update)?;
            validate_session(&c.record)?;
            validate_actor(&c.actor)?;
            if c.record.revision != 1 || c.references_update.initial() != c.record.references {
                return Err(TaskStoreError::Invalid("initial session revision"));
            }
        }
    }
    if let Some(column) = batch.column_by_name("embedding_vector") {
        let list = column
            .as_any()
            .downcast_ref::<arrow_array::ListArray>()
            .ok_or(TaskStoreError::Invalid("Task embedding vector column"))?;
        if let Row::Embedding(e) = &value {
            let values = list
                .values()
                .as_any()
                .downcast_ref::<arrow_array::Float32Array>()
                .ok_or(TaskStoreError::Invalid("Task embedding vector values"))?;
            let offsets = list.value_offsets();
            let start = offsets[row] as usize;
            let end = offsets[row + 1] as usize;
            if list.is_null(row)
                || end - start != e.document.identity().dimensions().get() as usize
                || (start..end).any(|i| values.is_null(i) || !values.value(i).is_finite())
            {
                return Err(TaskStoreError::Invalid("Task embedding vector"));
            }
        } else if !list.is_null(row) {
            return Err(TaskStoreError::Invalid(
                "Task source row contains derived vector",
            ));
        }
    }
    let encoded = self::batch_for(std::slice::from_ref(&value), batch.schema().as_ref())?;
    for (actual, expected) in batch
        .columns()
        .iter()
        .zip(encoded.columns())
        .take(schema().fields().len())
    {
        if actual.slice(row, 1).as_ref() != expected.as_ref() {
            return Err(TaskStoreError::Invalid("projection disagrees with payload"));
        }
    }
    Ok(value)
}

pub(super) fn validate_progress(p: Option<&Progress>) -> Result<(), TaskStoreError> {
    if let Some(p) = p {
        if p.observation.sequence == 0 {
            return Err(TaskStoreError::Invalid("progress sequence"));
        }
        super::canonical(&p.summary)?;
        super::canonical(&p.next_step)?;
    }
    Ok(())
}
fn validate_fence(o: &Ownership, f: Option<&[u8; 32]>) -> Result<(), TaskStoreError> {
    if o.owner.is_some() != f.is_some() {
        Err(TaskStoreError::Invalid("private claim fence"))
    } else {
        Ok(())
    }
}
pub(super) fn optional_number(
    batch: &RecordBatch,
    name: &str,
    row: usize,
) -> Result<Option<u64>, TaskStoreError> {
    let a = batch
        .column_by_name(name)
        .and_then(|a| a.as_any().downcast_ref::<UInt64Array>())
        .ok_or(TaskStoreError::Invalid("integer column"))?;
    Ok((!a.is_null(row)).then(|| a.value(row)))
}
pub(super) fn ownership(batch: &RecordBatch, row: usize) -> Result<Ownership, TaskStoreError> {
    let o = Ownership {
        owner: text(batch, "owner", row)?.map(str::to_owned),
        claim_epoch: number(batch, "claim_epoch", row)?,
        claimed_at: optional_number(batch, "claimed_at", row)?,
        heartbeat_at: optional_number(batch, "heartbeat_at", row)?,
        lease_expires_at: optional_number(batch, "lease_expires_at", row)?,
    };
    o.validate()?;
    Ok(o)
}
pub(super) fn progress(
    batch: &RecordBatch,
    row: usize,
) -> Result<Option<ProgressObservation>, TaskStoreError> {
    let p: Option<ProgressObservation> = text(batch, "progress", row)?
        .map(serde_json::from_str)
        .transpose()?;
    if p.as_ref().is_some_and(|p| p.sequence == 0) {
        return Err(TaskStoreError::Invalid("progress projection"));
    }
    Ok(p)
}

fn validate_update(
    original: &graphit_core::file_io::SnapshotContext,
    patch: &super::references::TaskReferenceUpdate,
) -> Result<(), TaskStoreError> {
    if let super::references::TaskReferenceUpdate::Replace(refs) = patch {
        super::references::validate_authored(original, refs)?;
    }
    Ok(())
}
