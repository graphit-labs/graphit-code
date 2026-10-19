//! Explicit public Task index publication and observation of its real SDK key.
use super::{
    NativeTaskStore, TaskStoreError, TaskStoreReadLimits, TaskStoreWriteLimits, model::*, native,
    public_search, write,
};
use graphit_auth::{ProjectWriteAdmission, ProjectWriteOperation};
use graphit_core::file_io::SnapshotContext;
use graphit_data::lance_branch_store::{
    BranchError, BranchSnapshot, DeltaRecoveryKey, DeltaResolution, JsonFtsIndexState,
    PreparedIndex,
};
use std::sync::Arc;

fn generation(pin: &BranchSnapshot) -> TaskGeneration {
    TaskGeneration {
        version: pin.version(),
        physical_branch: pin.physical_branch().map(str::to_owned),
        inherited_target: pin.inherited_target().map(str::to_owned),
    }
}

async fn acquire(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: TaskStoreWriteLimits,
) -> Result<Option<write::WritePin>, TaskStoreError> {
    native::require_operation(admission, &[ProjectWriteOperation::Task])?;
    native::check_write(original, admission)?;
    let project = admission.project()?;
    let selected = native::Selection::resolve(
        admission.record_storage().await?,
        project.scope.live,
        project.version.version(),
        limits.read.native,
        original,
    )?;
    acquire_selected(original, admission, limits, selected).await
}
async fn acquire_selected(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: TaskStoreWriteLimits,
    selected: native::Selection,
) -> Result<Option<write::WritePin>, TaskStoreError> {
    native::check_write(original, admission)?;
    let scope = admission.project()?.scope.live;
    let relative = match &selected {
        native::Selection::Remote(_) => {
            return match write::acquire_selected(
                original, admission, limits, false, false, selected,
            )
            .await
            {
                Ok(pin) => Ok(Some(pin)),
                Err(TaskStoreError::MissingSession) => Ok(None),
                Err(error) => Err(error),
            };
        }
        native::Selection::Local { relative, .. } => relative,
    };
    let before = || native::check_write(original, admission);
    if native::capture(scope, relative, original, limits.read, true, before)?.is_none() {
        if let Some(parent) =
            native::capture(scope, relative, original, limits.read, false, before)?
        {
            before()?;
            parent.revalidate(original)?;
            return Err(TaskStoreError::IncompleteCorpus);
        }
        before()?;
        return Ok(None);
    }
    Ok(Some(
        write::acquire_selected(original, admission, limits, false, false, selected).await?,
    ))
}

fn confirmed(
    publication: TaskLexicalIndexPublication,
    check: Result<(), TaskStoreError>,
) -> Result<TaskLexicalIndexPublication, TaskStoreError> {
    match check {
        Ok(()) => Ok(publication),
        Err(cause) => Err(TaskStoreError::LexicalIndexConfirmed {
            publication: Box::new(publication),
            cause: Box::new(cause),
        }),
    }
}
async fn commit_index(
    prepared: PreparedIndex,
    before: impl Fn() -> Result<(), TaskStoreError>,
) -> Result<(BranchSnapshot, TaskLexicalIndexPublication), TaskStoreError> {
    let recovery_key = prepared.recovery_key().clone();
    before()?;
    let published = prepared.commit().await?;
    let publication = TaskLexicalIndexPublication {
        generation: generation(&published),
        recovery_key,
    };
    let publication = confirmed(publication, before())?;
    Ok((published, publication))
}

/// Initialize derived state under the already-held writer's actual authority.
pub(super) async fn initialize(
    pin: &mut BranchSnapshot,
    original: &Arc<SnapshotContext>,
    limits: TaskStoreReadLimits,
    before: impl Fn() -> Result<(), TaskStoreError>,
) -> Result<Option<TaskLexicalIndexPublication>, TaskStoreError> {
    before()?;
    let state = pin.json_fts_index_state(public_search::COLUMN).await?;
    before()?;
    match state {
        JsonFtsIndexState::Ready => return Ok(None),
        JsonFtsIndexState::Incompatible => {
            return Err(TaskStoreError::Invalid("Task FTS index incompatible"));
        }
        JsonFtsIndexState::Missing => {}
    }
    public_search::scan_validate(pin, original, limits, &before).await?;
    before()?;
    let prepared = pin
        .prepare_json_fts_index(
            public_search::COLUMN,
            public_search::INDEX_NAME,
            graphit_data::lance_branch_store::JsonFtsIndexMode::Create,
        )
        .await?;
    let (published, publication) = commit_index(prepared, before).await?;
    *pin = published;
    Ok(Some(publication))
}

pub(super) fn preserve_publication<T>(
    result: Result<T, TaskStoreError>,
    publication: Option<TaskLexicalIndexPublication>,
) -> Result<T, TaskStoreError> {
    result.map_err(|error| match (publication, error) {
        (Some(publication), TaskStoreError::Confirmed { version, cause }) => {
            TaskStoreError::Confirmed {
                version,
                cause: Box::new(TaskStoreError::LexicalIndexConfirmed {
                    publication: Box::new(publication),
                    cause,
                }),
            }
        }
        (Some(publication), cause) => TaskStoreError::LexicalIndexConfirmed {
            publication: Box::new(publication),
            cause: Box::new(cause),
        },
        (None, error) => error,
    })
}

impl NativeTaskStore {
    /// Publish a new safe JSON index explicitly; an occupied name remains an SDK error.
    pub async fn create_lexical_index(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<TaskLexicalIndexCreation, TaskStoreError> {
        let Some(capture) = acquire(original, admission, limits).await? else {
            return Ok(TaskLexicalIndexCreation::NoCorpus);
        };
        let before = || {
            native::check_write(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, TaskStoreError>(())
        };
        public_search::scan_validate(&capture.pin, original, limits.read, &before).await?;
        before()?;
        let pin = capture.pin.materialize().await?;
        before()?;
        let prepared = pin
            .prepare_json_fts_index(
                public_search::COLUMN,
                public_search::INDEX_NAME,
                graphit_data::lance_branch_store::JsonFtsIndexMode::Create,
            )
            .await?;
        let (_, publication) = commit_index(prepared, before).await?;
        Ok(TaskLexicalIndexCreation::Published(publication))
    }

    /// Observe the actual transaction under fresh write authority, without replay or birth.
    pub async fn resolve_lexical_index(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        key: &DeltaRecoveryKey,
        limits: TaskStoreWriteLimits,
    ) -> Result<DeltaResolution, TaskStoreError> {
        native::require_operation(admission, &[ProjectWriteOperation::Task])?;
        native::check_write(original, admission)?;
        let project = admission.project()?;
        let branch = project.version.lance_branch();
        let physical = (!branch.is_empty()).then_some(branch.as_str());
        let selected = native::Selection::resolve(
            admission.record_storage().await?,
            project.scope.live,
            project.version.version(),
            limits.read.native,
            original,
        )?;
        if key.corpus_uri() != selected.uri() || key.physical_branch() != physical {
            return Err(BranchError::RecoveryBinding.into());
        }
        let Some(capture) = acquire_selected(original, admission, limits, selected).await? else {
            return Ok(DeltaResolution::Unresolved);
        };
        let before = || {
            native::check_write(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, TaskStoreError>(())
        };
        before()?;
        if capture.pin.inherited_target().is_some() {
            return Ok(DeltaResolution::Unresolved);
        }
        let observed = capture.pin.resolve(key).await?;
        before()?;
        Ok(observed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::RecordBatch;
    use graphit_data::{
        lance_branch_store::LanceBranchStore, lance_module_store::LanceModuleStore,
    };
    use std::error::Error;

    async fn pin() -> (tempfile::TempDir, BranchSnapshot) {
        let temp = tempfile::tempdir().unwrap();
        let uri = temp
            .path()
            .join("index.lance")
            .to_string_lossy()
            .into_owned();
        let schema = super::super::rows::schema();
        LanceModuleStore::create(
            &uri,
            Arc::clone(&schema),
            vec![RecordBatch::new_empty(Arc::clone(&schema))],
        )
        .await
        .unwrap();
        let store = LanceBranchStore::open(&uri, schema, None).await.unwrap();
        (temp, store.capture_target(None).await.unwrap())
    }
    #[tokio::test]
    async fn task_lexical_index_strict_conflict_real_keys_and_cause() {
        let (_temp, pin) = pin().await;
        let a = pin
            .prepare_json_fts_index(
                public_search::COLUMN,
                "first",
                graphit_data::lance_branch_store::JsonFtsIndexMode::Create,
            )
            .await
            .unwrap();
        let a_key = a.recovery_key().clone();
        let b = pin
            .prepare_json_fts_index(
                public_search::COLUMN,
                "second",
                graphit_data::lance_branch_store::JsonFtsIndexMode::Create,
            )
            .await
            .unwrap();
        let b_key = b.recovery_key().clone();
        let (_, winner) = commit_index(a, || Ok(())).await.unwrap();
        let loser = commit_index(b, || Ok(()))
            .await
            .err()
            .expect("SDK conflict");
        let TaskStoreError::Commit(cause) = &loser else {
            panic!("actual SDK conflict")
        };
        assert_eq!(cause.recovery_key(), &b_key);
        assert!(cause.source().is_some());
        assert!(loser.source().is_some());
        let combined = preserve_publication::<()>(Err(loser), Some(winner.clone())).unwrap_err();
        let TaskStoreError::LexicalIndexConfirmed { publication, cause } = combined else {
            panic!("index publication stays distinct from the failed write")
        };
        assert_eq!(publication.recovery_key, a_key);
        let TaskStoreError::Commit(cause) = *cause else {
            panic!("actual SDK conflict")
        };
        assert_eq!(cause.recovery_key(), &b_key);
        assert_eq!(winner.recovery_key, a_key);
        assert_eq!(
            pin.resolve(&a_key).await.unwrap(),
            DeltaResolution::Applied {
                version: winner.generation.version
            }
        );
        assert_eq!(
            pin.resolve(&b_key).await.unwrap(),
            DeltaResolution::Conflicted {
                version: winner.generation.version
            }
        );
    }
    #[tokio::test]
    async fn task_lexical_index_known_commit_retains_publication_and_request_cause() {
        let (_temp, pin) = pin().await;
        let prepared = pin
            .prepare_json_fts_index(
                public_search::COLUMN,
                public_search::INDEX_NAME,
                graphit_data::lance_branch_store::JsonFtsIndexMode::Create,
            )
            .await
            .unwrap();
        let (_, publication) = commit_index(prepared, || Ok(())).await.unwrap();
        let key = publication.recovery_key.clone();
        let generation = publication.generation.clone();
        let cancellation = graphit_core::file_lock::Cancellation::default();
        let request = Arc::new(SnapshotContext::new(
            std::time::Instant::now() + std::time::Duration::from_secs(60),
            cancellation.clone(),
        ));
        // A real commit followed by the production delivery boundary. This does not
        // intercept SDK lost acknowledgement or prove atomic Auth/SDK publication.
        cancellation.cancel();
        let error =
            confirmed(publication, request.check().map_err(TaskStoreError::from)).unwrap_err();
        let TaskStoreError::LexicalIndexConfirmed { publication, cause } = &error else {
            panic!("confirmed")
        };
        assert_eq!(publication.generation, generation);
        assert_eq!(publication.recovery_key, key);
        assert!(matches!(cause.as_ref(), TaskStoreError::Request(_)));
        assert!(error.source().is_some());
        assert_eq!(
            pin.resolve(&key).await.unwrap(),
            DeltaResolution::Applied {
                version: generation.version
            }
        );
    }
}
