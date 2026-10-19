//! Derived JSON index initialization and explicit ordinary maintenance.
use super::{history, native, read, write, *};
use graphit_auth::ProjectWriteAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_data::lance_branch_store::{
    BranchError, BranchSnapshot, DeltaRecoveryKey, DeltaResolution, JsonFtsIndexMode,
    JsonFtsIndexState, PreparedIndex,
};
use std::{collections::HashMap, sync::Arc};

const COLUMN: &str = "payload";
const INDEX_NAME: &str = "memory-json-content-v1";

/// Initialize derived state using the writer's actual ordinary or fenced policy.
pub(super) async fn initialize(
    capture: &mut write::WritePin,
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: MemoryStoreWriteLimits,
) -> Result<Option<MemoryLexicalIndexPublication>, MemoryStoreError> {
    let before = || capture.check(original, admission);
    before()?;
    let state = capture.pin.json_fts_index_state(COLUMN).await?;
    before()?;
    match state {
        JsonFtsIndexState::Ready => return Ok(None),
        JsonFtsIndexState::Incompatible => {
            return Err(MemoryStoreError::LexicalIndexUnavailable(state));
        }
        JsonFtsIndexState::Missing => {}
    }
    let project = admission.project()?;
    let project_id = native::project_id(project.scope.live)?;
    validate_corpus(&capture.pin, original, project_id, limits.read, &before).await?;
    let prepared = capture
        .pin
        .prepare_json_fts_index(COLUMN, INDEX_NAME, JsonFtsIndexMode::Create)
        .await?;
    let (pin, publication) =
        commit_index(prepared, project_id, project.version.version(), before).await?;
    capture.pin = pin;
    Ok(Some(publication))
}

async fn validate_corpus(
    pin: &graphit_data::lance_branch_store::BranchSnapshot,
    original: &Arc<SnapshotContext>,
    project_id: graphit_core::project_identity::ProjectId,
    limits: MemoryStoreReadLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<(), MemoryStoreError> {
    let found = read::exact_rows(pin, "true", original, project_id, limits, &before).await?;
    let mut groups = HashMap::new();
    for row in found {
        original.check()?;
        groups
            .entry(row.memory.id)
            .or_insert_with(Vec::new)
            .push(row);
    }
    for group in groups.into_values() {
        history::assemble(group, project_id, || Ok(original.check()?))?
            .ok_or(MemoryStoreError::IncompleteCorpus)?;
    }
    before()
}

/// Keep both observed effects without interpreting an index as a CRUD receipt.
pub(super) fn preserve_publication<T>(
    result: Result<T, MemoryStoreError>,
    publication: Option<MemoryLexicalIndexPublication>,
) -> Result<T, MemoryStoreError> {
    result.map_err(|error| {
        let Some(publication) = publication else {
            return error;
        };
        match error {
            MemoryStoreError::Confirmed { version, cause } => MemoryStoreError::Confirmed {
                version,
                cause: Box::new(MemoryStoreError::IndexConfirmed {
                    publication: Box::new(publication),
                    cause,
                }),
            },
            cause => MemoryStoreError::IndexConfirmed {
                publication: Box::new(publication),
                cause: Box::new(cause),
            },
        }
    })
}

async fn commit_index(
    prepared: PreparedIndex,
    project_id: graphit_core::project_identity::ProjectId,
    project_version: &str,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<(BranchSnapshot, MemoryLexicalIndexPublication), MemoryStoreError> {
    let recovery_key = prepared.recovery_key().clone();
    before()?;
    let published = prepared.commit().await?;
    let publication = MemoryLexicalIndexPublication {
        generation: native::generation(&published, project_id, project_version),
        recovery_key,
    };
    before().map_err(|cause| MemoryStoreError::IndexConfirmed {
        publication: Box::new(publication.clone()),
        cause: Box::new(cause),
    })?;
    Ok((published, publication))
}

impl NativeMemoryStore {
    /// Ensure a compatible index or explicitly refresh its physical segments.
    pub async fn maintain_lexical_index(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        operation: MemoryLexicalIndexOperation,
        limits: MemoryStoreWriteLimits,
    ) -> Result<MemoryLexicalIndexOutcome, MemoryStoreError> {
        let Some(mut capture) = write::acquire(original, admission, limits, false).await? else {
            native::check_write(original, admission)?;
            return Ok(MemoryLexicalIndexOutcome::NoCorpus);
        };
        let project = admission.project()?;
        let project_id = native::project_id(project.scope.live)?;
        let before = || capture.check(original, admission);
        before()?;
        let state = capture.pin.json_fts_index_state(COLUMN).await?;
        before()?;
        let mode = match (operation, state) {
            (MemoryLexicalIndexOperation::Ensure, JsonFtsIndexState::Ready) => {
                return Ok(MemoryLexicalIndexOutcome::Ready(native::generation(
                    &capture.pin,
                    project_id,
                    project.version.version(),
                )));
            }
            (MemoryLexicalIndexOperation::Ensure, JsonFtsIndexState::Missing) => {
                JsonFtsIndexMode::Create
            }
            (MemoryLexicalIndexOperation::Refresh, JsonFtsIndexState::Ready) => {
                JsonFtsIndexMode::Replace
            }
            (_, state) => return Err(MemoryStoreError::LexicalIndexUnavailable(state)),
        };
        validate_corpus(&capture.pin, original, project_id, limits.read, &before).await?;
        before()?;
        capture.pin = capture.pin.clone().materialize().await?;
        let before = || capture.check(original, admission);
        before()?;
        let prepared = capture
            .pin
            .prepare_json_fts_index(COLUMN, INDEX_NAME, mode)
            .await?;
        let (_, publication) =
            commit_index(prepared, project_id, project.version.version(), before).await?;
        Ok(MemoryLexicalIndexOutcome::Published(publication))
    }

    /// Observe an index transaction under fresh ordinary write authority.
    /// Does not materialize an inherited branch, replay or repair publication.
    pub async fn resolve_lexical_index(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        key: &DeltaRecoveryKey,
        limits: MemoryStoreWriteLimits,
    ) -> Result<DeltaResolution, MemoryStoreError> {
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
        let Some(capture) = write::acquire_selected(original, admission, limits, selected).await?
        else {
            native::check_write(original, admission)?;
            return Ok(DeltaResolution::Unresolved);
        };
        let before = || {
            native::check_write(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, MemoryStoreError>(())
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
    use graphit_core::file_lock::Cancellation;
    use graphit_data::{
        lance_branch_store::LanceBranchStore, lance_module_store::LanceModuleStore,
    };
    use graphit_hub::{native::ContentLimits, policy::RequestContext};
    use std::{
        error::Error,
        time::{Duration, Instant},
    };

    async fn pin() -> (
        tempfile::TempDir,
        graphit_data::lance_branch_store::BranchSnapshot,
    ) {
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
    fn request(cancel: &Cancellation) -> RequestContext<'_> {
        RequestContext::new(
            ContentLimits {
                max_input_bytes: 1048576,
                max_output_bytes: 1048576,
                max_depth: 64,
                max_nodes: 65536,
                max_scalar_bytes: 65536,
                max_collection_entries: 65536,
            },
            cancel,
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap()
    }
    #[tokio::test]
    async fn lexical_index_real_strict_conflict_preserves_key_and_observed_winner() {
        let (_temp, pin) = pin().await;
        let project = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
        let first = pin
            .prepare_json_fts_index(COLUMN, "first", JsonFtsIndexMode::Create)
            .await
            .unwrap();
        let a = first.recovery_key().clone();
        let second = pin
            .prepare_json_fts_index(COLUMN, "second", JsonFtsIndexMode::Create)
            .await
            .unwrap();
        let b = second.recovery_key().clone();
        let (_, winner) = commit_index(first, project, "main", || Ok(()))
            .await
            .unwrap();
        let loser = commit_index(second, project, "main", || Ok(()))
            .await
            .map(|(_, publication)| publication)
            .unwrap_err();
        match &loser {
            MemoryStoreError::Commit(cause) => {
                assert_eq!(cause.recovery_key(), &b);
                assert!(cause.source().is_some());
            }
            _ => panic!("actual strict commit failure"),
        };
        assert!(loser.source().is_some());
        assert_eq!(winner.recovery_key, a);
        assert_eq!(
            pin.resolve(&a).await.unwrap(),
            DeltaResolution::Applied {
                version: winner.generation.sdk_version
            }
        );
        assert_eq!(
            pin.resolve(&b).await.unwrap(),
            DeltaResolution::Conflicted {
                version: winner.generation.sdk_version
            }
        );
    }

    #[tokio::test]
    async fn lexical_index_confirmed_boundary_retains_actual_publication_after_request_cancel() {
        let (_temp, pin) = pin().await;
        let project = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
        let prepared = pin
            .prepare_json_fts_index(COLUMN, INDEX_NAME, JsonFtsIndexMode::Create)
            .await
            .unwrap();
        let key = prepared.recovery_key().clone();
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let calls = std::cell::Cell::new(0);
        let late = commit_index(prepared, project, "main", || {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                cancel.cancel();
            }
            ctx.snapshot().check().map_err(MemoryStoreError::from)
        })
        .await
        .map(|(_, publication)| publication)
        .unwrap_err();
        let generation = match &late {
            MemoryStoreError::IndexConfirmed { publication, .. } => publication.generation.clone(),
            _ => panic!("actual confirmed index"),
        };
        match &late {
            MemoryStoreError::IndexConfirmed { publication, cause } => {
                assert_eq!(publication.generation, generation);
                assert_eq!(publication.recovery_key, key);
                assert!(matches!(cause.as_ref(), MemoryStoreError::Request(_)));
            }
            _ => panic!("confirmed index delivery failure"),
        };
        assert!(late.source().is_some());
        assert_eq!(
            pin.resolve(&key).await.unwrap(),
            DeltaResolution::Applied {
                version: generation.sdk_version
            }
        );
        // A real commit followed by cancellation; no lost-ack inference.
    }
}
