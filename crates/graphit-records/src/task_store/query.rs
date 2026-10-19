//! One authenticated SDK pin, finite projection and generation-bound keyset pages.
use super::{query_model::QueryBinding, *};
use crate::{gates::TaskStatus, task_spec::TaskId};
use arrow_array::RecordBatch;
use futures::TryStreamExt;
use graphit_auth::ProjectReadAdmission;
use graphit_core::{file_io::SnapshotContext, native_path::NativePath};
use graphit_data::lance_module_store::StoreError;
use lance::dataset::scanner::ColumnOrdering;
use serde_json::Value;
use std::{collections::HashSet, sync::Arc};

const PUBLIC_COLUMNS: [TaskQueryColumn; 11] = [
    TaskQueryColumn::Id,
    TaskQueryColumn::Status,
    TaskQueryColumn::Title,
    TaskQueryColumn::Revision,
    TaskQueryColumn::CreatedAt,
    TaskQueryColumn::UpdatedAt,
    TaskQueryColumn::SessionId,
    TaskQueryColumn::ParentId,
    TaskQueryColumn::TaskType,
    TaskQueryColumn::Priority,
    TaskQueryColumn::Flagged,
];
fn status_text(status: TaskStatus) -> Result<String, TaskStoreError> {
    serde_json::to_value(status)?
        .as_str()
        .map(str::to_owned)
        .ok_or(TaskStoreError::Invalid("Task status representation"))
}
fn ids_clause(ids: impl Iterator<Item = String>) -> String {
    let values: Vec<_> = ids.map(|id| format!("'{id}'")).collect();
    if values.is_empty() {
        "false".into()
    } else {
        format!("record_id IN ({})", values.join(","))
    }
}
fn predicate(selection: &TaskQuerySelection) -> Result<String, TaskStoreError> {
    let mut clauses = Vec::new();
    let status = match selection {
        TaskQuerySelection::Tasks {
            ids,
            status,
            session_id,
        } => {
            clauses.push("kind = 'task'".to_owned());
            if let Some(ids) = ids {
                clauses.push(ids_clause(ids.iter().map(|id| model::id(id.get()))));
            }
            if let Some(id) = session_id {
                clauses.push(format!("session_id = '{}'", model::id(id.get())));
            }
            status
        }
        TaskQuerySelection::Sessions { ids, status } => {
            clauses.push("kind = 'session'".to_owned());
            if let Some(ids) = ids {
                clauses.push(ids_clause(ids.iter().map(|id| model::id(id.get()))));
            }
            status
        }
    };
    if let Some(status) = status {
        clauses.push(format!("status = '{}'", status_text(*status)?));
    }
    Ok(clauses.join(" AND "))
}
fn value(
    batch: &RecordBatch,
    column: TaskQueryColumn,
    row: usize,
) -> Result<Value, TaskStoreError> {
    let name = column.physical();
    Ok(match column.data_type() {
        TaskQueryType::UnsignedInteger => {
            let number = rows::number(batch, name, row)?;
            if (column == TaskQueryColumn::Priority && number > 4)
                || (column == TaskQueryColumn::Revision && number == 0)
            {
                return Err(TaskStoreError::Invalid("Task query numeric projection"));
            }
            Value::from(number)
        }
        TaskQueryType::Boolean => Value::Bool(rows::boolean(batch, name, row)?),
        _ => {
            let text = if column == TaskQueryColumn::ParentId {
                rows::text(batch, name, row)?
            } else {
                Some(rows::required(batch, name, row)?)
            };
            if let Some(text) = text {
                match column.data_type() {
                    TaskQueryType::Ulid => {
                        let _: TaskId = serde_json::from_value(Value::String(text.into()))?;
                    }
                    TaskQueryType::TaskStatus => {
                        let _: TaskStatus = serde_json::from_value(Value::String(text.into()))?;
                    }
                    _ => {
                        canonical(text)?;
                    }
                }
                Value::String(text.into())
            } else {
                Value::Null
            }
        }
    })
}
impl NativeTaskStore {
    /// Describe only public query views; requires the same real Task read authority.
    pub fn query_schema(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
    ) -> Result<TaskQuerySchema, TaskStoreError> {
        native::check_read(original, admission)?;
        let tables = [TaskQueryTable::Tasks, TaskQueryTable::Sessions]
            .into_iter()
            .map(|table| {
                let columns = PUBLIC_COLUMNS
                    .into_iter()
                    .filter(|column| table == TaskQueryTable::Tasks || !column.task_only())
                    .map(|name| TaskQueryColumnSchema {
                        name,
                        data_type: name.data_type(),
                        nullable: name == TaskQueryColumn::ParentId,
                    })
                    .collect();
                TaskQueryTableSchema { table, columns }
            })
            .collect();
        native::check_read(original, admission)?;
        Ok(TaskQuerySchema { tables })
    }
    /// Query public scalar columns without materializing histories or the readiness graph.
    pub async fn query(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        limits: TaskStoreReadLimits,
        query: TaskQuery,
    ) -> Result<TaskQueryRead, TaskStoreError> {
        native::check_read(original, admission)?;
        let mut seen = HashSet::new();
        if query.columns.is_empty()
            || query.columns.iter().any(|c| {
                !seen.insert(*c)
                    || (matches!(query.selection, TaskQuerySelection::Sessions { .. })
                        && c.task_only())
            })
        {
            return Err(TaskStoreError::Invalid("Task query columns"));
        }
        let Some(capture) = native::read_pin(original, admission, limits).await? else {
            if query.cursor.is_some() {
                return Err(TaskStoreError::InvalidCursor);
            }
            native::check_read(original, admission)?;
            return Ok(TaskQueryRead::NoCorpus);
        };
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, TaskStoreError>(())
        };
        before()?;
        let project = admission.project()?;
        let binding = QueryBinding {
            corpus_key: capture.guard.corpus_key(capture.pin.corpus_uri()),
            project_id: native::project_id(project.scope.live)?,
            root: NativePath::new(project.scope.live.owner_root().to_owned())?,
            brand: project.scope.live.brand().directory().to_owned(),
            project_version: project.version.version().to_owned(),
            version: capture.pin.version(),
            physical_branch: capture.pin.physical_branch().map(str::to_owned),
            inherited_target: capture.pin.inherited_target().map(str::to_owned),
            selection: query.selection.clone(),
            columns: query.columns.clone(),
            page_size: query.page_size,
            top_k: query.top_k,
        };
        let consumed = match &query.cursor {
            None => 0,
            Some(cursor)
                if *cursor.binding == binding
                    && cursor.consumed > 0
                    && cursor.consumed < query.top_k.get()
                    && cursor.consumed.is_multiple_of(query.page_size.get()) =>
            {
                cursor.consumed
            }
            _ => return Err(TaskStoreError::InvalidCursor),
        };
        let take = query.page_size.get().min(query.top_k.get() - consumed);
        let scan_limit = take
            .checked_add(1)
            .and_then(|n| i64::try_from(n).ok())
            .ok_or(TaskStoreError::Invalid("Task query page size"))?;
        let mut filter = predicate(&query.selection)?;
        if let Some(cursor) = &query.cursor {
            filter.push_str(&format!(
                " AND record_id > '{}'",
                model::id(cursor.after.get())
            ));
        }
        let mut columns: Vec<_> = query.columns.iter().map(|c| c.physical()).collect();
        if !columns.contains(&"record_id") {
            columns.push("record_id");
        }
        let mut scan = capture.pin.scan();
        scan.filter(&filter)
            .map_err(StoreError::from)?
            .project(&columns)
            .map_err(StoreError::from)?
            .order_by(Some(vec![ColumnOrdering::asc_nulls_last(
                "record_id".into(),
            )]))
            .map_err(StoreError::from)?
            .limit(Some(scan_limit), None)
            .map_err(StoreError::from)?
            .batch_size(limits.batch_rows.get() as usize);
        let mut stream = crate::storage::read(original, scan.try_into_stream())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let mut results = Vec::new();
        let mut last: Option<TaskId> = query.cursor.as_ref().map(|c| c.after);
        let mut has_more = false;
        loop {
            before()?;
            let batch = crate::storage::read(original, stream.try_next())
                .await?
                .map_err(StoreError::from)?;
            before()?;
            let Some(batch) = batch else { break };
            for row in 0..batch.num_rows() {
                before()?;
                let id: TaskId = serde_json::from_value(Value::String(
                    rows::required(&batch, "record_id", row)?.into(),
                ))?;
                if last.is_some_and(|last| last.get() >= id.get()) {
                    return Err(TaskStoreError::Invalid("Task query ordering"));
                }
                if results.len() < take {
                    let values = query
                        .columns
                        .iter()
                        .map(|c| value(&batch, *c, row))
                        .collect::<Result<Vec<_>, _>>()?;
                    results.push(values);
                    last = Some(id);
                } else {
                    has_more = true;
                }
            }
        }
        before()?;
        let consumed = consumed + results.len();
        let next_cursor = if has_more && consumed < query.top_k.get() {
            Some(TaskQueryCursor {
                binding: Box::new(binding.clone()),
                after: last.ok_or(TaskStoreError::Invalid("Task query empty continuation"))?,
                consumed,
            })
        } else {
            None
        };
        Ok(TaskQueryRead::Published {
            generation: TaskGeneration {
                version: binding.version,
                physical_branch: binding.physical_branch,
                inherited_target: binding.inherited_target,
            },
            columns: query.columns,
            rows: results,
            next_cursor,
        })
    }
}
