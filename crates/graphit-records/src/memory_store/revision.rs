use super::{automatic, history, native, read, rows, write, *};
use crate::reference_projection::LogicalScope;
use graphit_auth::ProjectWriteAdmission;
use graphit_core::{file_io::SnapshotContext, project_identity::ProjectId};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn validate(
    original: &SnapshotContext,
    project_id: ProjectId,
    operation: &MemoryMutation,
    maximum: usize,
) -> Result<(), MemoryStoreError> {
    validate_in_scope(
        original,
        LogicalScope::Project(project_id),
        operation,
        maximum,
    )
}
pub(crate) fn validate_in_scope(
    original: &SnapshotContext,
    scope: LogicalScope<'_>,
    operation: &MemoryMutation,
    maximum: usize,
) -> Result<(), MemoryStoreError> {
    original.check()?;
    if operation.expected() == 0 {
        return Err(MemoryStoreError::Invalid("zero expected revision"));
    }
    let mut bytes = 0;
    for text in [operation.key(), operation.reason()] {
        nonblank(text)?;
        charge(&mut bytes, text.len(), maximum, "mutation input bytes")?;
    }
    charge(&mut bytes, 8, maximum, "mutation input bytes")?;
    match operation {
        MemoryMutation::FoldSurvivor(v) => automatic::validate_survivor(original, v, maximum)?,
        MemoryMutation::FoldDelete(v) => automatic::validate_delete(original, v, maximum)?,
        _ => {}
    }
    if let MemoryMutation::Update(v) = operation {
        for text in v
            .title
            .iter()
            .chain(v.body.iter())
            .chain(v.tags.iter().flatten())
        {
            original.check()?;
            nonblank(text)?;
            charge(&mut bytes, text.len(), maximum, "mutation input bytes")?;
        }
        for present in [
            v.classification.is_some(),
            v.mandatory.is_some(),
            v.important.is_some(),
        ] {
            if present {
                charge(&mut bytes, 1, maximum, "mutation input bytes")?;
            }
        }
        if let Some(refs) = &v.references {
            for r in refs {
                original.check()?;
                let scope = match &r.scope {
                    MemoryReferenceScope::Project(p) => p.to_string(),
                    MemoryReferenceScope::User(u) => u.clone(),
                };
                for text in [model::kind(r.target_kind), &scope, &r.id, &r.relation]
                    .into_iter()
                    .chain(r.context.as_deref())
                    .chain(r.field.as_deref())
                {
                    charge(&mut bytes, text.len(), maximum, "mutation input bytes")?;
                }
            }
            write::validate_references_in_scope(original, scope, refs)?;
        }
    }
    original.check()?;
    Ok(())
}

pub(super) fn derive(
    original: &SnapshotContext,
    project_id: ProjectId,
    before: &Memory,
    operation: &MemoryMutation,
    actor: &str,
    at: u64,
) -> Result<Memory, MemoryStoreError> {
    derive_in_scope(
        original,
        LogicalScope::Project(project_id),
        before,
        operation,
        actor,
        at,
    )
}
pub(crate) fn derive_in_scope(
    original: &SnapshotContext,
    scope: LogicalScope<'_>,
    before: &Memory,
    operation: &MemoryMutation,
    actor: &str,
    at: u64,
) -> Result<Memory, MemoryStoreError> {
    valid_actor(actor)?;
    let mut after = before.clone();
    match operation {
        MemoryMutation::Update(v) => {
            if let Some(value) = &v.title {
                after.title.clone_from(value);
            }
            if let Some(value) = &v.body {
                after.body.clone_from(value);
            }
            if let Some(value) = v.classification {
                after.attributes.classification = value;
            }
            if let Some(value) = v.mandatory {
                after.attributes.mandatory = value;
            }
            if let Some(value) = v.important {
                after.attributes.important = value;
            }
            if let Some(value) = &v.tags {
                after.tags.clone_from(value);
            }
            if let Some(value) = &v.references {
                after.references = write::validate_references_in_scope(original, scope, value)?;
            }
            if after == *before {
                return Err(MemoryStoreError::NoChange);
            }
            if after.references != before.references {
                after.reference_generation = before
                    .reference_generation
                    .checked_add(1)
                    .ok_or(MemoryStoreError::Invalid("reference generation overflow"))?;
            }
        }
        MemoryMutation::Delete(_)
        | MemoryMutation::AutomaticDelete(_)
        | MemoryMutation::FoldDelete(_) => {
            if matches!(operation, MemoryMutation::AutomaticDelete(_))
                && (before.attributes.important || before.attributes.mandatory)
            {
                return Err(MemoryStoreError::Invalid("protected automatic deletion"));
            }
            if let MemoryMutation::FoldDelete(v) = operation
                && before.id != v.member.id
            {
                return Err(MemoryStoreError::Invalid("fold member identity disagrees"));
            }
            after.references.clear();
            after.reference_generation = before
                .reference_generation
                .checked_add(1)
                .ok_or(MemoryStoreError::Invalid("reference generation overflow"))?;
        }
        MemoryMutation::Promote(_) | MemoryMutation::Demote(_) => {
            let important = matches!(operation, MemoryMutation::Promote(_));
            if after.attributes.important == important {
                return Err(MemoryStoreError::NoChange);
            }
            after.attributes.important = important;
        }
        MemoryMutation::FoldSurvivor(v) => {
            after.attributes = automatic::derive_attributes(original, before, v)?;
            after.body.clone_from(&v.input.body);
            if let Some(title) = &v.input.title {
                after.title.clone_from(title);
            }
        }
    }
    if at < before.updated_at {
        return Err(MemoryStoreError::Invalid("backward Memory clock"));
    }
    after.revision = before
        .revision
        .checked_add(1)
        .ok_or(MemoryStoreError::Invalid("Memory revision overflow"))?;
    after.updated_by = actor.to_owned();
    after.updated_at = at;
    original.check()?;
    Ok(after)
}

pub(super) fn validate_event(
    original: &SnapshotContext,
    event: &MemoryRevision,
) -> Result<(), MemoryStoreError> {
    validate(original, event.project_id, &event.operation, usize::MAX)?;
    rows::valid_memory(original, event.project_id, &event.before)?;
    rows::valid_memory(original, event.project_id, &event.after)?;
    if event.operation.expected() != event.before.revision || event.after.revision < 2 {
        return Err(MemoryStoreError::Invalid("revision precondition disagrees"));
    }
    let expected = derive(
        original,
        event.project_id,
        &event.before,
        &event.operation,
        &event.actor,
        event.after.updated_at,
    )?;
    if expected != event.after {
        return Err(MemoryStoreError::Invalid("revision outcome disagrees"));
    }
    Ok(())
}

pub(super) async fn lookup(
    capture: &write::WritePin,
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    id: MemoryId,
    operation: &MemoryMutation,
    limits: MemoryStoreWriteLimits,
) -> Result<Option<MemoryRevision>, MemoryStoreError> {
    let before = || capture.check(original, admission);
    let event = lookup_attempt(
        &capture.pin,
        original,
        admission,
        operation.key(),
        limits,
        before,
    )
    .await?;
    if let Some(event) = &event
        && (event.after.id != id || event.operation != *operation)
    {
        return Err(MemoryStoreError::IdempotencyConflict);
    }
    Ok(event)
}
/// Validate the exact stored event before a caller compares authored input or needs live heads.
pub(super) async fn lookup_attempt(
    pin: &graphit_data::lance_branch_store::BranchSnapshot,
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    key: &str,
    limits: MemoryStoreWriteLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<MemoryRevision>, MemoryStoreError> {
    let project = admission.project()?;
    let project_id = native::project_id(project.scope.live)?;
    let actor = write::actor(admission)?;
    let filter = write::attempt_filter(project_id, project.version.version(), &actor, key);
    let members =
        read::exact_rows(pin, &filter, original, project_id, limits.read, &before).await?;
    if members.is_empty() {
        before()?;
        return Ok(None);
    }
    let row = &members[0];
    let chain = history::chain(
        pin,
        original,
        row.memory.id,
        project_id,
        limits.read,
        &before,
    )
    .await?
    .ok_or(MemoryStoreError::IncompleteCorpus)?;
    if members.len() == 2
        && members
            .iter()
            .all(|r| r.kind == "head" || r.kind == "creation")
    {
        return Err(MemoryStoreError::IdempotencyConflict);
    }
    if members.len() != 1 {
        return Err(MemoryStoreError::Invalid("duplicate mutation attempt"));
    }
    let event = row
        .revision
        .as_ref()
        .ok_or(MemoryStoreError::IdempotencyConflict)?;
    if !chain.revisions.contains(event) {
        return Err(MemoryStoreError::Invalid("mutation observation disagrees"));
    }
    before()?;
    Ok(Some(event.clone()))
}

impl NativeMemoryStore {
    pub async fn update(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: MemoryId,
        input: UpdateMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        mutate(
            original,
            admission,
            id,
            MemoryMutation::Update(input),
            limits,
            native::WritePolicy::Ordinary,
        )
        .await
    }
    pub async fn delete(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: MemoryId,
        input: DeleteMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        mutate(
            original,
            admission,
            id,
            MemoryMutation::Delete(input),
            limits,
            native::WritePolicy::Ordinary,
        )
        .await
    }
    pub async fn reobserve_mutation(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: MemoryId,
        operation: MemoryMutation,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Option<MemoryRevision>, MemoryStoreError> {
        Self::reobserve_mutation_with_policy(
            original,
            admission,
            id,
            operation,
            limits,
            native::WritePolicy::Ordinary,
        )
        .await
    }
    pub(super) async fn reobserve_mutation_with_policy(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: MemoryId,
        operation: MemoryMutation,
        limits: MemoryStoreWriteLimits,
        policy: native::WritePolicy,
    ) -> Result<Option<MemoryRevision>, MemoryStoreError> {
        policy.check(original, admission)?;
        validate(
            original,
            native::project_id(admission.project()?.scope.live)?,
            &operation,
            limits.max_input_bytes.get(),
        )?;
        let Some(capture) =
            write::acquire_with_policy(original, admission, limits, false, policy).await?
        else {
            policy.check(original, admission)?;
            return Ok(None);
        };
        lookup(&capture, original, admission, id, &operation, limits).await
    }
}
pub(super) async fn mutate(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    id: MemoryId,
    operation: MemoryMutation,
    limits: MemoryStoreWriteLimits,
    policy: native::WritePolicy,
) -> Result<Memory, MemoryStoreError> {
    policy.check(original, admission)?;
    let project = admission.project()?;
    let project_id = native::project_id(project.scope.live)?;
    validate(
        original,
        project_id,
        &operation,
        limits.max_input_bytes.get(),
    )?;
    let Some(mut capture) =
        write::acquire_with_policy(original, admission, limits, false, policy).await?
    else {
        policy.check(original, admission)?;
        return Err(MemoryStoreError::NotFound);
    };
    if let Some(event) = lookup(&capture, original, admission, id, &operation, limits).await? {
        return Ok(event.after);
    }
    let before = || {
        capture.check(original, admission)?;
        Ok(())
    };
    let chain = history::chain(&capture.pin, original, id, project_id, limits.read, &before)
        .await?
        .filter(|v| !v.deleted)
        .ok_or(MemoryStoreError::NotFound)?;
    if operation.expected() != chain.current.revision {
        return Err(MemoryStoreError::RevisionConflict {
            expected: operation.expected(),
            actual: chain.current.revision,
        });
    }
    if matches!(&operation, MemoryMutation::Promote(_)) && chain.current.attributes.important
        || matches!(&operation, MemoryMutation::Demote(_)) && !chain.current.attributes.important
    {
        before()?;
        return Ok(chain.current);
    }
    let at = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| MemoryStoreError::Invalid("clock precedes epoch"))?
            .as_millis(),
    )
    .map_err(|_| MemoryStoreError::Invalid("timestamp overflow"))?;
    let actor = write::actor(admission)?;
    let after = derive(original, project_id, &chain.current, &operation, &actor, at)?;
    let event = MemoryRevision {
        project_id,
        operation_version: project.version.version().to_owned(),
        actor,
        operation,
        before: chain.current,
        after: after.clone(),
    };
    let batch = rows::mutation_batch(&chain.creation, &event, limits)?;
    before()?;
    capture.pin = capture.pin.clone().materialize().await?;
    let index = super::lexical_index::initialize(&mut capture, original, admission, limits).await?;
    let result = async {
        let before = || capture.check(original, admission);
        before()?;
        let pending = capture
            .pin
            .prepare_merge(vec![batch], vec!["row_key".into()])
            .await?;
        write::publish(pending, before).await
    }
    .await;
    super::lexical_index::preserve_publication(result, index)?;
    Ok(after)
}
