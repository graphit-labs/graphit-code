//! Selected Task detail reads. Full audit/idempotency readers remain separate.
use super::{get_model::*, *};
use crate::task_spec::TaskId;
use futures::TryStreamExt;
use graphit_auth::ProjectReadAdmission;
use graphit_core::{file_io::SnapshotContext, native_path::NativePath};
use graphit_data::{lance_branch_store::BranchSnapshot, lance_module_store::StoreError};
use lance::dataset::scanner::ColumnOrdering;
use std::{collections::HashSet, sync::Arc};

struct Reader<'a, F> {
    pin: &'a BranchSnapshot,
    original: &'a Arc<SnapshotContext>,
    limits: TaskStoreReadLimits,
    before: F,
    id: TaskId,
}
impl<F: Fn() -> Result<(), TaskStoreError>> Reader<'_, F> {
    async fn rows(&self, clause: &str, take: usize) -> Result<Vec<rows::Row>, TaskStoreError> {
        (self.before)()?;
        let count =
            i64::try_from(take).map_err(|_| TaskStoreError::Invalid("Task detail page size"))?;
        let mut scan = self.pin.scan();
        scan.filter(&format!(
            "record_id = '{}' AND ({clause})",
            model::id(self.id.get())
        ))
        .map_err(StoreError::from)?
        .order_by(Some(vec![ColumnOrdering::asc_nulls_last(
            "revision".into(),
        )]))
        .map_err(StoreError::from)?
        .limit(Some(count), None)
        .map_err(StoreError::from)?
        .batch_size(self.limits.batch_rows.get() as usize);
        let mut stream = crate::storage::read(self.original, scan.try_into_stream())
            .await?
            .map_err(StoreError::from)?;
        let mut found = Vec::new();
        loop {
            (self.before)()?;
            let next = crate::storage::read(self.original, stream.try_next())
                .await?
                .map_err(StoreError::from)?;
            (self.before)()?;
            let Some(batch) = next else { break };
            for row in 0..batch.num_rows() {
                (self.before)()?;
                found.push(rows::decode(&batch, row, self.original)?);
            }
        }
        (self.before)()?;
        Ok(found)
    }
    async fn event(&self, revision: u64) -> Result<LifecycleEvent<Task>, TaskStoreError> {
        let mut found = self
            .rows(&format!("kind = 'task_event' AND revision = {revision}"), 2)
            .await?;
        if found.len() != 1 {
            return Err(TaskStoreError::Invalid(
                "missing or duplicate selected Task event",
            ));
        }
        match found.pop().unwrap() {
            rows::Row::TaskEvent(event) if event.record.specification.revision == revision => {
                Ok(event)
            }
            _ => Err(TaskStoreError::Invalid("selected Task event")),
        }
    }
    async fn prior(
        &self,
        revision: u64,
        creation: &Creation<Task>,
    ) -> Result<Task, TaskStoreError> {
        if revision == 1 {
            Ok(creation.record.clone())
        } else {
            Ok(self.event(revision).await?.record)
        }
    }
    async fn comment(&self, sequence: u64) -> Result<rows::StoredComment, TaskStoreError> {
        let key = format!("task_comment/{}/{sequence}", model::id(self.id.get()));
        let mut found = self
            .rows(&format!("kind = 'task_comment' AND row_key = '{key}'"), 2)
            .await?;
        if found.len() != 1 {
            return Err(TaskStoreError::Invalid(
                "missing or duplicate selected comment",
            ));
        }
        match found.pop().unwrap() {
            rows::Row::TaskComment(comment) => Ok(comment),
            _ => Err(TaskStoreError::Invalid("selected comment row")),
        }
    }
    async fn event_comment(&self, event: &LifecycleEvent<Task>) -> Result<(), TaskStoreError> {
        if let LifecycleKind::CommentAdded { sequence, .. } = &event.kind {
            let comment = self.comment(sequence.get()).await?;
            comments::validate_binding(event, &comment)?;
        }
        Ok(())
    }
}
fn transition(prior: &Task, event: &LifecycleEvent<Task>) -> Result<(), TaskStoreError> {
    let next = &event.record;
    if prior.specification.revision.checked_add(1) != Some(next.specification.revision)
        || prior.specification.id != next.specification.id
        || prior.session_id != next.session_id
        || prior.idempotency_key != next.idempotency_key
        || prior.created_at != next.created_at
        || event.observed_at != next.updated_at
        || next.updated_at < prior.updated_at
    {
        return Err(TaskStoreError::Invalid("selected Task transition"));
    }
    let references = match &event.references_update {
        TaskReferenceUpdate::Preserve => &prior.references,
        TaskReferenceUpdate::Replace(next) => next,
    };
    if references != &next.references {
        return Err(TaskStoreError::Invalid("reference lifecycle history"));
    }
    Ok(())
}
fn continuation(
    input: &TaskDetailPageInput,
    binding: &DetailBinding,
    section: DetailSection,
) -> Result<(u64, u64), TaskStoreError> {
    match &input.cursor {
        None => Ok((1, 0)),
        Some(cursor)
            if *cursor.binding == *binding
                && cursor.section == section
                && cursor.after_revision >= 1
                && match section {
                    DetailSection::History => cursor.after_sequence == 0,
                    DetailSection::Comments => {
                        (cursor.after_revision == 1 && cursor.after_sequence == 0)
                            || (cursor.after_revision > 1 && cursor.after_sequence > 0)
                    }
                } =>
        {
            Ok((cursor.after_revision, cursor.after_sequence))
        }
        _ => Err(TaskStoreError::InvalidCursor),
    }
}
fn cursor(
    binding: &DetailBinding,
    section: DetailSection,
    after_revision: u64,
    after_sequence: u64,
) -> TaskDetailCursor {
    TaskDetailCursor {
        binding: Box::new(binding.clone()),
        section,
        after_revision,
        after_sequence,
    }
}
impl NativeTaskStore {
    /// Read the current selected record and only explicitly requested detail pages.
    pub async fn read_task(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        input: TaskGetInput,
        limits: TaskStoreReadLimits,
    ) -> Result<Option<TaskGetRead>, TaskStoreError> {
        let Some(capture) = native::read_pin(original, admission, limits).await? else {
            if input
                .history
                .as_ref()
                .and_then(|p| p.cursor.as_ref())
                .is_some()
                || input
                    .comments
                    .as_ref()
                    .and_then(|p| p.cursor.as_ref())
                    .is_some()
            {
                return Err(TaskStoreError::InvalidCursor);
            }
            return Ok(None);
        };
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok(())
        };
        before()?;
        let project = admission.project()?;
        let mut binding = DetailBinding {
            corpus_key: capture.guard.corpus_key(capture.pin.corpus_uri()),
            root: NativePath::new(project.scope.live.owner_root().to_owned())?,
            project_id: native::project_id(project.scope.live)?,
            brand: project.scope.live.brand().directory().to_owned(),
            project_version: project.version.version().to_owned(),
            generation: TaskGeneration {
                version: capture.pin.version(),
                physical_branch: capture.pin.physical_branch().map(str::to_owned),
                inherited_target: capture.pin.inherited_target().map(str::to_owned),
            },
            branch_provenance: None,
            task_id: input.id,
        };
        let selected = input
            .history
            .as_ref()
            .and_then(|p| p.cursor.as_ref())
            .or_else(|| input.comments.as_ref().and_then(|p| p.cursor.as_ref()));
        let inherited = if let Some(cursor) = selected {
            let mut identity = (*cursor.binding).clone();
            identity.generation = binding.generation.clone();
            identity.branch_provenance = None;
            if identity != binding {
                return Err(TaskStoreError::InvalidCursor);
            }
            let logical_branch = project.version.lance_branch();
            let inherited = match (
                cursor.binding.generation.physical_branch.as_deref(),
                cursor.binding.generation.inherited_target.as_deref(),
            ) {
                (None, Some(target)) if !logical_branch.is_empty() && target == logical_branch => {
                    true
                }
                (physical, None)
                    if physical
                        == (!logical_branch.is_empty()).then_some(logical_branch.as_str()) =>
                {
                    false
                }
                _ => return Err(TaskStoreError::InvalidCursor),
            };
            if !inherited
                && cursor.binding.generation.physical_branch.is_some()
                && cursor.binding.branch_provenance.is_none()
            {
                return Err(TaskStoreError::InvalidCursor);
            }
            binding = (*cursor.binding).clone();
            inherited
        } else {
            capture.pin.inherited_target().is_some()
        };
        // Validate both cursors before any selected payload read.
        let history_after = input
            .history
            .as_ref()
            .map(|p| continuation(p, &binding, DetailSection::History))
            .transpose()?;
        let comments_after = input
            .comments
            .as_ref()
            .map(|p| continuation(p, &binding, DetailSection::Comments))
            .transpose()?;
        // Only the admitted receiver selects storage. A cursor carries no URI
        // or branch authority; the SDK proves any inherited parent/fork access.
        let (pin, provenance) = if inherited && capture.pin.physical_branch().is_some() {
            let (pin, provenance) = crate::storage::read(
                original,
                capture.pin.at_inherited_version(binding.generation.version),
            )
            .await??;
            (pin, Some(provenance))
        } else {
            crate::storage::read(
                original,
                capture
                    .pin
                    .at_version_with_provenance(binding.generation.version),
            )
            .await??
        };
        before()?;
        if pin.version() != binding.generation.version
            || pin.physical_branch() != binding.generation.physical_branch.as_deref()
            || pin.inherited_target() != binding.generation.inherited_target.as_deref()
            || (binding.branch_provenance.is_some() && binding.branch_provenance != provenance)
        {
            return Err(TaskStoreError::InvalidCursor);
        }
        // An originally absent child gains authoritative provenance when it
        // materializes. Carry that proof on every continuation returned now.
        binding.branch_provenance = provenance;
        let reader = Reader {
            pin: &pin,
            original,
            limits,
            before: &before,
            id: input.id,
        };
        let mut task = None;
        let mut creation = None;
        // At most one head and one creation. The third row diagnoses duplicates.
        for row in reader.rows("kind IN ('task','task_creation')", 3).await? {
            match row {
                rows::Row::Task(t) if task.is_none() => task = Some(t.record),
                rows::Row::TaskCreation(c) if creation.is_none() => creation = Some(c),
                _ => return Err(TaskStoreError::Invalid("duplicate Task head or creation")),
            }
        }
        let (task, creation) = match (task, creation) {
            (None, None) => return Ok(None),
            (Some(task), Some(creation)) => (task, creation),
            _ => return Err(TaskStoreError::Invalid("incomplete Task head")),
        };
        if creation.record.specification.id != input.id
            || creation.record.specification.revision != 1
            || creation.references_update.initial() != creation.record.references.as_slice()
            || task.specification.id != input.id
            || task.session_id != creation.record.session_id
            || task.created_at != creation.record.created_at
            || task.idempotency_key != creation.record.idempotency_key
        {
            return Err(TaskStoreError::Invalid("Task creation identity"));
        }
        let revision = task.specification.revision;
        let last_event = if revision == 1 {
            if task != creation.record {
                return Err(TaskStoreError::Invalid("Task initial head"));
            }
            None
        } else {
            let last = reader.event(revision).await?;
            if last.record != task {
                return Err(TaskStoreError::Invalid("Task final head"));
            }
            transition(&reader.prior(revision - 1, &creation).await?, &last)?;
            reader.event_comment(&last).await?;
            Some(last)
        };
        let history = if let Some(page) = &input.history {
            let (after, _) = history_after.unwrap();
            if after > revision || (page.cursor.is_some() && after >= revision && after != 1) {
                return Err(TaskStoreError::InvalidCursor);
            }
            let take = page
                .page_size
                .get()
                .checked_add(1)
                .ok_or(TaskStoreError::Invalid("Task detail page size"))?;
            let found = reader
                .rows(
                    &format!(
                        "kind = 'task_event' AND revision > {after} AND revision <= {revision}"
                    ),
                    take,
                )
                .await?;
            let more = found.len() > page.page_size.get();
            let mut events = Vec::new();
            let mut prior = reader.prior(after, &creation).await?;
            let page_before = prior.clone();
            for (index, row) in found.into_iter().enumerate() {
                let rows::Row::TaskEvent(event) = row else {
                    return Err(TaskStoreError::Invalid("history page row"));
                };
                transition(&prior, &event)?;
                if index == page.page_size.get() {
                    break;
                }
                reader.event_comment(&event).await?;
                prior = event.record.clone();
                events.push(event);
            }
            if events.is_empty() && after != revision {
                return Err(TaskStoreError::Invalid("missing requested Task history"));
            }
            if !more && prior.specification.revision != revision {
                return Err(TaskStoreError::Invalid("incomplete requested Task history"));
            }
            let next_cursor = more.then(|| {
                cursor(
                    &binding,
                    DetailSection::History,
                    prior.specification.revision,
                    0,
                )
            });
            Some(TaskHistoryPage {
                before: page_before,
                events,
                next_cursor,
            })
        } else {
            None
        };
        let comments = if let Some(page) = &input.comments {
            let (after, mut sequence) = comments_after.unwrap();
            let mut at = task.created_at;
            let mut ids = HashSet::new();
            let mut keys = HashSet::new();
            if after > revision {
                return Err(TaskStoreError::InvalidCursor);
            }
            if page.cursor.is_some() && sequence > 0 {
                let previous = reader.comment(sequence).await?;
                if sequence == 0 || previous.revision != after {
                    return Err(TaskStoreError::InvalidCursor);
                }
                comments::validate_binding(&reader.event(after).await?, &previous)?;
                if previous.task_id != input.id
                    || previous.session_id != task.session_id
                    || previous.at < task.created_at
                    || previous.at > task.updated_at
                {
                    return Err(TaskStoreError::Invalid("comment predecessor identity"));
                }
                at = previous.at;
                ids.insert(previous.id);
                keys.insert(previous.idempotency_key.clone());
            }
            let take = page
                .page_size
                .get()
                .checked_add(1)
                .ok_or(TaskStoreError::Invalid("Task detail page size"))?;
            let found = reader
                .rows(
                    &format!(
                        "kind = 'task_comment' AND revision > {after} AND revision <= {revision}"
                    ),
                    take,
                )
                .await?;
            let more = found.len() > page.page_size.get();
            let mut comments = Vec::new();
            let mut end = after;
            for (index, row) in found.into_iter().enumerate() {
                let rows::Row::TaskComment(comment) = row else {
                    return Err(TaskStoreError::Invalid("comments page row"));
                };
                sequence = sequence
                    .checked_add(1)
                    .ok_or(TaskStoreError::Invalid("comment sequence overflow"))?;
                if comment.sequence.get() != sequence
                    || comment.revision <= end
                    || comment.at < at
                    || comment.at > task.updated_at
                    || comment.task_id != input.id
                    || comment.session_id != task.session_id
                    || !ids.insert(comment.id)
                    || !keys.insert(comment.idempotency_key.clone())
                {
                    return Err(TaskStoreError::Invalid(
                        "selected comment sequence or identity",
                    ));
                }
                if index == page.page_size.get() {
                    sequence -= 1;
                    break;
                }
                comments::validate_binding(&reader.event(comment.revision).await?, &comment)?;
                end = comment.revision;
                at = comment.at;
                comments.push(comment.record);
            }
            let next_cursor =
                more.then(|| cursor(&binding, DetailSection::Comments, end, sequence));
            Some(TaskCommentsPage {
                comments,
                next_cursor,
            })
        } else {
            None
        };
        before()?;
        Ok(Some(TaskGetRead {
            task,
            creation,
            last_event,
            history_start_cursor: cursor(&binding, DetailSection::History, 1, 0),
            comments_start_cursor: cursor(&binding, DetailSection::Comments, 1, 0),
            generation: binding.generation,
            history,
            comments,
        }))
    }
}
