//! Selected Session reads at one authenticated publication; full audits remain separate.
use super::{session_get_model::*, *};
use futures::TryStreamExt;
use graphit_auth::ProjectReadAdmission;
use graphit_core::{file_io::SnapshotContext, native_path::NativePath};
use graphit_data::{lance_branch_store::BranchSnapshot, lance_module_store::StoreError};
use lance::dataset::scanner::ColumnOrdering;
use std::sync::Arc;

struct Reader<'a, F> {
    pin: &'a BranchSnapshot,
    original: &'a Arc<SnapshotContext>,
    limits: TaskStoreReadLimits,
    before: F,
    id: SessionId,
}
impl<F: Fn() -> Result<(), TaskStoreError>> Reader<'_, F> {
    async fn rows(&self, clause: &str, take: usize) -> Result<Vec<rows::Row>, TaskStoreError> {
        (self.before)()?;
        let count =
            i64::try_from(take).map_err(|_| TaskStoreError::Invalid("Session detail page size"))?;
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
    async fn event(&self, revision: u64) -> Result<LifecycleEvent<Session>, TaskStoreError> {
        let mut found = self
            .rows(
                &format!("kind = 'session_event' AND revision = {revision}"),
                2,
            )
            .await?;
        if found.len() != 1 {
            return Err(TaskStoreError::Invalid(
                "missing or duplicate selected Session event",
            ));
        }
        match found.pop().unwrap() {
            rows::Row::SessionEvent(event) if event.record.revision == revision => Ok(event),
            _ => Err(TaskStoreError::Invalid("selected Session event")),
        }
    }
    async fn prior(
        &self,
        revision: u64,
        creation: &Creation<Session>,
    ) -> Result<Session, TaskStoreError> {
        if revision == 1 {
            Ok(creation.record.clone())
        } else {
            Ok(self.event(revision).await?.record)
        }
    }
}
fn transition(prior: &Session, event: &LifecycleEvent<Session>) -> Result<(), TaskStoreError> {
    let next = &event.record;
    if prior.revision.checked_add(1) != Some(next.revision) || event.observed_at != next.updated_at
    {
        return Err(TaskStoreError::Invalid("selected Session transition"));
    }
    let references = match &event.references_update {
        TaskReferenceUpdate::Preserve => &prior.references,
        TaskReferenceUpdate::Replace(next) => next,
    };
    if references != &next.references {
        return Err(TaskStoreError::Invalid("reference lifecycle history"));
    }
    read::validate_session_transition(prior, event)
}
fn continuation(
    input: &SessionDetailPageInput,
    binding: &SessionDetailBinding,
) -> Result<u64, TaskStoreError> {
    match &input.cursor {
        None => Ok(1),
        Some(cursor)
            if *cursor.binding == *binding
                && cursor.section == SessionDetailSection::History
                && cursor.after_revision >= 1 =>
        {
            Ok(cursor.after_revision)
        }
        _ => Err(TaskStoreError::InvalidCursor),
    }
}
fn cursor(binding: &SessionDetailBinding, after_revision: u64) -> SessionDetailCursor {
    SessionDetailCursor {
        binding: Box::new(binding.clone()),
        section: SessionDetailSection::History,
        after_revision,
    }
}
impl NativeTaskStore {
    /// Read current Session state and only its explicitly selected audit page.
    pub async fn read_session(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        input: SessionGetInput,
        limits: TaskStoreReadLimits,
    ) -> Result<Option<SelectedSessionRead>, TaskStoreError> {
        let Some(capture) = native::read_pin(original, admission, limits).await? else {
            if input
                .history
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
        let mut binding = SessionDetailBinding {
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
            session_id: input.id,
        };
        let selected = input.history.as_ref().and_then(|p| p.cursor.as_ref());
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
        let history_after = input
            .history
            .as_ref()
            .map(|p| continuation(p, &binding))
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
        let mut session = None;
        let mut creation = None;
        // The third selected row diagnoses duplicate current or creation rows.
        for row in reader
            .rows("kind IN ('session','session_creation')", 3)
            .await?
        {
            match row {
                rows::Row::Session(row) if session.is_none() => session = Some(row.record),
                rows::Row::SessionCreation(row) if creation.is_none() => creation = Some(row),
                _ => {
                    return Err(TaskStoreError::Invalid(
                        "duplicate Session head or creation",
                    ));
                }
            }
        }
        let (session, creation) = match (session, creation) {
            (None, None)
                if input
                    .history
                    .as_ref()
                    .and_then(|p| p.cursor.as_ref())
                    .is_none() =>
            {
                return Ok(None);
            }
            (None, None) => return Err(TaskStoreError::InvalidCursor),
            (Some(session), Some(creation)) => (session, creation),
            _ => return Err(TaskStoreError::Invalid("incomplete Session head")),
        };
        if creation.record.id != input.id
            || creation.record.revision != 1
            || creation.references_update.initial() != creation.record.references.as_slice()
            || session.id != input.id
            || session.revision == 0
            || session.created_at != creation.record.created_at
            || session.input.idempotency_key != creation.record.input.idempotency_key
        {
            return Err(TaskStoreError::Invalid("Session creation identity"));
        }
        let revision = session.revision;
        let last_event = if revision == 1 {
            if session != creation.record {
                return Err(TaskStoreError::Invalid("Session initial head"));
            }
            None
        } else {
            let last = reader.event(revision).await?;
            if last.record != session {
                return Err(TaskStoreError::Invalid("Session final head"));
            }
            transition(&reader.prior(revision - 1, &creation).await?, &last)?;
            Some(last)
        };
        let history = if let Some(page) = &input.history {
            let after = history_after.unwrap();
            if after > revision || (page.cursor.is_some() && after >= revision && after != 1) {
                return Err(TaskStoreError::InvalidCursor);
            }
            let take = page
                .page_size
                .get()
                .checked_add(1)
                .ok_or(TaskStoreError::Invalid("Session detail page size"))?;
            let found = reader
                .rows(
                    &format!(
                        "kind = 'session_event' AND revision > {after} AND revision <= {revision}"
                    ),
                    take,
                )
                .await?;
            let more = found.len() > page.page_size.get();
            let mut events = Vec::new();
            let mut prior = reader.prior(after, &creation).await?;
            let page_before = prior.clone();
            for (index, row) in found.into_iter().enumerate() {
                let rows::Row::SessionEvent(event) = row else {
                    return Err(TaskStoreError::Invalid("history page row"));
                };
                transition(&prior, &event)?;
                if index == page.page_size.get() {
                    break;
                }
                prior = event.record.clone();
                events.push(event);
            }
            if events.is_empty() && after != revision {
                return Err(TaskStoreError::Invalid("missing requested Session history"));
            }
            if !more && prior.revision != revision {
                return Err(TaskStoreError::Invalid(
                    "incomplete requested Session history",
                ));
            }
            let next_cursor = more.then(|| cursor(&binding, prior.revision));
            Some(SessionHistoryPage {
                before: page_before,
                events,
                next_cursor,
            })
        } else {
            None
        };
        before()?;
        Ok(Some(SelectedSessionRead {
            session,
            creation,
            last_event,
            history_start_cursor: cursor(&binding, 1),
            generation: binding.generation,
            history,
        }))
    }
}
