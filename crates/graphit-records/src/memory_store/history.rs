use super::{native, read, *};
use graphit_auth::ProjectReadAdmission;
use graphit_core::{file_io::SnapshotContext, project_identity::ProjectId};
use graphit_data::lance_branch_store::BranchSnapshot;
use std::{collections::HashSet, sync::Arc};

pub(crate) struct Chain {
    pub creation: CreationMemory,
    pub current: Memory,
    pub deleted: bool,
    pub revisions: Vec<MemoryRevision>,
}
pub(super) async fn chain(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    id: MemoryId,
    project_id: ProjectId,
    limits: MemoryStoreReadLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<Chain>, MemoryStoreError> {
    let filter = format!("record_id = {}", native::literal(&id.to_string()));
    let found = read::exact_rows(pin, &filter, original, project_id, limits, &before).await?;
    let chain = assemble(found, project_id, || Ok(original.check()?))?;
    before()?;
    Ok(chain)
}

/// Validate already decoded rows without rescanning or resetting input limits.
pub(super) fn assemble(
    found: Vec<super::rows::Row>,
    project_id: ProjectId,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<Chain>, MemoryStoreError> {
    if found.is_empty() {
        before()?;
        return Ok(None);
    }
    let mut head = None;
    let mut creation = None;
    let mut revisions = Vec::new();
    let mut attempts = HashSet::new();
    for row in found {
        before()?;
        if row.project_id != project_id {
            return Err(MemoryStoreError::Invalid("foreign Memory project"));
        }
        if row.kind != "head"
            && !attempts.insert((
                row.operation_version.clone(),
                row.actor.clone(),
                row.key.clone(),
            ))
        {
            return Err(MemoryStoreError::Invalid("duplicate Memory chain attempt"));
        }
        match row.kind.as_str() {
            "head" if head.is_none() => head = Some(row),
            "creation" if creation.is_none() => creation = Some(row),
            "revision" => revisions.push(row.revision.ok_or(MemoryStoreError::IncompleteCorpus)?),
            _ => return Err(MemoryStoreError::Invalid("duplicate Memory chain member")),
        }
    }
    let (head, audit) = head
        .zip(creation)
        .ok_or(MemoryStoreError::IncompleteCorpus)?;
    if head.operation_version != audit.operation_version
        || head.key != audit.key
        || head.actor != audit.actor
    {
        return Err(MemoryStoreError::Invalid(
            "Memory creation binding disagrees",
        ));
    }
    let creation = audit.creation.ok_or(MemoryStoreError::IncompleteCorpus)?;
    let mut current = &creation.memory;
    let mut deleted = false;
    revisions.sort_by_key(|r| r.after.revision);
    for event in &revisions {
        before()?;
        if deleted {
            return Err(MemoryStoreError::Invalid("revision after deleted Memory"));
        }
        if current.revision.checked_add(1) != Some(event.after.revision) {
            return Err(MemoryStoreError::IncompleteCorpus);
        }
        if &event.before != current {
            return Err(MemoryStoreError::Invalid("Memory history fork"));
        }
        current = &event.after;
        deleted = event.operation.deleted();
    }
    if &head.memory != current || head.tombstone != deleted {
        return Err(MemoryStoreError::Invalid(
            "Memory current/history disagrees",
        ));
    }
    before()?;
    Ok(Some(Chain {
        creation,
        current: head.memory,
        deleted,
        revisions,
    }))
}
impl NativeMemoryStore {
    /// Validate the complete immutable chain, including the final deleted state.
    pub async fn history(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        id: MemoryId,
        limits: MemoryStoreReadLimits,
    ) -> Result<Option<MemoryHistory>, MemoryStoreError> {
        let Some(capture) = native::read_pin(original, admission, limits.native).await? else {
            native::check_read(original, admission)?;
            return Ok(None);
        };
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok(())
        };
        let project = admission.project()?;
        let project_id = native::project_id(project.scope.live)?;
        let result = chain(&capture.pin, original, id, project_id, limits, &before)
            .await?
            .map(|c| MemoryHistory {
                creation: c.creation,
                current: c.current,
                deleted: c.deleted,
                revisions: c.revisions,
                generation: native::generation(&capture.pin, project_id, project.version.version()),
            });
        before()?;
        Ok(result)
    }
}
