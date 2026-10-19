use super::{history, model::FoldAttributeObservation, native, revision, rows, write, *};
use crate::memory_attributes::{MemoryAttributes, merge_attributes};
use futures::TryStreamExt;
use graphit_auth::{ProjectReadAdmission, ProjectWriteAdmission};
use graphit_core::{file_io::SnapshotContext, project_identity::ProjectId};
use graphit_data::{lance_branch_store::BranchSnapshot, lance_module_store::StoreError};
use std::{
    collections::HashSet,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

fn check_pair(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
) -> Result<(), MemoryStoreError> {
    native::check_read(original, read)?;
    native::WritePolicy::Fenced.check(original, write)?;
    let r = read.project()?;
    let w = write.project()?;
    if native::project_id(r.scope.live)? != native::project_id(w.scope.live)?
        || r.version.version() != w.version.version()
        || r.scope.live.runtime_root() != w.scope.live.runtime_root()
    {
        return Err(MemoryStoreError::WrongAdmission);
    }
    Ok(())
}

pub(crate) fn validate_input(
    original: &SnapshotContext,
    input: &FoldMemories,
    maximum: usize,
) -> Result<(), MemoryStoreError> {
    original.check()?;
    if input.members.is_empty() {
        return Err(MemoryStoreError::Invalid("empty fold members"));
    }
    let mut bytes = 0;
    for text in [&input.body, &input.reason]
        .into_iter()
        .chain(input.title.iter())
    {
        nonblank(text)?;
        charge(&mut bytes, text.len(), maximum, "fold input bytes")?;
    }
    let mut ids = HashSet::new();
    let mut keys = HashSet::new();
    for member in std::iter::once(&input.survivor).chain(&input.members) {
        original.check()?;
        if member.expected_revision == 0 {
            return Err(MemoryStoreError::Invalid("zero fold revision"));
        }
        nonblank(&member.idempotency_key)?;
        charge(
            &mut bytes,
            member.idempotency_key.len(),
            maximum,
            "fold input bytes",
        )?;
        charge(&mut bytes, 34, maximum, "fold input bytes")?;
        if !ids.insert(member.id) || !keys.insert(&member.idempotency_key) {
            return Err(MemoryStoreError::Invalid("duplicate fold member or key"));
        }
    }
    original.check()?;
    Ok(())
}
pub(super) fn validate_survivor(
    original: &SnapshotContext,
    value: &MemoryFoldSurvivor,
    maximum: usize,
) -> Result<(), MemoryStoreError> {
    validate_input(original, &value.input, maximum)?;
    validate_observations(original, value)
}
fn validate_observations(
    original: &SnapshotContext,
    value: &MemoryFoldSurvivor,
) -> Result<(), MemoryStoreError> {
    if value.observations.len()
        != value
            .input
            .members
            .len()
            .checked_add(1)
            .ok_or(MemoryStoreError::Invalid("fold size overflow"))?
    {
        return Err(MemoryStoreError::Invalid("fold observations incomplete"));
    }
    for (member, observed) in std::iter::once(&value.input.survivor)
        .chain(&value.input.members)
        .zip(&value.observations)
    {
        original.check()?;
        if member.id != observed.id || member.expected_revision != observed.revision {
            return Err(MemoryStoreError::Invalid("fold observation disagrees"));
        }
    }
    Ok(())
}
pub(super) fn validate_delete(
    original: &SnapshotContext,
    value: &MemoryFoldDelete,
    maximum: usize,
) -> Result<(), MemoryStoreError> {
    original.check()?;
    nonblank(&value.survivor_key)?;
    if value.member.id == value.survivor
        || value.member.idempotency_key == value.survivor_key
        || value.survivor_revision < 2
    {
        return Err(MemoryStoreError::Invalid("fold deletion anchor"));
    }
    let mut bytes = 0;
    charge(
        &mut bytes,
        value.survivor_key.len(),
        maximum,
        "fold deletion bytes",
    )?;
    charge(&mut bytes, 60, maximum, "fold deletion bytes")?;
    Ok(())
}
pub(super) fn derive_attributes(
    original: &SnapshotContext,
    before: &Memory,
    value: &MemoryFoldSurvivor,
) -> Result<MemoryAttributes, MemoryStoreError> {
    validate_observations(original, value)?;
    let observed = &value.observations[0];
    if observed.id != before.id
        || observed.revision != before.revision
        || observed.attributes != before.attributes
    {
        return Err(MemoryStoreError::Invalid(
            "fold survivor observation disagrees",
        ));
    }
    let mut attributes = observed.attributes;
    for member in &value.observations[1..] {
        original.check()?;
        attributes = match merge_attributes(&[attributes, member.attributes], original) {
            Ok(attributes) => attributes,
            Err(_) => {
                original.check()?;
                return Err(MemoryStoreError::Invalid("fold attributes"));
            }
        };
    }
    original.check()?;
    Ok(attributes)
}
pub(crate) fn effect_parts(
    before: &Memory,
    after: &Memory,
    operation: &MemoryMutation,
    sdk_version: Option<u64>,
) -> AutomaticMemoryEffect {
    AutomaticMemoryEffect {
        memory_id: after.id,
        before_revision: before.revision,
        after_revision: after.revision,
        idempotency_key: operation.key().to_owned(),
        sdk_version,
    }
}
fn effect(event: &MemoryRevision, sdk_version: Option<u64>) -> AutomaticMemoryEffect {
    effect_parts(&event.before, &event.after, &event.operation, sdk_version)
}
pub(crate) fn fold_survivor(
    input: FoldMemories,
    observations: Vec<(MemoryId, u64, MemoryAttributes)>,
) -> MemoryMutation {
    MemoryMutation::FoldSurvivor(MemoryFoldSurvivor {
        input,
        observations: observations
            .into_iter()
            .map(|(id, revision, attributes)| FoldAttributeObservation {
                id,
                revision,
                attributes,
            })
            .collect(),
    })
}
pub(crate) fn fold_observations<'a>(
    operation: &'a MemoryMutation,
    input: &FoldMemories,
) -> Result<impl Iterator<Item = (MemoryId, u64, MemoryAttributes)> + 'a, MemoryStoreError> {
    let MemoryMutation::FoldSurvivor(stored) = operation else {
        return Err(MemoryStoreError::IdempotencyConflict);
    };
    if stored.input != *input {
        return Err(MemoryStoreError::IdempotencyConflict);
    }
    Ok(stored
        .observations
        .iter()
        .skip(1)
        .map(|o| (o.id, o.revision, o.attributes)))
}
pub(crate) fn delete_operation(
    input: &FoldMemories,
    member: &MemoryFoldMember,
) -> Result<MemoryMutation, MemoryStoreError> {
    Ok(MemoryMutation::FoldDelete(MemoryFoldDelete {
        member: member.clone(),
        survivor: input.survivor.id,
        survivor_key: input.survivor.idempotency_key.clone(),
        survivor_revision: input
            .survivor
            .expected_revision
            .checked_add(1)
            .ok_or(MemoryStoreError::Invalid("fold revision overflow"))?,
        reason: input.reason.clone(),
    }))
}
async fn observe_fold(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    input: &FoldMemories,
    limits: MemoryStoreWriteLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<MemoryFoldReceipt>, MemoryStoreError> {
    let anchor = revision::lookup_attempt(
        pin,
        original,
        admission,
        &input.survivor.idempotency_key,
        limits,
        &before,
    )
    .await?;
    let Some(anchor) = anchor else {
        for member in &input.members {
            if revision::lookup_attempt(
                pin,
                original,
                admission,
                &member.idempotency_key,
                limits,
                &before,
            )
            .await?
            .is_some()
            {
                return Err(MemoryStoreError::IdempotencyConflict);
            }
        }
        before()?;
        return Ok(None);
    };
    if anchor.after.id != input.survivor.id {
        return Err(MemoryStoreError::IdempotencyConflict);
    }
    let observations = fold_observations(&anchor.operation, input)?;
    let mut removed = Vec::new();
    for (member, (_, observed_revision, observed_attributes)) in
        input.members.iter().zip(observations)
    {
        let event = revision::lookup_attempt(
            pin,
            original,
            admission,
            &member.idempotency_key,
            limits,
            &before,
        )
        .await?
        .ok_or(MemoryStoreError::IncompleteCorpus)?;
        if event.after.id != member.id
            || event.operation != delete_operation(input, member)?
            || event.before.attributes != observed_attributes
            || event.before.revision != observed_revision
            || event.after.updated_at != anchor.after.updated_at
        {
            return Err(MemoryStoreError::Invalid("fold group disagrees"));
        }
        removed.push(effect(&event, None));
    }
    before()?;
    Ok(Some(MemoryFoldReceipt {
        survivor: effect(&anchor, None),
        removed,
    }))
}
async fn commit(
    capture: &write::WritePin,
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    admission: &ProjectWriteAdmission<'_>,
    batches: Vec<arrow_array::RecordBatch>,
) -> Result<u64, MemoryStoreError> {
    let before = || {
        check_pair(original, read, admission)?;
        capture.check(original, admission)
    };
    before()?;
    let target = capture.pin.clone().materialize().await?;
    before()?;
    let pending = target
        .prepare_merge(batches, vec!["row_key".into()])
        .await?;
    before()?;
    write::publish_version(pending, before).await
}
fn event(
    original: &SnapshotContext,
    project_id: ProjectId,
    version: &str,
    actor: &str,
    at: u64,
    before: &Memory,
    operation: MemoryMutation,
) -> Result<MemoryRevision, MemoryStoreError> {
    let after = revision::derive(original, project_id, before, &operation, actor, at)?;
    Ok(MemoryRevision {
        project_id,
        operation_version: version.to_owned(),
        actor: actor.to_owned(),
        operation,
        before: before.clone(),
        after,
    })
}
fn now() -> Result<u64, MemoryStoreError> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| MemoryStoreError::Invalid("clock precedes epoch"))?
            .as_millis(),
    )
    .map_err(|_| MemoryStoreError::Invalid("timestamp overflow"))
}
async fn live_chain(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    project_id: ProjectId,
    member: &MemoryFoldMember,
    limits: MemoryStoreReadLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<history::Chain, MemoryStoreError> {
    let chain = history::chain(pin, original, member.id, project_id, limits, before)
        .await?
        .filter(|c| !c.deleted)
        .ok_or(MemoryStoreError::NotFound)?;
    if member.expected_revision != chain.current.revision {
        return Err(MemoryStoreError::RevisionConflict {
            expected: member.expected_revision,
            actual: chain.current.revision,
        });
    }
    Ok(chain)
}

impl NativeMemoryStore {
    /// Preserve the actual derived survivor and archive all selected removals in one SDK delta.
    pub async fn automatic_fold(
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
        admission: &ProjectWriteAdmission<'_>,
        input: FoldMemories,
        limits: MemoryStoreWriteLimits,
    ) -> Result<MemoryFoldReceipt, MemoryStoreError> {
        check_pair(original, read, admission)?;
        validate_input(original, &input, limits.max_input_bytes.get())?;
        let capture = write::acquire_with_check(
            original,
            admission,
            limits,
            false,
            native::WritePolicy::Fenced,
            || check_pair(original, read, admission),
        )
        .await?
        .ok_or(MemoryStoreError::NotFound)?;
        let before = || {
            check_pair(original, read, admission)?;
            capture.check(original, admission)
        };
        if let Some(receipt) =
            observe_fold(&capture.pin, original, admission, &input, limits, &before).await?
        {
            return Ok(receipt);
        }
        let project = admission.project()?;
        let project_id = native::project_id(project.scope.live)?;
        let survivor = live_chain(
            &capture.pin,
            original,
            project_id,
            &input.survivor,
            limits.read,
            &before,
        )
        .await?;
        let actor = write::actor(admission)?;
        let at = now()?;
        let mut batches = Vec::with_capacity(input.members.len() + 1);
        let mut removed = Vec::with_capacity(input.members.len());
        let mut observations = vec![FoldAttributeObservation {
            id: survivor.current.id,
            revision: survivor.current.revision,
            attributes: survivor.current.attributes,
        }];
        for member in &input.members {
            let chain = live_chain(
                &capture.pin,
                original,
                project_id,
                member,
                limits.read,
                &before,
            )
            .await?;
            observations.push(FoldAttributeObservation {
                id: chain.current.id,
                revision: chain.current.revision,
                attributes: chain.current.attributes,
            });
            let deletion = event(
                original,
                project_id,
                project.version.version(),
                &actor,
                at,
                &chain.current,
                delete_operation(&input, member)?,
            )?;
            batches.push(rows::mutation_batch(&chain.creation, &deletion, limits)?);
            removed.push(effect(&deletion, None));
        }
        let anchor = event(
            original,
            project_id,
            project.version.version(),
            &actor,
            at,
            &survivor.current,
            fold_survivor(
                input.clone(),
                observations
                    .into_iter()
                    .map(|o| (o.id, o.revision, o.attributes))
                    .collect(),
            ),
        )?;
        batches.insert(
            0,
            rows::mutation_batch(&survivor.creation, &anchor, limits)?,
        );
        before()?;
        let version = commit(&capture, original, read, admission, batches).await?;
        for effect in &mut removed {
            effect.sdk_version = Some(version);
        }
        Ok(MemoryFoldReceipt {
            survivor: effect(&anchor, Some(version)),
            removed,
        })
    }
    /// Observe the whole exact group; current live members are unnecessary after a committed fold.
    pub async fn reobserve_automatic_fold(
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
        admission: &ProjectWriteAdmission<'_>,
        input: FoldMemories,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Option<MemoryFoldReceipt>, MemoryStoreError> {
        check_pair(original, read, admission)?;
        validate_input(original, &input, limits.max_input_bytes.get())?;
        let Some(capture) = native::read_pin_with_check(original, read, limits.read.native, || {
            check_pair(original, read, admission)
        })
        .await?
        else {
            check_pair(original, read, admission)?;
            return Ok(None);
        };
        let before = || {
            check_pair(original, read, admission)?;
            capture.guard.revalidate(original)?;
            Ok(())
        };
        observe_fold(&capture.pin, original, admission, &input, limits, before).await
    }
    /// Automatic bare deletion protects both flags and the final active record under the Writer.
    pub async fn automatic_delete(
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
        admission: &ProjectWriteAdmission<'_>,
        id: MemoryId,
        input: DeleteMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<AutomaticDeleteOutcome, MemoryStoreError> {
        check_pair(original, read, admission)?;
        let project = admission.project()?;
        let project_id = native::project_id(project.scope.live)?;
        let operation = MemoryMutation::AutomaticDelete(input);
        revision::validate(
            original,
            project_id,
            &operation,
            limits.max_input_bytes.get(),
        )?;
        let capture = write::acquire_with_check(
            original,
            admission,
            limits,
            false,
            native::WritePolicy::Fenced,
            || check_pair(original, read, admission),
        )
        .await?
        .ok_or(MemoryStoreError::NotFound)?;
        let before = || {
            check_pair(original, read, admission)?;
            capture.check(original, admission)
        };
        if let Some(found) = revision::lookup_attempt(
            &capture.pin,
            original,
            admission,
            operation.key(),
            limits,
            &before,
        )
        .await?
        {
            if found.after.id != id || found.operation != operation {
                return Err(MemoryStoreError::IdempotencyConflict);
            }
            return Ok(AutomaticDeleteOutcome::Deleted(effect(&found, None)));
        }
        let member = MemoryFoldMember {
            id,
            expected_revision: operation.expected(),
            idempotency_key: operation.key().to_owned(),
        };
        let chain = live_chain(
            &capture.pin,
            original,
            project_id,
            &member,
            limits.read,
            &before,
        )
        .await?;
        if chain.current.attributes.important || chain.current.attributes.mandatory {
            before()?;
            return Ok(AutomaticDeleteOutcome::Refused(
                AutomaticDeleteRefusal::Protected,
            ));
        }
        if active_count(&capture.pin, original, project_id, id, limits.read, &before).await? <= 1 {
            before()?;
            return Ok(AutomaticDeleteOutcome::Refused(
                AutomaticDeleteRefusal::LastRecord,
            ));
        }
        let event = event(
            original,
            project_id,
            project.version.version(),
            &write::actor(admission)?,
            now()?,
            &chain.current,
            operation,
        )?;
        let batch = rows::mutation_batch(&chain.creation, &event, limits)?;
        let version = commit(&capture, original, read, admission, vec![batch]).await?;
        Ok(AutomaticDeleteOutcome::Deleted(effect(
            &event,
            Some(version),
        )))
    }
    /// Read-only exact observation, requiring current legitimate read and fenced write admissions.
    pub async fn reobserve_automatic_delete(
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
        admission: &ProjectWriteAdmission<'_>,
        id: MemoryId,
        input: DeleteMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Option<AutomaticMemoryEffect>, MemoryStoreError> {
        check_pair(original, read, admission)?;
        let operation = MemoryMutation::AutomaticDelete(input);
        revision::validate(
            original,
            native::project_id(admission.project()?.scope.live)?,
            &operation,
            limits.max_input_bytes.get(),
        )?;
        let Some(capture) = native::read_pin_with_check(original, read, limits.read.native, || {
            check_pair(original, read, admission)
        })
        .await?
        else {
            check_pair(original, read, admission)?;
            return Ok(None);
        };
        let before = || {
            check_pair(original, read, admission)?;
            capture.guard.revalidate(original)?;
            Ok(())
        };
        let found = revision::lookup_attempt(
            &capture.pin,
            original,
            admission,
            operation.key(),
            limits,
            &before,
        )
        .await?;
        match found {
            Some(event) if event.after.id == id && event.operation == operation => {
                before()?;
                Ok(Some(effect(&event, None)))
            }
            Some(_) => Err(MemoryStoreError::IdempotencyConflict),
            None => {
                before()?;
                Ok(None)
            }
        }
    }
}

/// Metadata only, one immutable pin through EOF; retained identities detect duplicates.
async fn active_count(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    project_id: ProjectId,
    target: MemoryId,
    limits: MemoryStoreReadLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<usize, MemoryStoreError> {
    before()?;
    let columns = [
        "row_key",
        "kind",
        "record_id",
        "project_id",
        "title",
        "revision",
        "created_at",
        "updated_at",
        "created_by",
        "updated_by",
        "superseded",
        "tombstone",
    ];
    let expected = rows::schema().project(&[0, 1, 2, 3, 7, 8, 9, 10, 11, 12, 13, 14])?;
    let mut scanner = pin.scan();
    scanner
        .filter("kind = 'head' AND superseded = false AND tombstone = false")
        .map_err(StoreError::from)?
        .project(&columns)
        .map_err(StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize);
    let mut stream = crate::storage::read(original, scanner.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    before()?;
    let mut ids = HashSet::new();
    loop {
        before()?;
        let batch = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = batch else {
            break;
        };
        rows::validate_schema(&batch, &expected)?;
        for row in 0..batch.num_rows() {
            before()?;

            let id: MemoryId = rows::text(&batch, 2, row)?.parse()?;
            if rows::text(&batch, 0, row)? != format!("head/{id}")
                || rows::text(&batch, 1, row)? != "head"
                || rows::text(&batch, 3, row)? != project_id.to_string()
                || rows::number(&batch, 5, row)? == 0
                || rows::number(&batch, 7, row)? < rows::number(&batch, 6, row)?
                || rows::flag(&batch, 10, row)?
                || rows::flag(&batch, 11, row)?
                || !ids.insert(id)
            {
                return Err(MemoryStoreError::Invalid("automatic active head"));
            }
            nonblank(rows::text(&batch, 4, row)?)?;
            valid_actor(rows::text(&batch, 8, row)?)?;
            valid_actor(rows::text(&batch, 9, row)?)?;
        }
    }
    before()?;
    if !ids.contains(&target) {
        return Err(MemoryStoreError::IncompleteCorpus);
    }
    Ok(ids.len())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    include!("../../tests/memory_store/automatic_boundaries.rs");
}
