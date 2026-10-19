use super::{native, rows, *};
use futures::TryStreamExt;
use graphit_auth::ProjectReadAdmission;
use graphit_core::{file_io::SnapshotContext, project_identity::ProjectId};
use graphit_data::{lance_branch_store::BranchSnapshot, lance_module_store::StoreError};
use std::{collections::HashSet, sync::Arc};

pub(super) async fn exact_rows(
    pin: &BranchSnapshot,
    filter: &str,
    original: &Arc<SnapshotContext>,
    project_id: ProjectId,
    limits: MemoryStoreReadLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Vec<rows::Row>, MemoryStoreError> {
    Ok(scan_rows(pin, filter, original, project_id, limits, before)
        .await?
        .logical)
}

pub(crate) struct DecodedRows<T> {
    pub logical: Vec<T>,
    pub has_rows: bool,
}
type PhysicalRows = DecodedRows<rows::Row>;

pub(super) async fn scan_rows(
    pin: &BranchSnapshot,
    filter: &str,
    original: &Arc<SnapshotContext>,
    project_id: ProjectId,
    limits: MemoryStoreReadLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<PhysicalRows, MemoryStoreError> {
    scan_decoded(
        pin,
        filter,
        rows::schema().as_ref(),
        original,
        limits,
        before,
        |batch, row| rows::decode(batch, row, original, project_id),
    )
    .await
}

/// One full-EOF physical scan, with exact namespace-specific schema and decoder.
#[allow(
    clippy::too_many_arguments,
    reason = "Internal typed decoders share scan bounds without introducing an authority abstraction."
)]
pub(crate) async fn scan_decoded<T>(
    pin: &BranchSnapshot,
    filter: &str,
    expected: &arrow_schema::Schema,
    original: &Arc<SnapshotContext>,
    limits: MemoryStoreReadLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
    decode: impl Fn(&arrow_array::RecordBatch, usize) -> Result<Option<T>, MemoryStoreError>,
) -> Result<DecodedRows<T>, MemoryStoreError> {
    before()?;
    let mut scanner = pin.scan();
    scanner
        .filter(filter)
        .map_err(StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize)
        .fragment_readahead(1)
        .batch_readahead(1);
    let mut stream = crate::storage::read(original, scanner.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    before()?;
    let mut found = Vec::new();
    let mut keys = HashSet::new();
    loop {
        before()?;
        let batch = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = batch else {
            break;
        };
        rows::validate_schema(&batch, expected)?;
        for row in 0..batch.num_rows() {
            original.check()?;

            if !keys.insert(rows::text(&batch, 0, row)?.to_owned()) {
                return Err(MemoryStoreError::Invalid("duplicate Memory row key"));
            }
            if let Some(decoded) = decode(&batch, row)? {
                found.push(decoded);
            }
        }
    }
    before()?;
    Ok(DecodedRows {
        logical: found,
        has_rows: !keys.is_empty(),
    })
}

pub(super) async fn ensure_id_available(
    pin: &BranchSnapshot,
    id: MemoryId,
    original: &Arc<SnapshotContext>,
    project_id: ProjectId,
    limits: MemoryStoreReadLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<(), MemoryStoreError> {
    let filter = format!("record_id = {}", native::literal(&id.to_string()));
    if scan_rows(pin, &filter, original, project_id, limits, before)
        .await?
        .has_rows
    {
        return Err(MemoryStoreError::Invalid("generated Memory ID collision"));
    }
    Ok(())
}
impl NativeMemoryStore {
    /// Validate the complete chain from one pin and return only a live current record.
    pub async fn get(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        id: MemoryId,
        limits: MemoryStoreReadLimits,
    ) -> Result<Option<MemoryDetail>, MemoryStoreError> {
        let Some(capture) = native::read_pin(original, admission, limits.native).await? else {
            native::check_read(original, admission)?;
            return Ok(None);
        };
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, MemoryStoreError>(())
        };
        let project = admission.project()?;
        let project_id = native::project_id(project.scope.live)?;
        let result = super::history::chain(&capture.pin, original, id, project_id, limits, &before)
            .await?
            .filter(|chain| !chain.deleted)
            .map(|chain| MemoryDetail {
                memory: chain.current,
                creation: chain.creation,
                generation: native::generation(&capture.pin, project_id, project.version.version()),
            });
        before()?;
        Ok(result)
    }
    /// Return the complete four-column current set, sorted by canonical ID.
    pub async fn observe_heads(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        limits: MemoryHeadReadLimits,
    ) -> Result<MemoryHeadRead, MemoryStoreError> {
        let Some(capture) = native::read_pin(original, admission, limits.native).await? else {
            native::check_read(original, admission)?;
            return Ok(MemoryHeadRead::NoCorpus);
        };
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, MemoryStoreError>(())
        };
        let heads = collect_heads(&capture.pin, original, limits, &before).await?;
        let project = admission.project()?;
        let generation = native::generation(
            &capture.pin,
            native::project_id(project.scope.live)?,
            project.version.version(),
        );
        before()?;
        Ok(MemoryHeadRead::Published { generation, heads })
    }
}

pub(super) async fn collect_heads(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    limits: MemoryHeadReadLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Vec<MemoryActivityHead>, MemoryStoreError> {
    before()?;
    let mut scanner = pin.scan();
    scanner
        .filter("kind = 'head' AND superseded = false AND tombstone = false")
        .map_err(StoreError::from)?
        .project(&["record_id", "title", "updated_by", "updated_at"])
        .map_err(StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize)
        .fragment_readahead(1)
        .batch_readahead(1);
    let mut stream = crate::storage::read(original, scanner.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    before()?;
    let mut heads = Vec::new();
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
        rows::validate_schema(&batch, &rows::head_schema())?;
        for row in 0..batch.num_rows() {
            original.check()?;

            let id: MemoryId = rows::text(&batch, 0, row)?.parse()?;
            if !ids.insert(id) {
                return Err(MemoryStoreError::Invalid("duplicate Memory head ID"));
            }
            heads.push(rows::head(&batch, row)?);
        }
    }
    heads.sort_by_key(|h| h.id);
    before()?;
    Ok(heads)
}
