use super::{native, read, *};
use graphit_auth::ProjectReadAdmission;
use graphit_core::{file_io::SnapshotContext, project_identity::ProjectId};
use graphit_data::lance_branch_store::BranchSnapshot;
use std::{collections::HashSet, sync::Arc};

impl MemoryListFilter {
    pub(crate) fn matches(&self, memory: &Memory) -> bool {
        self.classification
            .is_none_or(|class| memory.attributes.classification == class)
            && self
                .tag
                .as_ref()
                .is_none_or(|tag| memory.tags.contains(tag))
            && self
                .mandatory
                .is_none_or(|value| memory.attributes.mandatory == value)
            && self
                .important
                .is_none_or(|value| memory.attributes.important == value)
            && (!self.exclude_mandatory || !memory.attributes.mandatory)
    }
}
pub(crate) fn category(memory: &Memory) -> u8 {
    if memory.attributes.mandatory {
        0
    } else if memory.attributes.important {
        1
    } else {
        2
    }
}
impl NativeMemoryStore {
    /// Read complete current content from one pin, then filter and order it.
    /// Limits cover every current input row, including rows excluded by filters.
    pub async fn list(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        filter: MemoryListFilter,
        limits: MemoryStoreReadLimits,
    ) -> Result<MemoryListRead, MemoryStoreError> {
        native::check_read(original, admission)?;
        if let Some(tag) = &filter.tag {
            nonblank(tag)?;
        }
        let Some(capture) = native::read_pin(original, admission, limits.native).await? else {
            native::check_read(original, admission)?;
            return Ok(MemoryListRead::NoCorpus);
        };
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, MemoryStoreError>(())
        };
        let project = admission.project()?;
        let project_id = native::project_id(project.scope.live)?;
        let memories =
            collect(&capture.pin, original, project_id, &filter, limits, &before).await?;
        let generation = native::generation(&capture.pin, project_id, project.version.version());
        before()?;
        Ok(MemoryListRead::Published {
            generation,
            memories,
        })
    }
}

pub(super) async fn collect(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    project_id: ProjectId,
    filter: &MemoryListFilter,
    limits: MemoryStoreReadLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Vec<Memory>, MemoryStoreError> {
    let rows = read::exact_rows(
        pin,
        "kind = 'head' AND superseded = false AND tombstone = false",
        original,
        project_id,
        limits,
        &before,
    )
    .await?;
    let mut ids = HashSet::new();
    let mut memories = Vec::new();
    for row in rows {
        original.check()?;
        if row.project_id != project_id {
            return Err(MemoryStoreError::Invalid("foreign Memory project"));
        }
        if !ids.insert(row.memory.id) {
            return Err(MemoryStoreError::Invalid("duplicate Memory head ID"));
        }
        if filter.matches(&row.memory) {
            memories.push(row.memory);
        }
    }
    memories.sort_by(|a, b| {
        category(a)
            .cmp(&category(b))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    before()?;
    Ok(memories)
}
