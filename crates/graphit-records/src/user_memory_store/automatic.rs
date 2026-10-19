use super::{
    native::{self, UserWritePolicy},
    rows, store, *,
};
use crate::{
    memory_store::{automatic as shared, revision, rows as fields, *},
    reference_projection::LogicalScope,
};
use futures::TryStreamExt;
use graphit_auth::{UserReadAdmission, UserWriteAdmission};
use graphit_core::file_io::SnapshotContext;
use graphit_data::{lance_branch_store::BranchSnapshot, lance_module_store::StoreError};
use std::{collections::HashSet, sync::Arc};

fn check_pair(
    original: &Arc<SnapshotContext>,
    read: &UserReadAdmission<'_>,
    write: &UserWriteAdmission<'_>,
) -> Result<(), MemoryStoreError> {
    native::check_read(original, read)?;
    UserWritePolicy::Fenced.check(original, write)?;
    let r = read.user()?.live;
    let w = write.user()?.live;
    if native::owner(r)? != native::owner(w)? || r.runtime_root() != w.runtime_root() {
        return Err(MemoryStoreError::WrongAdmission);
    }
    Ok(())
}
fn effect(event: &UserMemoryRevision, version: Option<u64>) -> AutomaticMemoryEffect {
    shared::effect_parts(&event.before, &event.after, &event.operation, version)
}
async fn observe_fold(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    owner: &UserMemoryOwner,
    actor: &str,
    input: &FoldMemories,
    limits: MemoryStoreWriteLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<MemoryFoldReceipt>, MemoryStoreError> {
    let anchor = store::lookup_attempt(
        pin,
        original,
        owner,
        actor,
        &input.survivor.idempotency_key,
        limits,
        &before,
    )
    .await?;
    let Some(anchor) = anchor else {
        for member in &input.members {
            if store::lookup_attempt(
                pin,
                original,
                owner,
                actor,
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
    let observations = shared::fold_observations(&anchor.operation, input)?;
    let mut removed = Vec::new();
    for (member, (observed_id, observed_revision, observed_attributes)) in
        input.members.iter().zip(observations)
    {
        before()?;
        let event = store::lookup_attempt(
            pin,
            original,
            owner,
            actor,
            &member.idempotency_key,
            limits,
            &before,
        )
        .await?
        .ok_or(MemoryStoreError::IncompleteCorpus)?;
        if observed_id != member.id
            || event.after.id != member.id
            || event.operation != shared::delete_operation(input, member)?
            || event.before.revision != observed_revision
            || event.before.attributes != observed_attributes
            || event.after.updated_at != anchor.after.updated_at
        {
            return Err(MemoryStoreError::Invalid("USER fold group disagrees"));
        }
        removed.push(effect(&event, None));
    }
    let receipt = MemoryFoldReceipt {
        survivor: effect(&anchor, None),
        removed,
    };
    before()?;
    Ok(Some(receipt))
}
async fn live_chain(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    owner: &UserMemoryOwner,
    member: &MemoryFoldMember,
    limits: MemoryStoreReadLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<store::Chain, MemoryStoreError> {
    let chain = store::chain(pin, original, owner, member.id, limits, &before)
        .await?
        .filter(|c| !c.deleted)
        .ok_or(MemoryStoreError::NotFound)?;
    if member.expected_revision != chain.current.revision {
        return Err(MemoryStoreError::RevisionConflict {
            expected: member.expected_revision,
            actual: chain.current.revision,
        });
    }
    before()?;
    Ok(chain)
}
fn event(
    original: &Arc<SnapshotContext>,
    owner: &UserMemoryOwner,
    actor: &str,
    at: u64,
    before: &Memory,
    operation: MemoryMutation,
) -> Result<UserMemoryRevision, MemoryStoreError> {
    let sid = UserMemoryScopeId::from_owner(owner)?;
    let after = revision::derive_in_scope(
        original,
        LogicalScope::User(sid.as_str()),
        before,
        &operation,
        actor,
        at,
    )?;
    Ok(UserMemoryRevision {
        owner: owner.clone(),
        actor: actor.to_owned(),
        operation,
        before: before.clone(),
        after,
    })
}
async fn commit(
    capture: &native::WritePin,
    original: &Arc<SnapshotContext>,
    read: &UserReadAdmission<'_>,
    write: &UserWriteAdmission<'_>,
    batches: Vec<arrow_array::RecordBatch>,
) -> Result<UserMemoryPublication, MemoryStoreError> {
    let before = || {
        check_pair(original, read, write)?;
        capture.check(original, write)
    };
    before()?;
    let pending = capture
        .pin
        .prepare_merge(batches, vec!["row_key".into()])
        .await?;
    before()?;
    store::publish(pending, &capture.owner, before).await
}
fn check_confirmed(
    publication: &UserMemoryPublication,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<(), MemoryStoreError> {
    before().map_err(|cause| MemoryStoreError::UserConfirmed {
        publication: Box::new(publication.clone()),
        cause: Box::new(cause),
    })
}
impl NativeUserMemoryStore {
    pub async fn automatic_fold(
        original: &Arc<SnapshotContext>,
        read: &UserReadAdmission<'_>,
        write: &UserWriteAdmission<'_>,
        input: FoldMemories,
        limits: MemoryStoreWriteLimits,
    ) -> Result<MemoryFoldReceipt, MemoryStoreError> {
        check_pair(original, read, write)?;
        shared::validate_input(original, &input, limits.max_input_bytes.get())?;
        let capture = native::acquire_with_check(
            original,
            write,
            limits,
            false,
            UserWritePolicy::Fenced,
            || check_pair(original, read, write),
        )
        .await?
        .ok_or(MemoryStoreError::NotFound)?;
        let before = || {
            check_pair(original, read, write)?;
            capture.check(original, write)
        };
        let actor = native::actor(write)?;
        if let Some(receipt) = observe_fold(
            &capture.pin,
            original,
            &capture.owner,
            &actor,
            &input,
            limits,
            &before,
        )
        .await?
        {
            return Ok(receipt);
        }
        let survivor = live_chain(
            &capture.pin,
            original,
            &capture.owner,
            &input.survivor,
            limits.read,
            &before,
        )
        .await?;
        let at = store::now()?;
        let mut observations = vec![(
            survivor.current.id,
            survivor.current.revision,
            survivor.current.attributes,
        )];
        let mut batches = Vec::new();
        let mut removed = Vec::new();
        for member in &input.members {
            before()?;
            let chain = live_chain(
                &capture.pin,
                original,
                &capture.owner,
                member,
                limits.read,
                &before,
            )
            .await?;
            observations.push((
                chain.current.id,
                chain.current.revision,
                chain.current.attributes,
            ));
            let deletion = event(
                original,
                &capture.owner,
                &actor,
                at,
                &chain.current,
                shared::delete_operation(&input, member)?,
            )?;
            batches.push(rows::mutation_batch(&chain.creation, &deletion, limits)?);
            removed.push(effect(&deletion, None));
        }
        let anchor = event(
            original,
            &capture.owner,
            &actor,
            at,
            &survivor.current,
            shared::fold_survivor(input, observations),
        )?;
        batches.insert(
            0,
            rows::mutation_batch(&survivor.creation, &anchor, limits)?,
        );
        before()?;
        let publication = commit(&capture, original, read, write, batches).await?;
        let version = publication.generation.sdk_version;
        for effect in &mut removed {
            effect.sdk_version = Some(version);
        }
        let receipt = MemoryFoldReceipt {
            survivor: effect(&anchor, Some(version)),
            removed,
        };
        check_confirmed(&publication, before)?;
        Ok(receipt)
    }
    pub async fn reobserve_automatic_fold(
        original: &Arc<SnapshotContext>,
        read: &UserReadAdmission<'_>,
        write: &UserWriteAdmission<'_>,
        input: FoldMemories,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Option<MemoryFoldReceipt>, MemoryStoreError> {
        check_pair(original, read, write)?;
        shared::validate_input(original, &input, limits.max_input_bytes.get())?;
        let Some(capture) = native::read_pin_with_check(original, read, limits.read.native, || {
            check_pair(original, read, write)
        })
        .await?
        else {
            check_pair(original, read, write)?;
            return Ok(None);
        };
        let before = || {
            check_pair(original, read, write)?;
            capture.guard.revalidate(original)?;
            Ok(())
        };
        observe_fold(
            &capture.pin,
            original,
            &capture.owner,
            &native::actor(write)?,
            &input,
            limits,
            before,
        )
        .await
    }
    pub async fn automatic_delete(
        original: &Arc<SnapshotContext>,
        read: &UserReadAdmission<'_>,
        write: &UserWriteAdmission<'_>,
        id: MemoryId,
        input: DeleteMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<AutomaticDeleteOutcome, MemoryStoreError> {
        check_pair(original, read, write)?;
        let owner = native::owner(write.user()?.live)?;
        let sid = UserMemoryScopeId::from_owner(&owner)?;
        let operation = MemoryMutation::AutomaticDelete(input);
        revision::validate_in_scope(
            original,
            LogicalScope::User(sid.as_str()),
            &operation,
            limits.max_input_bytes.get(),
        )?;
        let capture = native::acquire_with_check(
            original,
            write,
            limits,
            false,
            UserWritePolicy::Fenced,
            || check_pair(original, read, write),
        )
        .await?
        .ok_or(MemoryStoreError::NotFound)?;
        let before = || {
            check_pair(original, read, write)?;
            capture.check(original, write)
        };
        let actor = native::actor(write)?;
        if let Some(found) = store::lookup_attempt(
            &capture.pin,
            original,
            &capture.owner,
            &actor,
            operation.key(),
            limits,
            &before,
        )
        .await?
        {
            if found.after.id != id || found.operation != operation {
                return Err(MemoryStoreError::IdempotencyConflict);
            }
            let outcome = AutomaticDeleteOutcome::Deleted(effect(&found, None));
            before()?;
            return Ok(outcome);
        }
        let member = MemoryFoldMember {
            id,
            expected_revision: operation.expected(),
            idempotency_key: operation.key().to_owned(),
        };
        let chain = live_chain(
            &capture.pin,
            original,
            &capture.owner,
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
        if active_count(
            &capture.pin,
            original,
            &capture.owner,
            id,
            limits.read,
            &before,
        )
        .await?
            <= 1
        {
            before()?;
            return Ok(AutomaticDeleteOutcome::Refused(
                AutomaticDeleteRefusal::LastRecord,
            ));
        }
        let event = event(
            original,
            &capture.owner,
            &actor,
            store::now()?,
            &chain.current,
            operation,
        )?;
        let batch = rows::mutation_batch(&chain.creation, &event, limits)?;
        let publication = commit(&capture, original, read, write, vec![batch]).await?;
        let outcome = AutomaticDeleteOutcome::Deleted(effect(
            &event,
            Some(publication.generation.sdk_version),
        ));
        check_confirmed(&publication, before)?;
        Ok(outcome)
    }
    pub async fn reobserve_automatic_delete(
        original: &Arc<SnapshotContext>,
        read: &UserReadAdmission<'_>,
        write: &UserWriteAdmission<'_>,
        id: MemoryId,
        input: DeleteMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Option<AutomaticMemoryEffect>, MemoryStoreError> {
        check_pair(original, read, write)?;
        let owner = native::owner(write.user()?.live)?;
        let sid = UserMemoryScopeId::from_owner(&owner)?;
        let operation = MemoryMutation::AutomaticDelete(input);
        revision::validate_in_scope(
            original,
            LogicalScope::User(sid.as_str()),
            &operation,
            limits.max_input_bytes.get(),
        )?;
        let Some(capture) = native::read_pin_with_check(original, read, limits.read.native, || {
            check_pair(original, read, write)
        })
        .await?
        else {
            check_pair(original, read, write)?;
            return Ok(None);
        };
        let before = || {
            check_pair(original, read, write)?;
            capture.guard.revalidate(original)?;
            Ok(())
        };
        let found = store::lookup_attempt(
            &capture.pin,
            original,
            &capture.owner,
            &native::actor(write)?,
            operation.key(),
            limits,
            &before,
        )
        .await?;
        before()?;
        match found {
            Some(e) if e.after.id == id && e.operation == operation => {
                let result = Some(effect(&e, None));
                before()?;
                Ok(result)
            }
            Some(_) => Err(MemoryStoreError::IdempotencyConflict),
            None => Ok(None),
        }
    }
}

/// Bounded metadata-only heads from the held writer pin; consume EOF before deciding.
async fn active_count(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    owner: &UserMemoryOwner,
    target: MemoryId,
    limits: MemoryStoreReadLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<usize, MemoryStoreError> {
    before()?;
    let columns = [
        "row_key",
        "kind",
        "record_id",
        "installation_id",
        "user_issuer",
        "user_subject",
        "title",
        "revision",
        "created_at",
        "updated_at",
        "created_by",
        "updated_by",
        "superseded",
        "tombstone",
    ];
    let expected =
        rows::user_memory_schema().project(&[0, 1, 2, 3, 4, 5, 8, 9, 10, 11, 12, 13, 14, 15])?;
    let mut scanner = pin.scan();
    scanner
        .filter("kind = 'head' OR row_key LIKE 'head/%'")
        .map_err(StoreError::from)?
        .project(&columns)
        .map_err(StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize);
    let mut stream = crate::storage::read(original, scanner.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    before()?;
    let mut ids = HashSet::new();
    let mut active = 0;
    let mut target_active = false;
    loop {
        before()?;
        let batch = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = batch else {
            break;
        };
        fields::validate_schema(&batch, &expected)?;
        for row in 0..batch.num_rows() {
            before()?;

            let id: MemoryId = fields::text(&batch, 2, row)?.parse()?;
            if fields::text(&batch, 0, row)? != format!("head/{id}")
                || fields::text(&batch, 1, row)? != "head"
                || fields::text(&batch, 3, row)? != owner.installation_id.as_str()
                || fields::text(&batch, 4, row)? != owner.identity.issuer()
                || fields::text(&batch, 5, row)? != owner.identity.subject()
                || fields::number(&batch, 7, row)? == 0
                || fields::number(&batch, 9, row)? < fields::number(&batch, 8, row)?
                || fields::flag(&batch, 12, row)?
                || !ids.insert(id)
            {
                return Err(MemoryStoreError::Invalid("USER automatic head"));
            }
            nonblank(fields::text(&batch, 6, row)?)?;
            valid_actor(fields::text(&batch, 10, row)?)?;
            valid_actor(fields::text(&batch, 11, row)?)?;
            if !fields::flag(&batch, 13, row)? {
                active += 1;
                if id == target {
                    target_active = true;
                }
            }
        }
    }
    before()?;
    if !target_active {
        return Err(MemoryStoreError::IncompleteCorpus);
    }
    Ok(active)
}

#[cfg(test)]
#[path = "../../tests/user_memory_store/automatic_publication.rs"]
mod publication_tests;
