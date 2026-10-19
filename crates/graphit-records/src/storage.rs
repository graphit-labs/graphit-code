//! Physical guards for the selected authoritative record table.
use arrow_schema::Schema;
use graphit_auth::RemoteRecordStorage;
use graphit_core::{
    file_io::SnapshotContext,
    native_document_file::{CapturedProjectDocumentDirectory, CapturedUserDocumentDirectory},
};
use graphit_data::lance_branch_store::{BranchError, LanceBranchStore};
use lance::dataset::{InsertBuilder, WriteMode, WriteParams};
use lance_io::object_store::{ObjectStoreParams, StorageOptions, StorageOptionsAccessor};
use lance_table::io::commit::commit_handler_from_url;
use sha2::{Digest, Sha256};
use std::{future::Future, io, sync::Arc};

pub(crate) enum Guard {
    Project(CapturedProjectDocumentDirectory),
    User(CapturedUserDocumentDirectory),
    Remote(RemoteRecordStorage),
}
impl Guard {
    /// Bind continuations and deferred writes to the actual table, including an
    /// S3 endpoint that cannot be represented by the bucket URI alone.
    pub(crate) fn corpus_key(&self, uri: &str) -> String {
        match self {
            Self::Remote(storage) => storage.corpus_key.clone(),
            Self::Project(_) | Self::User(_) => format!("{:x}", Sha256::digest(uri.as_bytes())),
        }
    }
    pub(crate) fn transport_expired(&self) -> bool {
        matches!(self, Self::Remote(storage) if storage.check().is_err())
    }
    pub(crate) fn revalidate(&self, original: &Arc<SnapshotContext>) -> io::Result<()> {
        original.check()?;
        match self {
            Self::Project(guard) => guard.revalidate(original),
            Self::User(guard) => guard.revalidate(original),
            // Snapshot validity is separate from its old STS transport. Every
            // operation still rechecks its admission; continuous readers renew
            // expired IO between batches, and writes acquire a fresh writer.
            Self::Remote(_) => Ok(()),
        }
    }
}

/// Stop read-only SDK work with its original caller. Do not use around a commit:
/// once publication starts, its actual outcome and recovery receipt must be retained.
pub(crate) async fn read<T>(
    original: &SnapshotContext,
    work: impl Future<Output = T>,
) -> io::Result<T> {
    original.check()?;
    let deadline = async {
        if let Some(deadline) = original.deadline() {
            tokio::time::sleep_until(deadline.into()).await;
        } else {
            std::future::pending::<()>().await;
        }
    };
    tokio::select! {
        biased;
        () = original.cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
        () = deadline => Err(io::ErrorKind::TimedOut.into()),
        result = work => { original.check()?; Ok(result) },
    }
}

pub(crate) fn params(storage: &RemoteRecordStorage) -> Result<ObjectStoreParams, BranchError> {
    // The SDK imports these environment options even over explicit options.
    // Fail closed instead of extending a commit's retries or enabling HTTP.
    let configured = StorageOptions(storage.storage_options.as_ref().clone());
    let effective = StorageOptions::new(configured.0.clone());
    validate_transport_options(&configured, &effective)?;
    Ok(ObjectStoreParams {
        aws_credentials: Some(Arc::clone(&storage.credentials)),
        storage_options_accessor: Some(Arc::new(StorageOptionsAccessor::with_static_options(
            storage.storage_options.as_ref().clone(),
        ))),
        ..Default::default()
    })
}

fn validate_transport_options(
    configured: &StorageOptions,
    effective: &StorageOptions,
) -> Result<(), BranchError> {
    if effective.0.get("allow_http") != configured.0.get("allow_http")
        || effective.client_retry_timeout() != configured.client_retry_timeout()
    {
        return Err(BranchError::from(lance::Error::invalid_input(
            "S3 environment overrides the authorized HTTP or request retry policy",
        )));
    }
    Ok(())
}

pub(crate) async fn renew_snapshot(
    original: &Arc<SnapshotContext>,
    pin: &graphit_data::lance_branch_store::BranchSnapshot,
    previous: &Guard,
    storage: RemoteRecordStorage,
) -> Result<(graphit_data::lance_branch_store::BranchSnapshot, Guard), BranchError> {
    if previous.corpus_key(pin.corpus_uri()) != storage.corpus_key {
        return Err(BranchError::RecoveryBinding);
    }
    storage
        .check()
        .map_err(|e| BranchError::from(lance::Error::io(e.to_string())))?;
    let renewed = read(original, pin.with_store_params(params(&storage)?))
        .await
        .map_err(|e| BranchError::from(lance::Error::io(e.to_string())))??;
    storage
        .check()
        .map_err(|e| BranchError::from(lance::Error::io(e.to_string())))?;
    Ok((renewed, Guard::Remote(storage)))
}

/// The SDK owns conditional creation and optimistic commits; no local mirror or lock.
pub(crate) async fn open_remote(
    original: &Arc<SnapshotContext>,
    storage: &RemoteRecordStorage,
    schema: Arc<Schema>,
    create: bool,
) -> Result<Option<LanceBranchStore>, BranchError> {
    let before = || {
        original
            .check()
            .and_then(|_| storage.check())
            .map_err(|error| BranchError::from(lance::Error::io(error.to_string())))
    };
    before()?;
    let params = params(storage)?;
    let delegate = commit_handler_from_url(&storage.uri, &Some(params.clone())).await?;
    let load = || LanceBranchStore::open(&storage.uri, Arc::clone(&schema), Some(params.clone()));
    match read(original, load())
        .await
        .map_err(|e| BranchError::from(lance::Error::io(e.to_string())))?
    {
        Ok(store) => {
            before()?;
            return Ok(Some(store));
        }
        Err(BranchError::Sdk(error))
            if matches!(error.as_ref(), lance::Error::DatasetNotFound { .. }) => {}
        Err(error) => return Err(error),
    }
    before()?;
    if !create {
        return Ok(None);
    }
    let result = Box::pin(
        InsertBuilder::new(&storage.uri)
            .with_params(&WriteParams {
                mode: WriteMode::Create,
                store_params: Some(params.clone()),
                commit_handler: Some(Arc::clone(&delegate)),
                skip_auto_cleanup: true,
                ..Default::default()
            })
            .execute(vec![arrow_array::RecordBatch::new_empty(Arc::clone(
                &schema,
            ))]),
    )
    .await;
    match result {
        Ok(_) => (),
        // Another client may have created the same empty table concurrently.
        Err(lance::Error::DatasetAlreadyExists { .. }) => (),
        Err(error) => return Err(error.into()),
    }
    before()?;
    let store = read(original, load())
        .await
        .map_err(|e| BranchError::from(lance::Error::io(e.to_string())))??;
    before()?;
    Ok(Some(store))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_environment_cannot_weaken_authorized_transport_policy() {
        let configured = StorageOptions(std::collections::HashMap::from([
            ("allow_http".into(), "false".into()),
            ("client_retry_timeout".into(), "17".into()),
        ]));
        assert!(validate_transport_options(&configured, &configured).is_ok());
        for (key, value) in [("allow_http", "true"), ("client_retry_timeout", "999")] {
            let mut effective = configured.clone();
            effective.0.insert(key.into(), value.into());
            assert!(validate_transport_options(&configured, &effective).is_err());
        }
    }
}
