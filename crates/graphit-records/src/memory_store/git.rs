//! The Git coordinator borrows this module's existing authoritative writer path.
use super::*;
use crate::git_records::{
    self, Capture, GitModuleReport, GitRecordModule, GitRecordsError, Prepared,
};
use graphit_auth::ProjectWriteAdmission;
use graphit_core::file_io::SnapshotContext;
use std::sync::Arc;

pub(crate) async fn capture(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: MemoryStoreWriteLimits,
) -> Result<Option<Capture>, GitRecordsError> {
    let Some(writer) = write::acquire(original, admission, limits, false).await? else {
        return Ok(None);
    };
    writer.check(original, admission)?;
    Ok(Some(Capture {
        module: GitRecordModule::Memory,
        limits: git_records::ReadLimits::Memory(limits.read),
        pin: writer.pin,
        guard: Arc::new(writer.guard),
    }))
}

pub(crate) async fn execute(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: MemoryStoreWriteLimits,
    mut prepared: Prepared,
) -> Result<GitModuleReport, GitRecordsError> {
    if let Some((report, check_working)) = prepared.noop() {
        admission.recheck().map_err(MemoryStoreError::Auth)?;
        original.check()?;
        if check_working {
            let current = capture(original, admission, limits)
                .await?
                .ok_or(MemoryStoreError::IncompleteCorpus)?;
            git_records::check_expected(&current.pin, prepared.expected.as_ref())?;
            current.guard.revalidate(original)?;
        }
        return Ok(report);
    }
    let mut writer = write::acquire(original, admission, limits, prepared.expected.is_none())
        .await?
        .ok_or(MemoryStoreError::IncompleteCorpus)?;
    writer.check(original, admission)?;
    git_records::check_expected(&writer.pin, prepared.expected.as_ref())?;
    let mut partial = Vec::new();
    if prepared.expected.is_none() {
        partial.push(GitModuleReport {
            module: GitRecordModule::Memory,
            effect: git_records::GitRecordEffect::Created,
            version: writer.pin.version(),
            commit: None,
        });
    }
    if prepared.index_before() {
        if writer.pin.inherited_target().is_some() {
            writer.check(original, admission)?;
            writer.pin = writer.pin.clone().materialize().await?;
            partial.push(GitModuleReport {
                module: GitRecordModule::Memory,
                effect: git_records::GitRecordEffect::Branched,
                version: writer.pin.version(),
                commit: None,
            });
            prepared.expected = Some(writer.pin.clone());
        }
        let index = Box::pin(super::lexical_index::initialize(
            &mut writer,
            original,
            admission,
            limits,
        ))
        .await
        .map_err(|cause| index_error(cause, &partial))?;
        if index.is_some() {
            partial.push(GitModuleReport {
                module: GitRecordModule::Memory,
                effect: git_records::GitRecordEffect::Indexed,
                version: writer.pin.version(),
                commit: None,
            });
            prepared.expected = Some(writer.pin.clone());
        }
    }
    let guard = &writer.guard;
    let before = || -> Result<(), GitRecordsError> {
        native::check_write(original, admission)?;
        guard.revalidate(original)?;
        Ok(())
    };
    let mut result = git_records::perform(original, &mut writer.pin, &prepared, before)
        .await
        .map_err(|mut error| {
            error.partial.splice(0..0, partial.clone());
            error
        })?;
    if prepared.index_after() {
        let mut effects = partial.clone();
        effects.push(result.clone());
        let index = Box::pin(super::lexical_index::initialize(
            &mut writer,
            original,
            admission,
            limits,
        ))
        .await
        .map_err(|cause| index_error(cause, &effects))?;
        if index.is_some() {
            result.version = writer.pin.version();
        }
    }
    if let Err(cause) = writer.check(original, admission) {
        return Err(GitRecordsError {
            cause: git_records::GitRecordsCause::Memory(cause),
            partial: vec![result],
        });
    }
    Ok(result)
}

fn index_error(cause: MemoryStoreError, partial: &[GitModuleReport]) -> GitRecordsError {
    let mut error: GitRecordsError = cause.into();
    error.partial.extend_from_slice(partial);
    if let git_records::GitRecordsCause::Memory(MemoryStoreError::IndexConfirmed {
        publication,
        ..
    }) = &error.cause
    {
        error.partial.push(GitModuleReport {
            module: GitRecordModule::Memory,
            effect: git_records::GitRecordEffect::Indexed,
            version: publication.generation.sdk_version,
            commit: None,
        });
    }
    error
}

pub(crate) fn validate_row(
    original: &SnapshotContext,
    project: graphit_core::project_identity::ProjectId,
    batch: &arrow_array::RecordBatch,
    row: usize,
) -> Result<(String, Option<git_records::VectorRef>), GitRecordsError> {
    rows::validate_schema(batch, rows::schema().as_ref())?;
    rows::decode(batch, row, original, project)?;
    if rows::text(batch, 1, row)? == "embedding" {
        let metadata = embeddings::metadata(batch, row, rows::MemoryLayout::Project)?;
        return Ok((
            rows::text(batch, 2, row)?.into(),
            Some(git_records::VectorRef::Memory {
                id: rows::text(batch, 2, row)?.parse()?,
                revision: metadata.revision,
                identity: metadata.identity,
                fingerprint: metadata.fingerprint,
            }),
        ));
    }
    Ok((rows::text(batch, 2, row)?.into(), None))
}

pub(crate) async fn compatible_vectors(
    original: &Arc<SnapshotContext>,
    target: &graphit_data::lance_branch_store::BranchSnapshot,
    candidates: &std::collections::BTreeMap<String, &git_records::VectorRef>,
    limits: git_records::ReadLimits,
) -> Result<std::collections::BTreeSet<String>, GitRecordsError> {
    let git_records::ReadLimits::Memory(limits) = limits else {
        return Err(MemoryStoreError::WrongAdmission.into());
    };
    let mut groups = std::collections::BTreeMap::new();
    for (key, vector) in candidates {
        if let git_records::VectorRef::Memory { id, .. } = vector {
            groups
                .entry(*id)
                .or_insert_with(Vec::new)
                .push((key, *vector));
        }
    }
    let mut result = std::collections::BTreeSet::new();
    if groups.is_empty() {
        return Ok(result);
    }
    use futures::TryStreamExt;
    use graphit_data::lance_branch_store::{BranchError, col, lit};
    let mut scan = target.scan();
    scan.filter_expr(col("kind").not_eq(lit("embedding")).and(
        col("record_id").in_list(groups.keys().map(|id| lit(id.to_string())).collect(), false),
    ));
    scan.order_by(Some(vec![
        lance::dataset::scanner::ColumnOrdering::asc_nulls_last("record_id".into()),
    ]))
    .map_err(BranchError::from)?
    .batch_size(limits.batch_rows.get() as usize)
    .fragment_readahead(1)
    .batch_readahead(1);
    let mut stream = crate::storage::read(original, scan.try_into_stream())
        .await?
        .map_err(BranchError::from)?;
    let mut current = None;
    let mut found = Vec::new();
    let mut consume = |id: MemoryId, found: Vec<rows::Row>| -> Result<(), GitRecordsError> {
        let Some(first) = found.first() else {
            return Ok(());
        };
        let project = first.project_id;
        let Some(chain) = history::assemble(found, project, || Ok(original.check()?))? else {
            return Ok(());
        };
        if let Some(vectors) = groups.get(&id) {
            for (key, vector) in vectors {
                let git_records::VectorRef::Memory {
                    revision,
                    identity,
                    fingerprint,
                    ..
                } = vector
                else {
                    continue;
                };
                if let Some(source) =
                    embeddings::source_at(lexical_search::HistoryView::Project(&chain), *revision)
                    && embeddings::memory_embedding_fingerprint(source, identity)? == *fingerprint
                {
                    result.insert((*key).clone());
                }
            }
        }
        Ok(())
    };
    loop {
        let next = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(BranchError::from)?;
        let Some(batch) = next else { break };
        rows::validate_schema(&batch, rows::schema().as_ref())?;
        for row in 0..batch.num_rows() {
            original.check()?;
            let id: MemoryId = rows::text(&batch, 2, row)?.parse()?;
            if current.is_some_and(|prior| prior != id) {
                consume(current.unwrap(), std::mem::take(&mut found))?;
            }
            current = Some(id);
            let project = rows::text(&batch, 3, row)?
                .parse()
                .map_err(|_| MemoryStoreError::Invalid("Git Memory project"))?;
            if let Some(value) = rows::decode(&batch, row, original, project)? {
                found.push(value);
            }
        }
    }
    if let Some(id) = current {
        consume(id, found)?;
    }
    original.check()?;
    Ok(result)
}
