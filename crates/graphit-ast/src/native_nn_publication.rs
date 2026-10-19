//! Causal receipt for an SDK-confirmed vector update to the current AST corpus.
//!
//! Commit outcomes and actual recovery keys survive a post-commit control
//! failure. Acquisition, bootstrap, authority and model/source integrity remain
//! with the actual caller.

use graphit_core::file_io::SnapshotContext;
use graphit_data::lance_branch_store::{BranchSnapshot, DeltaCommitFailure, DeltaRecoveryKey};
use std::{error::Error, fmt, io, sync::Arc};

#[derive(Debug)]
pub enum NativeNnPublicationError {
    Operation(io::Error),
    RequestMismatch,
    Commit {
        failure: Box<DeltaCommitFailure>,
        completion_error: Option<io::Error>,
    },
}

impl fmt::Display for NativeNnPublicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(error) => write!(f, "native NN publication operation: {error}"),
            Self::RequestMismatch => f.write_str("native NN publication request mismatch"),
            Self::Commit {
                failure,
                completion_error,
            } => {
                write!(f, "native NN publication commit: {failure}")?;
                if let Some(error) = completion_error {
                    write!(f, "; completion operation: {error}")?;
                }
                Ok(())
            }
        }
    }
}

impl Error for NativeNnPublicationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(error) => Some(error),
            Self::Commit { failure, .. } => Some(failure.as_ref()),
            Self::RequestMismatch => None,
        }
    }
}

/// An SDK-confirmed applied generation, even if request control later failed.
pub struct NativeNnPublication {
    snapshot: BranchSnapshot,
    key: DeltaRecoveryKey,
    original: Arc<SnapshotContext>,
    completion_error: Option<io::Error>,
}

impl NativeNnPublication {
    pub fn version(&self) -> u64 {
        self.snapshot.version()
    }
    pub fn physical_branch(&self) -> Option<&str> {
        self.snapshot.physical_branch()
    }
    pub fn inherited_target(&self) -> Option<&str> {
        self.snapshot.inherited_target()
    }
    pub fn recovery_key(&self) -> &DeltaRecoveryKey {
        &self.key
    }
    pub fn completion_error(&self) -> Option<&io::Error> {
        self.completion_error.as_ref()
    }

    /// Move a late control failure without retaining the applied snapshot.
    pub fn take_completion_error(&mut self) -> Option<io::Error> {
        self.completion_error.take()
    }

    /// Borrow the actual applied snapshot only for the same active Original.
    pub fn snapshot(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> Result<&BranchSnapshot, NativeNnPublicationError> {
        if !Arc::ptr_eq(&self.original, original) {
            return Err(NativeNnPublicationError::RequestMismatch);
        }
        original
            .check()
            .map_err(NativeNnPublicationError::Operation)?;
        Ok(&self.snapshot)
    }
}

pub fn finish_publication(
    result: Result<BranchSnapshot, DeltaCommitFailure>,
    key: DeltaRecoveryKey,
    original: &Arc<SnapshotContext>,
) -> Result<NativeNnPublication, NativeNnPublicationError> {
    let completion_error = original.check().err();
    match result {
        Ok(snapshot) => Ok(NativeNnPublication {
            snapshot,
            key,
            original: original.clone(),
            completion_error,
        }),
        Err(failure) => Err(NativeNnPublicationError::Commit {
            failure: Box::new(failure),
            completion_error,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::RecordBatch;
    use graphit_core::file_lock::Cancellation;
    use graphit_data::{lance_branch_store::DeltaResolution, lance_module_store::LanceModuleStore};
    use std::time::{Duration, Instant};
    use tempfile::TempDir;
    fn original() -> Arc<SnapshotContext> {
        Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(60),
            Cancellation::default(),
        ))
    }
    #[tokio::test]
    async fn actual_applied_outcome_survives_cancellation_at_finishing_boundary() {
        let temp = TempDir::new().unwrap();
        let batch = RecordBatch::new_empty(crate::corpus::schema());
        let uri = temp.path().join("nn.lance");
        LanceModuleStore::create(uri.to_str().unwrap(), batch.schema(), vec![batch.clone()])
            .await
            .unwrap();
        let snapshot = BranchSnapshot::capture_observed(uri.to_str().unwrap(), None, None)
            .await
            .unwrap();
        let prepared = snapshot.prepare_overwrite(vec![batch]).await.unwrap();
        let key = prepared.recovery_key().clone();
        let result = prepared.commit().await;
        assert!(result.is_ok());
        let context = original();
        context.cancellation().cancel();
        // Actual SDK outcome, controlled finishing boundary; no timing-race claim.
        let applied = finish_publication(result, key.clone(), &context).unwrap();
        assert_eq!(applied.version(), snapshot.version() + 1);
        assert_eq!(applied.recovery_key(), &key);
        assert_eq!(
            applied.completion_error().unwrap().kind(),
            io::ErrorKind::Interrupted
        );
        assert!(
            matches!(applied.snapshot(&context), Err(NativeNnPublicationError::Operation(ref e)) if e.kind()==io::ErrorKind::Interrupted)
        );
        assert_eq!(
            snapshot.resolve(&key).await.unwrap(),
            DeltaResolution::Applied {
                version: applied.version()
            }
        );
    }

    #[tokio::test]
    async fn actual_failed_cas_keeps_sdk_key_cause_and_post_control_error() {
        let temp = TempDir::new().unwrap();
        let batch = RecordBatch::new_empty(crate::corpus::schema());
        let uri = temp.path().join("nn.lance");
        LanceModuleStore::create(uri.to_str().unwrap(), batch.schema(), vec![batch.clone()])
            .await
            .unwrap();
        let snapshot = BranchSnapshot::capture_observed(uri.to_str().unwrap(), None, None)
            .await
            .unwrap();
        let winner = snapshot
            .prepare_overwrite(vec![batch.clone()])
            .await
            .unwrap();
        let loser = snapshot.prepare_overwrite(vec![batch]).await.unwrap();
        let key = loser.recovery_key().clone();
        let applied = winner.commit().await.unwrap();
        let result = loser.commit().await;
        let expected_cause = result.as_ref().err().unwrap().sdk_error().to_string();
        let context = Arc::new(SnapshotContext::new(
            Instant::now(),
            Cancellation::default(),
        ));
        let failure = finish_publication(result, key.clone(), &context)
            .err()
            .unwrap();
        assert!(failure.source().is_some());
        match failure {
            NativeNnPublicationError::Commit {
                failure,
                completion_error,
            } => {
                assert_eq!(failure.recovery_key(), &key);
                assert_eq!(failure.sdk_error().to_string(), expected_cause);
                assert_eq!(completion_error.unwrap().kind(), io::ErrorKind::TimedOut);
                assert!(failure.source().is_some());
            }
            other => panic!("actual commit failure required: {other:?}"),
        }
        assert_eq!(
            snapshot.resolve(&key).await.unwrap(),
            DeltaResolution::Conflicted {
                version: applied.version()
            }
        );
    }
}
