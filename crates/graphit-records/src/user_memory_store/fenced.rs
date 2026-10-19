use super::{
    native::{self, UserWritePolicy},
    store, *,
};
use crate::{memory_store::*, reference_projection::LogicalScope};
use graphit_auth::UserWriteAdmission;
use graphit_core::file_io::SnapshotContext;
use std::sync::Arc;

impl NativeUserMemoryStore {
    pub async fn fenced_create(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        input: CreateMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        store::create_with_policy(original, admission, input, limits, UserWritePolicy::Fenced).await
    }
    pub async fn reobserve_fenced_create(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        input: CreateMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<MemoryCreateObservation, MemoryStoreError> {
        store::reobserve_create_with_policy(
            original,
            admission,
            input,
            limits,
            UserWritePolicy::Fenced,
        )
        .await
    }
    pub async fn fenced_update(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        id: MemoryId,
        input: FencedUpdateMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        store::mutate_with_policy(
            original,
            admission,
            id,
            FencedMemoryMutation::Update(input).into_mutation(),
            limits,
            UserWritePolicy::Fenced,
        )
        .await
    }
    pub async fn fenced_delete(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        id: MemoryId,
        input: DeleteMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        store::mutate_with_policy(
            original,
            admission,
            id,
            MemoryMutation::Delete(input),
            limits,
            UserWritePolicy::Fenced,
        )
        .await
    }
    pub async fn fenced_promote(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        id: MemoryId,
        input: ChangeMemoryImportance,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        store::mutate_with_policy(
            original,
            admission,
            id,
            MemoryMutation::Promote(input),
            limits,
            UserWritePolicy::Fenced,
        )
        .await
    }
    pub async fn fenced_demote(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        id: MemoryId,
        input: ChangeMemoryImportance,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        store::mutate_with_policy(
            original,
            admission,
            id,
            MemoryMutation::Demote(input),
            limits,
            UserWritePolicy::Fenced,
        )
        .await
    }
    pub async fn reobserve_fenced_mutation(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        id: MemoryId,
        input: FencedMemoryMutation,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Option<UserMemoryRevision>, MemoryStoreError> {
        UserWritePolicy::Fenced.check(original, admission)?;
        let operation = input.into_mutation();
        let owner = native::owner(admission.user()?.live)?;
        let sid = UserMemoryScopeId::from_owner(&owner)?;
        crate::memory_store::revision::validate_in_scope(
            original,
            LogicalScope::User(sid.as_str()),
            &operation,
            limits.max_input_bytes.get(),
        )?;
        let Some(capture) =
            native::read_write_pin(original, admission, limits.read, UserWritePolicy::Fenced)
                .await?
        else {
            UserWritePolicy::Fenced.check(original, admission)?;
            return Ok(None);
        };
        let before = || {
            UserWritePolicy::Fenced.check(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok(())
        };
        let event = store::lookup_attempt(
            &capture.pin,
            original,
            &capture.owner,
            &native::actor(admission)?,
            operation.key(),
            limits,
            &before,
        )
        .await?;
        before()?;
        match event {
            Some(event) if event.after.id == id && event.operation == operation => Ok(Some(event)),
            Some(_) => Err(MemoryStoreError::IdempotencyConflict),
            None => Ok(None),
        }
    }
}
