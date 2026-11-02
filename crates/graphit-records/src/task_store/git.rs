//! The Git coordinator borrows Task's existing writer and physical schema checks.
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
    limits: TaskStoreWriteLimits,
) -> Result<Option<Capture>, GitRecordsError> {
    let writer = match write::acquire_unmaterialized(original, admission, limits).await {
        Ok(writer) => writer,
        Err(TaskStoreError::MissingSession) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    native::check_write(original, admission)?;
    writer.guard.revalidate(original)?;
    Ok(Some(Capture {
        module: GitRecordModule::Task,
        limits: git_records::ReadLimits::Task(limits.read),
        pin: writer.pin,
        guard: Arc::new(writer.guard),
    }))
}

pub(crate) async fn execute(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: TaskStoreWriteLimits,
    mut prepared: Prepared,
) -> Result<GitModuleReport, GitRecordsError> {
    if let Some((report, check_working)) = prepared.noop() {
        native::check_write(original, admission)?;
        if check_working {
            let current = capture(original, admission, limits)
                .await?
                .ok_or(TaskStoreError::MissingSession)?;
            git_records::check_expected(&current.pin, &prepared)?;
            current.guard.revalidate(original)?;
        }
        return Ok(report);
    }
    let mut writer = write::acquire_selected(
        original,
        admission,
        limits,
        prepared.expected.is_none(),
        false,
        native::Selection::resolve(
            admission.record_storage().await?,
            admission
                .project()
                .map_err(TaskStoreError::Auth)?
                .scope
                .live,
            admission
                .project()
                .map_err(TaskStoreError::Auth)?
                .version
                .version(),
            limits.read.native,
            original,
        )?,
    )
    .await?;
    native::check_write(original, admission)?;
    writer.guard.revalidate(original)?;
    let guard = &writer.guard;
    let before = || -> Result<(), GitRecordsError> {
        native::check_write(original, admission)?;
        guard.revalidate(original)?;
        Ok(())
    };
    let mut partial = Vec::new();
    if prepared.expected.is_none() {
        partial.push(GitModuleReport {
            module: GitRecordModule::Task,
            effect: git_records::GitRecordEffect::Created,
            version: writer.pin.version(),
            commit: None,
        });
    }
    git_records::check_expected(&writer.pin, &prepared)?;
    let extension: Result<(), GitRecordsError> = async {
        if prepared.index_before() && writer.pin.inherited_target().is_some() {
            before()?;
            writer.pin = writer.pin.clone().materialize().await?;
            partial.push(GitModuleReport {
                module: GitRecordModule::Task,
                effect: git_records::GitRecordEffect::Branched,
                version: writer.pin.version(),
                commit: None,
            });
            prepared.expected = Some(writer.pin.clone());
        }
        if prepared.needs_vectors()
            && writer
                .pin
                .schema()
                .field_with_name("embedding_vector")
                .is_err()
        {
            git_records::check_expected(&writer.pin, &prepared)?;
            if writer.pin.inherited_target().is_some() {
                before()?;
                writer.pin = writer.pin.clone().materialize().await?;
                partial.push(GitModuleReport {
                    module: GitRecordModule::Task,
                    effect: git_records::GitRecordEffect::Branched,
                    version: writer.pin.version(),
                    commit: None,
                });
            }
            before()?;
            writer.pin = writer
                .pin
                .add_nullable_columns(rows::vector_schema(), original)
                .await?;
            partial.push(GitModuleReport {
                module: GitRecordModule::Task,
                effect: git_records::GitRecordEffect::SchemaExtended,
                version: writer.pin.version(),
                commit: None,
            });
        }
        if prepared.index_before() {
            let before_index = || {
                native::check_write(original, admission)?;
                guard.revalidate(original)?;
                Ok(())
            };
            let index = Box::pin(super::lexical_index::initialize(
                &mut writer.pin,
                original,
                limits.read,
                before_index,
            ))
            .await
            .map_err(|cause| index_error(cause, &[]))?;
            if index.is_some() {
                partial.push(GitModuleReport {
                    module: GitRecordModule::Task,
                    effect: git_records::GitRecordEffect::Indexed,
                    version: writer.pin.version(),
                    commit: None,
                });
            }
        }
        if !partial.is_empty() {
            prepared.expected = Some(writer.pin.clone());
        }
        Ok(())
    }
    .await;
    if let Err(mut error) = extension {
        error.partial.splice(0..0, partial);
        return Err(error);
    }
    let mut result = git_records::perform(original, &mut writer.pin, &prepared, before)
        .await
        .map_err(|mut error| {
            error.partial.splice(0..0, partial.clone());
            error
        })?;
    if prepared.index_after() {
        let before_index = || {
            native::check_write(original, admission)?;
            guard.revalidate(original)?;
            Ok(())
        };
        let mut effects = partial.clone();
        effects.push(result.clone());
        let index = Box::pin(super::lexical_index::initialize(
            &mut writer.pin,
            original,
            limits.read,
            before_index,
        ))
        .await
        .map_err(|cause| index_error(cause, &effects))?;
        if index.is_some() {
            result.version = writer.pin.version();
        }
    }
    if let Err(error) = native::check_write(original, admission).and_then(|_| {
        writer
            .guard
            .revalidate(original)
            .map_err(TaskStoreError::from)
    }) {
        return Err(GitRecordsError {
            cause: git_records::GitRecordsCause::Task(error),
            partial: vec![result],
        });
    }
    Ok(result)
}

fn index_error(cause: TaskStoreError, partial: &[GitModuleReport]) -> GitRecordsError {
    let mut error: GitRecordsError = cause.into();
    error.partial.extend_from_slice(partial);
    if let git_records::GitRecordsCause::Task(TaskStoreError::LexicalIndexConfirmed {
        publication,
        ..
    }) = &error.cause
    {
        error.partial.push(GitModuleReport {
            module: GitRecordModule::Task,
            effect: git_records::GitRecordEffect::Indexed,
            version: publication.generation.version,
            commit: None,
        });
    }
    error
}

pub(crate) fn validate_row(
    original: &SnapshotContext,
    batch: &arrow_array::RecordBatch,
    row: usize,
) -> Result<(String, Option<git_records::VectorRef>), GitRecordsError> {
    rows::validate_schema(batch.schema().as_ref())?;
    let value = rows::decode(batch, row, original)?;
    let session = matches!(
        &value,
        rows::Row::Session(_) | rows::Row::SessionEvent(_) | rows::Row::SessionCreation(_)
    ) || matches!(&value, rows::Row::ReferenceProjection(p) if matches!(p.source, references::TaskReferenceRecord::Session(_)));
    let family = format!(
        "{}/{}",
        if session { "session" } else { "task" },
        rows::required(batch, "record_id", row)?
    );
    let vector = match value {
        rows::Row::Embedding(value) => {
            use arrow_array::{Array, Float32Array, ListArray};
            let vectors = batch
                .column_by_name("embedding_vector")
                .and_then(|v| v.as_any().downcast_ref::<ListArray>())
                .ok_or(TaskStoreError::Invalid("Task embedding vector column"))?;
            if vectors.is_null(row) {
                return Err(TaskStoreError::Invalid("Task null embedding vector").into());
            }
            let values = vectors.value(row);
            let values = values
                .as_any()
                .downcast_ref::<Float32Array>()
                .ok_or(TaskStoreError::Invalid("Task embedding vector type"))?;
            if values.len() != value.document.identity().dimensions().get() as usize
                || values.null_count() != 0
                || values.values().iter().any(|v| !v.is_finite())
            {
                return Err(TaskStoreError::Invalid("Task embedding vector values").into());
            }
            Some(git_records::VectorRef::Task(value.document))
        }
        _ => None,
    };
    Ok((family, vector))
}

pub(crate) async fn compatible_vectors(
    original: &Arc<SnapshotContext>,
    target: &graphit_data::lance_branch_store::BranchSnapshot,
    candidates: &std::collections::BTreeMap<String, &git_records::VectorRef>,
    limits: git_records::ReadLimits,
) -> Result<std::collections::BTreeSet<String>, GitRecordsError> {
    let git_records::ReadLimits::Task(limits) = limits else {
        return Err(TaskStoreError::WrongAdmission.into());
    };
    type VectorIdentity = ([u8; 32], u32, String);
    type SourceFingerprints = std::collections::HashMap<String, (String, [u8; 32])>;
    let mut identities: std::collections::BTreeMap<
        VectorIdentity,
        (super::embeddings::TaskEmbeddingIdentity, SourceFingerprints),
    > = std::collections::BTreeMap::new();
    for (key, vector) in candidates {
        let git_records::VectorRef::Task(document) = vector else {
            continue;
        };
        let identity = document.identity();
        let group = identities
            .entry((
                *identity.vector_id(),
                identity.dimensions().get(),
                identity.recipe().into(),
            ))
            .or_insert_with(|| (identity.clone(), std::collections::HashMap::new()));
        group.1.insert(
            document.source_key().into(),
            (key.clone(), *document.fingerprint()),
        );
    }
    let mut result = std::collections::BTreeSet::new();
    for (_, (identity, metadata)) in identities {
        let selected = metadata.keys().cloned().collect();
        embeddings::visit_documents(
            target,
            original,
            &identity,
            limits,
            Some(&selected),
            || Ok(original.check()?),
            |document| {
                if let Some((key, fingerprint)) = metadata.get(document.source_key())
                    && document.fingerprint() == fingerprint
                {
                    result.insert(key.clone());
                }
                Ok(true)
            },
        )
        .await?;
    }
    Ok(result)
}

pub(crate) async fn validate_delta(
    original: &Arc<SnapshotContext>,
    ours: &graphit_data::lance_branch_store::BranchSnapshot,
    theirs: &graphit_data::lance_branch_store::BranchSnapshot,
    deleted: &std::collections::BTreeSet<String>,
    source: &std::collections::BTreeSet<String>,
    batch_rows: u32,
) -> Result<(), GitRecordsError> {
    use futures::TryStreamExt;
    use graphit_data::lance_branch_store::{BranchError, col, lit};
    let mut tasks = Vec::new();
    let mut sessions = std::collections::HashSet::new();
    for (pin, keys, incoming) in [(ours, deleted, false), (theirs, source, true)] {
        if incoming && keys.is_empty() {
            continue;
        }
        let mut scan = pin.scan();
        let kind = col("kind").in_list(vec![lit("task"), lit("session")], false);
        let filter = if keys.is_empty() {
            kind
        } else {
            kind.and(col("row_key").in_list(
                keys.iter().map(|key| lit(key.as_str())).collect(),
                !incoming,
            ))
        };
        scan.filter_expr(filter);
        scan.batch_size(batch_rows as usize)
            .fragment_readahead(1)
            .batch_readahead(1);
        let mut stream = crate::storage::read(original, scan.try_into_stream())
            .await?
            .map_err(BranchError::from)?;
        loop {
            let next = crate::storage::read(original, stream.try_next())
                .await?
                .map_err(BranchError::from)?;
            let Some(batch) = next else { break };
            for row in 0..batch.num_rows() {
                original.check()?;
                match rows::decode(&batch, row, original)? {
                    rows::Row::Task(_) => tasks.push(read::project(&batch, row)?),
                    rows::Row::Session(value) => {
                        sessions.insert(value.record.id);
                    }
                    _ => return Err(TaskStoreError::Invalid("Git Task head projection").into()),
                }
            }
        }
    }
    if tasks
        .iter()
        .any(|task| !sessions.contains(&task.summary.session_id))
    {
        return Err(TaskStoreError::MissingSession.into());
    }
    let identities = tasks
        .iter()
        .map(|task| (task.summary.id, task.summary.session_id))
        .collect::<std::collections::HashMap<_, _>>();
    if identities.len() != tasks.len() {
        return Err(TaskStoreError::Invalid("Git duplicate Task identity").into());
    }
    for task in &tasks {
        if task
            .summary
            .parent_id
            .is_some_and(|parent| identities.get(&parent) != Some(&task.summary.session_id))
        {
            return Err(TaskStoreError::Invalid("Git Task parent session").into());
        }
    }
    read::ready(&mut tasks)?;
    original.check()?;
    Ok(())
}
