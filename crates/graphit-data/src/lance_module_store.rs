//! Per-module Lance snapshots and strict optimistic publication.
//!
//! Callers provide the complete physical schema and enforce domain permissions,
//! claims and leases. A dataset version is a storage fence, not an auth grant.
//! Reads never create the corpus. The SDK owns files, manifests and buffers.

//!
//! ```no_run
//! use std::sync::Arc;
//! use arrow_array::RecordBatch;
//! use arrow_schema::{Schema, Field, DataType};
//! use graphit_data::lance_module_store::{LanceModuleStore, Resolution};
//! # async fn workflow() -> Result<(), Box<dyn std::error::Error>> {
//! let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Utf8, false)]));
//! let store = LanceModuleStore::open("/authorized/task.lance", Arc::clone(&schema)).await?;
//! let pending = store.snapshot().prepare_replace(vec![RecordBatch::new_empty(schema)]).await?;
//! // Save the key before sending a commit whose acknowledgement could be lost.
//! let key_json = serde_json::to_vec(pending.recovery_key())?;
//! let next = pending.commit().await?;
//! let key = serde_json::from_slice(&key_json)?;
//! assert!(matches!(next.resolve(&key).await?, Resolution::Applied { .. }));
//! # Ok(())
//! # }
//! ```

use arrow_array::RecordBatch;
use arrow_schema::Schema;
use futures::StreamExt;
use graphit_core::file_io::SnapshotContext;
pub use lance::deps::datafusion::error::DataFusionError;
pub use lance::deps::datafusion::physical_plan::{
    SendableRecordBatchStream, stream::RecordBatchStreamAdapter,
};
use lance::{
    Dataset,
    dataset::{
        CommitBuilder, InsertBuilder, WriteMode, WriteParams,
        builder::DatasetBuilder,
        scanner::Scanner,
        transaction::{Operation, Transaction, TransactionBuilder},
    },
};
#[cfg(any(windows, test))]
use lance_io::object_store::providers::{ObjectStoreProvider, local::FileStoreProvider};
use lance_io::object_store::{ObjectStoreParams, uri_to_url};
use lance_table::{
    format::{IndexMetadata, Manifest},
    io::{commit::ManifestLocation, manifest::read_manifest_indexes},
};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::BTreeSet, error::Error, fmt, io, path::Path, sync::Arc};
use tempfile::TempDir;

/// Keep the caller's locator separate from the native path syntax the SDK uses
/// in branch URIs and object-store paths. This does not establish authority.
pub(crate) fn sdk_uri(uri: &str) -> Result<Cow<'_, str>, lance::Error> {
    if cfg!(windows) && Path::new(uri).is_absolute() {
        let directory = url::Url::from_directory_path(uri)
            .map_err(|()| lance::Error::invalid_input("invalid local dataset directory"))?;
        let path = object_store::path::Path::from_url_path(directory.path())
            .map_err(|error| lance::Error::invalid_input_source(Box::new(error)))?;
        let mut native = match directory.host_str() {
            Some(host) => format!("//{host}/{path}"),
            None => path.to_string(),
        };
        let mut components = Path::new(&native).components();
        let drive_root = matches!(components.next(), Some(std::path::Component::Prefix(prefix))
            if matches!(prefix.kind(), std::path::Prefix::Disk(_)))
            && components.next().is_none();
        if drive_root {
            native.push('/');
        }
        if !Path::new(&native).is_absolute() {
            return Err(lance::Error::invalid_input(
                "invalid local dataset directory",
            ));
        }
        return Ok(Cow::Owned(native));
    }
    Ok(Cow::Borrowed(uri))
}

// Keep the dataset URI native: named-branch BasePaths must remain parseable by
// the SDK's deep clone. Only the session's official local writer changes.
#[cfg(any(windows, test))]
#[derive(Debug)]
struct LocalObjectApi;

#[cfg(any(windows, test))]
#[async_trait::async_trait]
impl ObjectStoreProvider for LocalObjectApi {
    async fn new_store(
        &self,
        url: url::Url,
        params: &ObjectStoreParams,
    ) -> lance::Result<lance_io::object_store::ObjectStore> {
        if url.host_str().is_some() {
            return FileStoreProvider.new_store(url, params).await;
        }
        let suffix = url
            .as_str()
            .strip_prefix("file:")
            .ok_or_else(|| lance::Error::invalid_input("local provider requires a file URL"))?;
        let object_api = url::Url::parse(&format!("file-object-store:{suffix}"))
            .map_err(|error| lance::Error::invalid_input_source(Box::new(error)))?;
        FileStoreProvider.new_store(object_api, params).await
    }

    fn extract_path(&self, url: &url::Url) -> lance::Result<object_store::path::Path> {
        FileStoreProvider.extract_path(url)
    }

    fn calculate_object_store_prefix(
        &self,
        url: &url::Url,
        options: Option<&std::collections::HashMap<String, String>>,
    ) -> lance::Result<String> {
        if url.host_str().is_some() {
            FileStoreProvider.calculate_object_store_prefix(url, options)
        } else {
            Ok("file-object-store".into())
        }
    }
}

pub(crate) fn sdk_session(
    uri: &str,
    session: Arc<lance::session::Session>,
) -> Arc<lance::session::Session> {
    #[cfg(windows)]
    if Path::new(uri).is_absolute() {
        session
            .store_registry()
            .insert("file", Arc::new(LocalObjectApi));
    }
    #[cfg(not(windows))]
    let _ = uri;
    session
}

fn sdk_default_session(uri: &str) -> Option<Arc<lance::session::Session>> {
    #[cfg(windows)]
    if Path::new(uri).is_absolute() {
        return Some(sdk_session(
            uri,
            Arc::new(lance::session::Session::default()),
        ));
    }
    #[cfg(not(windows))]
    let _ = uri;
    None
}

pub(crate) fn sdk_builder(uri: &str) -> DatasetBuilder {
    let builder = DatasetBuilder::from_uri(uri);
    match sdk_default_session(uri) {
        Some(session) => builder.with_session(session),
        None => builder,
    }
}

fn local_clone_store(store: &lance_io::object_store::ObjectStore) -> bool {
    store.has_direct_local_paths()
        || (store.scheme() == "file-object-store" && store.store_prefix == "file-object-store")
}

/// Recovery data, scoped to the exact corpus locator used to open/create it.
/// Deserializing a key grants no authority; resolve validates it before IO.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryKey {
    corpus_uri: String,
    base_version: u64,
    transaction_id: String,
}
impl RecoveryKey {
    pub fn corpus_uri(&self) -> &str {
        &self.corpus_uri
    }
    pub fn base_version(&self) -> u64 {
        self.base_version
    }
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Resolution {
    Applied {
        version: u64,
    },
    Conflicted {
        version: u64,
    },
    /// The version was absent when observed. A still-running commit may publish it.
    Unresolved,
}

#[derive(Debug)]
pub enum StoreError {
    SchemaMismatch,
    CorpusMismatch,
    InvalidRecoveryKey,
    MissingTransaction,
    InvalidVersion,
    Sdk(Box<lance::Error>),
    Creation(Box<CommitFailure>),
}
impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SchemaMismatch => f.write_str("Records physical schema mismatch"),
            Self::CorpusMismatch => f.write_str("Recovery key belongs to a different corpus"),
            Self::InvalidRecoveryKey => f.write_str("Invalid Records recovery key"),
            Self::MissingTransaction => {
                f.write_str("Version exists without a verifiable transaction")
            }
            Self::InvalidVersion => f.write_str("SDK returned an unexpected version"),
            Self::Sdk(e) => write!(f, "Lance operation failed: {e}"),
            Self::Creation(e) => write!(f, "Lance creation failed: {e}"),
        }
    }
}
impl Error for StoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Sdk(e) => Some(e.as_ref()),
            Self::Creation(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}
impl From<lance::Error> for StoreError {
    fn from(value: lance::Error) -> Self {
        Self::Sdk(Box::new(value))
    }
}

/// Retains the recovery key and original SDK cause, including unknown-commit status.
#[derive(Debug)]
pub struct CommitFailure {
    key: RecoveryKey,
    cause: Box<lance::Error>,
}
impl CommitFailure {
    pub fn recovery_key(&self) -> &RecoveryKey {
        &self.key
    }
    pub fn sdk_error(&self) -> &lance::Error {
        &self.cause
    }
}
impl fmt::Display for CommitFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for CommitFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.cause.as_ref())
    }
}

pub struct LanceModuleStore {
    uri: String,
    schema: Arc<Schema>,
    dataset: Arc<Dataset>,
}

/// One staged Main creation. Preparation can write fragments but publishes no Main.
///
/// The caller retains its writer, authorization and source captures, revalidates
/// them before committing, and awaits an initiated commit. Dropping this value
/// does not publish, roll back or clean up the SDK's staged files.
pub struct PreparedCreate {
    schema: Arc<Schema>,
    transaction: Transaction,
    key: RecoveryKey,
    session: Option<Arc<lance::session::Session>>,
}

impl PreparedCreate {
    /// Attach immutable first-publication metadata before the exact Create.
    /// Its Arc can also annotate the same demand's real named Clone transaction.
    pub(crate) fn set_transaction_properties(
        &mut self,
        properties: Arc<std::collections::HashMap<String, String>>,
    ) {
        if let Some(existing) = &mut self.transaction.transaction_properties {
            Arc::make_mut(existing).extend(
                properties
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
        } else {
            self.transaction.transaction_properties = Some(properties);
        }
    }
    pub fn recovery_key(&self) -> &RecoveryKey {
        &self.key
    }

    /// Consume the staged transaction once, preserving the SDK cause and key.
    pub async fn commit(self) -> Result<(LanceModuleStore, RecoveryKey), StoreError> {
        let physical_uri = sdk_uri(self.key.corpus_uri())?;
        let builder = CommitBuilder::new(physical_uri.as_ref())
            .with_max_retries(0)
            .with_skip_auto_cleanup(true);
        let builder = match self.session {
            Some(session) => builder.with_session(session),
            None => builder,
        };
        let dataset = builder.execute(self.transaction).await.map_err(|cause| {
            StoreError::Creation(Box::new(CommitFailure {
                key: self.key.clone(),
                cause: Box::new(cause),
            }))
        })?;
        Ok((
            LanceModuleStore::from_dataset(self.key.corpus_uri(), self.schema, dataset)?,
            self.key,
        ))
    }
}

impl LanceModuleStore {
    /// Prepare a one-shot SDK Create, consuming and validating the stream to EOF.
    /// Empty images require an explicit zero-row batch with the supplied schema.
    /// Stream errors retain their cause and publish no manifest; staged files may remain.
    pub async fn prepare_create_stream(
        uri: &str,
        schema: Arc<Schema>,
        stream: SendableRecordBatchStream,
    ) -> Result<PreparedCreate, StoreError> {
        if stream.schema() != schema {
            return Err(StoreError::SchemaMismatch);
        }
        let physical_uri = sdk_uri(uri)?;
        let session = sdk_default_session(physical_uri.as_ref());
        let transaction = InsertBuilder::new(physical_uri.as_ref())
            .with_params(&WriteParams {
                mode: WriteMode::Create,
                skip_auto_cleanup: true,
                session: session.clone(),
                ..Default::default()
            })
            .execute_uncommitted_stream(checked_stream(Arc::clone(&schema), stream))
            .await?;
        let key = recovery_key(uri, &transaction);
        Ok(PreparedCreate {
            schema,
            transaction,
            key,
            session,
        })
    }

    pub async fn create(
        uri: &str,
        schema: Arc<Schema>,
        batches: Vec<RecordBatch>,
    ) -> Result<Self, StoreError> {
        Self::create_with_recovery(uri, schema, batches)
            .await
            .map(|(store, _)| store)
    }
    /// Create once, returning the actual store and Main transaction recovery key.
    ///
    /// A creation commit failure retains its key and SDK cause in `StoreError::Creation`.
    /// Other failures, including validation after commit, may return without a key;
    /// absence of a key does not prove absence of effects. Callers own authorization,
    /// the physical namespace, request checks and awaiting an initiated commit outcome.
    pub async fn create_with_recovery(
        uri: &str,
        schema: Arc<Schema>,
        batches: Vec<RecordBatch>,
    ) -> Result<(Self, RecoveryKey), StoreError> {
        Self::prepare_create(uri, schema, batches)
            .await?
            .commit()
            .await
    }

    /// Stage Create through the SDK, leaving publication to `PreparedCreate::commit`.
    ///
    /// Callers with a physical Source guard can revalidate it under their writer
    /// between preparation and commit. Staging may write unpublished fragments.
    pub async fn prepare_create(
        uri: &str,
        schema: Arc<Schema>,
        batches: Vec<RecordBatch>,
    ) -> Result<PreparedCreate, StoreError> {
        let batches = checked_batches(&schema, batches)?;
        let physical_uri = sdk_uri(uri)?;
        let session = sdk_default_session(physical_uri.as_ref());
        let params = WriteParams {
            mode: WriteMode::Create,
            skip_auto_cleanup: true,
            session: session.clone(),
            ..Default::default()
        };
        let transaction = InsertBuilder::new(physical_uri.as_ref())
            .with_params(&params)
            .execute_uncommitted(batches)
            .await?;
        let key = recovery_key(uri, &transaction);
        Ok(PreparedCreate {
            schema,
            transaction,
            key,
            session,
        })
    }
    pub async fn open(uri: &str, schema: Arc<Schema>) -> Result<Self, StoreError> {
        Self::open_checked(uri, |observed| observed == schema.as_ref()).await
    }
    pub async fn open_checked(
        uri: &str,
        accept: impl FnOnce(&Schema) -> bool,
    ) -> Result<Self, StoreError> {
        let physical_uri = sdk_uri(uri)?;
        let dataset = sdk_builder(physical_uri.as_ref()).load().await?;
        let schema = Arc::new(Schema::from(dataset.schema()));
        if !accept(&schema) {
            return Err(StoreError::SchemaMismatch);
        }
        Ok(Self {
            uri: uri.to_owned(),
            schema,
            dataset: Arc::new(dataset),
        })
    }
    /// Opens an exact SDK version using the caller's configured store session.
    ///
    /// The returned location describes the opened manifest, not authorization.
    /// Callers enforce admission and validate published root metadata separately.
    pub async fn open_native_snapshot(
        uri: &str,
        schema: Arc<Schema>,
        session: Arc<lance::session::Session>,
        dataset_version: u64,
    ) -> Result<(Self, lance_table::io::commit::ManifestLocation), StoreError> {
        let physical_uri = sdk_uri(uri)?;
        let session = sdk_session(physical_uri.as_ref(), session);
        let dataset = DatasetBuilder::from_uri(physical_uri.as_ref())
            .with_session(session)
            .with_version(dataset_version)
            .load()
            .await?;
        let location = dataset.manifest_location().clone();
        Ok((Self::from_dataset(uri, schema, dataset)?, location))
    }
    fn from_dataset(uri: &str, schema: Arc<Schema>, dataset: Dataset) -> Result<Self, StoreError> {
        if Schema::from(dataset.schema()) != *schema {
            return Err(StoreError::SchemaMismatch);
        }
        Ok(Self {
            uri: uri.to_owned(),
            schema,
            dataset: Arc::new(dataset),
        })
    }
    pub fn snapshot(&self) -> ModuleSnapshot {
        ModuleSnapshot {
            uri: self.uri.clone(),
            schema: Arc::clone(&self.schema),
            dataset: Arc::clone(&self.dataset),
            ast_reader: None,
        }
    }
    pub async fn resolve(&self, key: &RecoveryKey) -> Result<Resolution, StoreError> {
        if key.corpus_uri != self.uri {
            return Err(StoreError::CorpusMismatch);
        }
        let version = key
            .base_version
            .checked_add(1)
            .filter(|_| !key.transaction_id.is_empty())
            .ok_or(StoreError::InvalidRecoveryKey)?;
        let observed = match self.dataset.read_version_transaction(version).await {
            Ok(observed) => observed,
            Err(error)
                if !error.is_commit_status_unknown()
                    && matches!(
                        error,
                        lance::Error::DatasetNotFound { .. } | lance::Error::VersionNotFound { .. }
                    ) =>
            {
                return Ok(Resolution::Unresolved);
            }
            Err(error) => return Err(error.into()),
        };
        if observed.version != version {
            return Err(StoreError::InvalidVersion);
        }
        let transaction = observed.transaction.ok_or(StoreError::MissingTransaction)?;
        Ok(if transaction.uuid == key.transaction_id {
            Resolution::Applied { version }
        } else {
            Resolution::Conflicted { version }
        })
    }
}

/// Validate one batch at a time without retaining the corpus or replaying input.
pub(crate) fn checked_stream(
    schema: Arc<Schema>,
    stream: SendableRecordBatchStream,
) -> SendableRecordBatchStream {
    let expected = Arc::clone(&schema);
    let batches = futures::stream::try_unfold((stream, false), move |(mut stream, seen)| {
        let expected = Arc::clone(&expected);
        async move {
            match stream.next().await {
                Some(Ok(batch)) if batch.schema() == expected => Ok(Some((batch, (stream, true)))),
                Some(Ok(_)) => Err(lance::deps::datafusion::error::DataFusionError::External(
                    Box::new(StoreError::SchemaMismatch),
                )),
                Some(Err(error)) => Err(error),
                None if seen => Ok(None),
                None => Err(lance::deps::datafusion::error::DataFusionError::Execution(
                    "Empty stream requires an explicit zero-row batch".into(),
                )),
            }
        }
    });
    Box::pin(RecordBatchStreamAdapter::new(schema, batches))
}

impl crate::lance_branch_store::BranchSnapshot {
    /// Stages the retained physical branch without materializing an inherited target.
    ///
    /// The caller retains its genuine source/PROJECT capture and Original until
    /// the returned worker is joined, including during shutdown. This readonly
    /// bridge does not expose a writable module snapshot or grant access.
    pub fn spawn_native_clone_local(
        &self,
        stage: TempDir,
        original: Arc<SnapshotContext>,
    ) -> io::Result<tokio::task::JoinHandle<io::Result<NativeModuleClone>>> {
        self.module_snapshot()
            .spawn_native_clone_local(stage, original)
    }
}

#[derive(Clone)]
pub struct ModuleSnapshot {
    pub(crate) uri: String,
    pub(crate) schema: Arc<Schema>,
    pub(crate) dataset: Arc<Dataset>,
    pub(crate) ast_reader: Option<Arc<std::fs::File>>,
}
impl ModuleSnapshot {
    pub(crate) async fn build_code_fts(
        &self,
        columns: &[&str],
        original: &SnapshotContext,
    ) -> Result<Self, StoreError> {
        original
            .check()
            .map_err(|e| StoreError::from(lance::Error::io_source(Box::new(e))))?;
        let mut dataset = self.dataset.as_ref().clone();
        if dataset.count_rows(None).await? != 0 {
            let params = lance_index::scalar::InvertedIndexParams::code().max_token_length(None);
            for column in columns {
                original
                    .check()
                    .map_err(|e| StoreError::from(lance::Error::io_source(Box::new(e))))?;
                let metadata = lance::index::CreateIndexBuilder::new(
                    &mut dataset,
                    &[*column],
                    lance_index::IndexType::Inverted,
                    &params,
                )
                .name(format!("{column}_code_fts"))
                .replace(false)
                .execute_uncommitted()
                .await?;
                original
                    .check()
                    .map_err(|e| StoreError::from(lance::Error::io_source(Box::new(e))))?;
                let transaction = TransactionBuilder::new(
                    dataset.version_id(),
                    Operation::CreateIndex {
                        new_indices: vec![metadata],
                        removed_indices: vec![],
                    },
                )
                .build();
                dataset = CommitBuilder::new(Arc::new(dataset))
                    .with_max_retries(0)
                    .with_skip_auto_cleanup(true)
                    .execute(transaction)
                    .await?;
            }
        }
        original
            .check()
            .map_err(|e| StoreError::from(lance::Error::io_source(Box::new(e))))?;
        Ok(Self {
            uri: self.uri.clone(),
            schema: Arc::clone(&self.schema),
            dataset: Arc::new(dataset),
            ast_reader: self.ast_reader.clone(),
        })
    }
    /// Capture an exact historical version in this snapshot's physical branch,
    /// using its existing SDK session/store. No latest lookup or clone occurs.
    pub async fn at_version(&self, version: u64) -> Result<Self, StoreError> {
        if version == 0 {
            return Err(StoreError::InvalidVersion);
        }
        let dataset = self.dataset.checkout_version(version).await?;
        if dataset.version_id() != version
            || dataset.branch_location() != self.dataset.branch_location()
        {
            return Err(StoreError::InvalidVersion);
        }
        let mut snapshot =
            LanceModuleStore::from_dataset(&self.uri, Arc::clone(&self.schema), dataset)?
                .snapshot();
        snapshot.ast_reader = self.ast_reader.clone();
        Ok(snapshot)
    }

    /// Literal locator of this retained pin, without I/O or a new observation.
    /// This data grants no access; callers retain their admission and recheck the pin.
    pub fn corpus_uri(&self) -> &str {
        &self.uri
    }
    pub fn version(&self) -> u64 {
        self.dataset.version_id()
    }
    pub fn schema(&self) -> &Schema {
        &self.schema
    }
    pub fn scan(&self) -> Scanner {
        Scanner::new(Arc::clone(&self.dataset))
    }
    /// Stages this supported local version with the SDK's physical deep clone.
    ///
    /// The caller retains and joins the returned handle before shutdown or
    /// publication. A started SDK filesystem copy cannot be hard-cancelled;
    /// Original is checked before and after it. The worker owns the directory
    /// until copying has finished, including when the waiting future is dropped.
    pub fn spawn_native_clone_local(
        &self,
        stage: TempDir,
        original: Arc<SnapshotContext>,
    ) -> io::Result<tokio::task::JoinHandle<io::Result<NativeModuleClone>>> {
        original.check()?;
        let runtime = tokio::runtime::Handle::try_current().map_err(io::Error::other)?;
        let worker_runtime = runtime.clone();
        let snapshot = self.clone();
        Ok(runtime.spawn_blocking(move || {
            original.check()?;
            if !std::fs::symlink_metadata(stage.path())?.file_type().is_dir()
                || std::fs::read_dir(stage.path())?.next().transpose()?.is_some()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Native clone staging directory must be empty",
                ));
            }
            worker_runtime.block_on(async {
                let store = snapshot.dataset.object_store(None).await.map_err(io::Error::other)?;
                if !local_clone_store(&store)
                    || snapshot.dataset.schema().fields_pre_order().any(|field| field.is_blob() || field.is_blob_v2())
                {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "Native local clone requires local storage without blobs",
                    ));
                }
                let root = snapshot.dataset.branch_location().find_main().map_err(io::Error::other)?;
                let manifest = snapshot.dataset.manifest();
                let mut referenced_bases = BTreeSet::new();
                for fragment in manifest.fragments.iter() {
                    original.check()?;
                    referenced_bases.extend(fragment.referenced_lance_files().filter_map(|file| file.base_id));
                    referenced_bases.extend(fragment.deletion_file.iter().filter_map(|file| file.base_id));
                }
                if !manifest.base_paths.is_empty() {
                    original.check()?;
                    let indices = read_manifest_indexes(&store, snapshot.dataset.manifest_location(), manifest)
                        .await.map_err(io::Error::other)?;
                    original.check()?;
                    referenced_bases.extend(indices.iter().filter_map(|index| index.base_id));
                }
                for id in referenced_bases {
                    original.check()?;
                    let base = manifest.base_paths.get(&id).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "Native clone references an absent base")
                    })?;
                    if !base.is_dataset_root
                        || (!Path::new(&base.path).is_absolute() && url::Url::parse(&base.path).is_err())
                        || !matches!(uri_to_url(&base.path).map_err(io::Error::other)?.scheme(), "file" | "file+uring")
                        || base.extract_path(snapshot.dataset.session().store_registry()).map_err(io::Error::other)? != root.path
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::Unsupported,
                            "Native local clone requires explicit local bases at the captured table root",
                        ));
                    }
                    let base_store = snapshot.dataset.object_store(Some(id)).await.map_err(io::Error::other)?;
                    if !local_clone_store(base_store.as_ref()) {
                        return Err(io::Error::new(io::ErrorKind::Unsupported, "Native clone base requires local storage"));
                    }
                }
                let target = stage.path().to_str().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "Invalid native clone directory")
                })?;
                original.check()?;
                let mut source = snapshot.dataset.as_ref().clone();
                let physical_target = sdk_uri(target).map_err(io::Error::other)?;
                let result = source.deep_clone(physical_target.as_ref(), snapshot.version(), Some(ObjectStoreParams::default())).await;
                original.check()?;
                let dataset = result.map_err(io::Error::other)?;
                let output = ModuleSnapshot {
                    uri: dataset.uri().to_owned(),
                    schema: snapshot.schema,
                    dataset: Arc::new(dataset),
                    ast_reader: None,
                };
                Ok(NativeModuleClone { snapshot: output, directory: stage })
            })
        }))
    }
    /// Borrows physical SDK metadata from this retained version.
    ///
    /// Raw indices include system entries the query planner may hide. This is
    /// metadata, not authorization or a complete list of objects to copy.
    pub async fn native_metadata(
        &self,
        original: &SnapshotContext,
    ) -> io::Result<NativeModuleMetadata<'_>> {
        original.check()?;
        let read = async {
            let store = self.dataset.object_store(None).await?;
            read_manifest_indexes(
                &store,
                self.dataset.manifest_location(),
                self.dataset.manifest(),
            )
            .await
        };
        let result = tokio::select! {
            biased;
            () = original.cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
            result = async {
                match original.deadline() {
                    Some(deadline) => match tokio::time::timeout_at(deadline.into(), read).await {
                        Ok(result) => result.map_err(io::Error::other),
                        Err(_) => Err(io::ErrorKind::TimedOut.into()),
                    },
                    None => read.await.map_err(io::Error::other),
                }
            } => result,
        };
        original.check()?;
        Ok(NativeModuleMetadata {
            uri: &self.uri,
            dataset: &self.dataset,
            indices: result?,
        })
    }
    pub async fn prepare_replace(
        &self,
        batches: Vec<RecordBatch>,
    ) -> Result<PreparedCommit, StoreError> {
        self.prepare_overwrite(Arc::clone(&self.schema), batches)
            .await
    }
    /// Prepare a complete image with the caller's explicit physical schema.
    ///
    /// Unlike `prepare_replace`, this permits schema or metadata changes. Empty
    /// input retains the supplied schema in an empty batch. Publication still
    /// uses this snapshot's strict SDK fence; authorization belongs to the caller.
    pub async fn prepare_overwrite(
        &self,
        schema: Arc<Schema>,
        batches: Vec<RecordBatch>,
    ) -> Result<PreparedCommit, StoreError> {
        self.version()
            .checked_add(1)
            .ok_or(StoreError::InvalidVersion)?;
        let batches = checked_batches(&schema, batches)?;
        let params = WriteParams {
            mode: WriteMode::Overwrite,
            skip_auto_cleanup: true,
            ..Default::default()
        };
        let transaction = InsertBuilder::new(Arc::clone(&self.dataset))
            .with_params(&params)
            .execute_uncommitted(batches)
            .await?;
        let key = recovery_key(&self.uri, &transaction);
        Ok(PreparedCommit {
            original: self.clone(),
            schema,
            transaction,
            key,
        })
    }
}

/// Owns disposable SDK clone storage; borrowed metadata cannot outlive it.
pub struct NativeModuleClone {
    // Drop the SDK owner before removing its physical directory.
    snapshot: ModuleSnapshot,
    directory: TempDir,
}
impl NativeModuleClone {
    pub fn directory(&self) -> &Path {
        self.directory.path()
    }
    pub async fn native_metadata(
        &self,
        original: &SnapshotContext,
    ) -> io::Result<NativeModuleMetadata<'_>> {
        self.snapshot.native_metadata(original).await
    }
}

/// Readonly SDK data borrowing the captured ModuleSnapshot's physical owner.
///
/// Index file lists can be absent, and blob sidecars or external bases can need
/// additional SDK discovery. This view grants neither access nor publication.
#[derive(Debug)]
pub struct NativeModuleMetadata<'a> {
    uri: &'a str,
    dataset: &'a Dataset,
    indices: Vec<IndexMetadata>,
}
impl NativeModuleMetadata<'_> {
    pub fn uri(&self) -> &str {
        self.uri
    }
    /// Borrow the retained SDK dataset, including its configured object-store session.
    /// The caller retains its authorization and original request around SDK IO.
    pub fn dataset(&self) -> &Dataset {
        self.dataset
    }
    pub fn manifest(&self) -> &Manifest {
        self.dataset.manifest()
    }
    pub fn manifest_location(&self) -> &ManifestLocation {
        self.dataset.manifest_location()
    }
    pub fn indices(&self) -> &[IndexMetadata] {
        &self.indices
    }
}

pub struct PreparedCommit {
    original: ModuleSnapshot,
    schema: Arc<Schema>,
    transaction: Transaction,
    key: RecoveryKey,
}
impl PreparedCommit {
    pub fn recovery_key(&self) -> &RecoveryKey {
        &self.key
    }
    pub async fn commit(self) -> Result<LanceModuleStore, CommitFailure> {
        let dataset = CommitBuilder::new(Arc::clone(&self.original.dataset))
            .with_max_retries(0)
            .with_skip_auto_cleanup(true)
            .execute(self.transaction)
            .await
            .map_err(|cause| CommitFailure {
                key: self.key,
                cause: Box::new(cause),
            })?;
        Ok(LanceModuleStore {
            uri: self.original.uri,
            schema: self.schema,
            dataset: Arc::new(dataset),
        })
    }
}
fn recovery_key(uri: &str, transaction: &Transaction) -> RecoveryKey {
    RecoveryKey {
        corpus_uri: uri.to_owned(),
        base_version: transaction.read_version,
        transaction_id: transaction.uuid.clone(),
    }
}
fn checked_batches(
    schema: &Arc<Schema>,
    mut batches: Vec<RecordBatch>,
) -> Result<Vec<RecordBatch>, StoreError> {
    if batches
        .iter()
        .any(|batch| batch.schema().as_ref() != schema.as_ref())
    {
        return Err(StoreError::SchemaMismatch);
    }
    if batches.is_empty() {
        batches.push(RecordBatch::new_empty(Arc::clone(schema)));
    }
    Ok(batches)
}

#[cfg(test)]
mod sdk_uri_tests {
    use super::*;
    #[tokio::test]
    async fn local_object_api_preserves_native_branch_bases_and_independent_clone() {
        use arrow_array::{RecordBatch, UInt64Array};
        use arrow_schema::{DataType, Field, Schema};
        use lance::dataset::refs::Ref;

        let temporary = tempfile::tempdir().unwrap();
        let source = temporary
            .path()
            .join("ordinary-native-directory-".repeat(4))
            .join("second-native-directory-".repeat(4))
            .join("source %20 β.lance");
        let stage = tempfile::Builder::new()
            .prefix("independent %20 β-")
            .tempdir_in(temporary.path())
            .unwrap();
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        let source_uri = sdk_uri(source.to_str().unwrap()).unwrap();
        let session = Arc::new(lance::session::Session::default());
        session
            .store_registry()
            .insert("file", Arc::new(LocalObjectApi));
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::UInt64,
            false,
        )]));
        let batch = |values| {
            RecordBatch::try_new(
                Arc::clone(&schema),
                vec![Arc::new(UInt64Array::from(values))],
            )
            .unwrap()
        };
        let transaction = InsertBuilder::new(source_uri.as_ref())
            .with_params(&WriteParams {
                session: Some(Arc::clone(&session)),
                skip_auto_cleanup: true,
                ..Default::default()
            })
            .execute_uncommitted(vec![batch(vec![1_u64, 2])])
            .await
            .unwrap();
        let mut main = CommitBuilder::new(source_uri.as_ref())
            .with_session(Arc::clone(&session))
            .execute(transaction)
            .await
            .unwrap();
        assert_eq!(main.uri(), source_uri);
        assert_eq!(
            main.object_store(None).await.unwrap().scheme(),
            "file-object-store"
        );
        let branch = "native-record-branch-".repeat(5);
        let named = main
            .create_branch(&branch, Ref::Version(None, Some(main.version_id())), None)
            .await
            .unwrap();
        let transaction = InsertBuilder::new(Arc::new(named))
            .with_params(&WriteParams {
                mode: WriteMode::Append,
                skip_auto_cleanup: true,
                ..Default::default()
            })
            .execute_uncommitted(vec![batch(vec![3_u64])])
            .await
            .unwrap();
        let named = DatasetBuilder::from_uri(source_uri.as_ref())
            .with_session(Arc::clone(&session))
            .with_branch(&branch, None)
            .load()
            .await
            .unwrap();
        let named = CommitBuilder::new(Arc::new(named))
            .execute(transaction)
            .await
            .unwrap();
        assert_eq!(named.count_rows(None).await.unwrap(), 3);
        assert_eq!(main.count_rows(None).await.unwrap(), 2);
        assert!(source.join("tree").join(&branch).as_os_str().len() > 260);
        let snapshot = LanceModuleStore::from_dataset(source_uri.as_ref(), schema, named)
            .unwrap()
            .snapshot();
        let original = Arc::new(SnapshotContext::service(
            graphit_core::file_lock::Cancellation::default(),
        ));
        let clone = snapshot
            .spawn_native_clone_local(stage, Arc::clone(&original))
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        assert!(
            clone
                .native_metadata(&original)
                .await
                .unwrap()
                .manifest()
                .base_paths
                .is_empty()
        );
        drop(snapshot);
        drop(main);
        std::fs::remove_dir_all(&source).unwrap();
        let fresh = Arc::new(lance::session::Session::default());
        fresh
            .store_registry()
            .insert("file", Arc::new(LocalObjectApi));
        let destination_uri = sdk_uri(clone.directory().to_str().unwrap()).unwrap();
        let reopened = DatasetBuilder::from_uri(destination_uri.as_ref())
            .with_session(fresh)
            .load()
            .await
            .unwrap();
        let read = reopened.scan().try_into_batch().await.unwrap();
        let mut values = read
            .column(0)
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap()
            .values()
            .to_vec();
        values.sort_unstable();
        assert_eq!(values, vec![1, 2, 3]);
    }

    #[test]
    fn remote_file_and_relative_locators_stay_literal() {
        for uri in [
            "s3://bucket/data%20set?region=us-east-1",
            "s3+ddb://bucket/table?ddbTableName=commits",
            "file:///C:/source%20%F0%9F%A6%80.lance",
            "file://server/share/source%25.lance",
            "relative/source.lance",
            "C:drive-relative.lance",
        ] {
            assert!(matches!(sdk_uri(uri).unwrap(), Cow::Borrowed(value) if value == uri));
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_native_locator_stays_literal() {
        let uri = "/tmp/source %20 🦀 ? # \\.lance";
        assert!(matches!(sdk_uri(uri).unwrap(), Cow::Borrowed(value) if value == uri));
    }

    #[cfg(windows)]
    #[test]
    fn windows_native_arguments_preserve_drive_unc_and_literal_names() {
        for (input, expected) in [
            (r"C:\", "C:/"),
            (r"C:\source %20 🦀.lance", "C:/source %20 🦀.lance"),
            (r"\\?\C:\source %20 🦀.lance", "C:/source %20 🦀.lance"),
            (r"\\?\C:\source.lance\tree\dev", "C:/source.lance/tree/dev"),
            (
                r"\\server\share\source %20 🦀.lance",
                "//server/share/source %20 🦀.lance",
            ),
            (
                r"\\?\UNC\server\share\source %20 🦀.lance",
                "//server/share/source %20 🦀.lance",
            ),
        ] {
            let physical = sdk_uri(input).unwrap();
            assert_eq!(physical, expected);
            assert!(Path::new(physical.as_ref()).is_absolute());
            assert_eq!(sdk_uri(physical.as_ref()).unwrap(), physical);
        }
    }

    #[tokio::test]
    async fn native_long_path_keeps_recovery_locator_named_pin_and_independent_clone() {
        use arrow_array::{RecordBatch, StringArray, UInt64Array};
        use arrow_schema::{DataType, Field, Schema};
        let directory = tempfile::tempdir().unwrap();
        let corpus = directory
            .path()
            .canonicalize()
            .unwrap()
            .join("ordinary-native-directory-".repeat(5))
            .join("second-native-directory-".repeat(4))
            .join("source %20 🦀.lance");
        std::fs::create_dir_all(corpus.parent().unwrap()).unwrap();
        assert!(corpus.as_os_str().len() > 260);
        let uri = corpus.to_str().unwrap();
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("value", DataType::UInt64, false),
        ]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(StringArray::from(vec!["one", "two"])),
                Arc::new(UInt64Array::from(vec![1, 2])),
            ],
        )
        .unwrap();
        let (store, key) =
            LanceModuleStore::create_with_recovery(uri, Arc::clone(&schema), vec![batch])
                .await
                .unwrap();
        assert_eq!(key.corpus_uri(), uri);
        assert_eq!(store.snapshot().corpus_uri(), uri);
        let branches =
            crate::lance_branch_store::LanceBranchStore::open(uri, Arc::clone(&schema), None)
                .await
                .unwrap();
        let pin = branches
            .capture_target(Some("dev"))
            .await
            .unwrap()
            .materialize()
            .await
            .unwrap();
        assert_eq!(pin.module_snapshot().corpus_uri(), uri);
        assert_eq!(pin.physical_branch(), Some("dev"));
        let reopened =
            crate::lance_branch_store::BranchSnapshot::capture_observed(uri, Some("dev"), None)
                .await
                .unwrap();
        assert_eq!(reopened.physical_branch(), Some("dev"));
        assert_eq!(reopened.version(), pin.version());
        assert_eq!(reopened.module_snapshot().corpus_uri(), uri);
        assert_eq!(
            store.resolve(&key).await.unwrap(),
            Resolution::Applied { version: 1 }
        );
        let append = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(StringArray::from(vec!["three"])),
                Arc::new(UInt64Array::from(vec![3_u64])),
            ],
        )
        .unwrap();
        let updated = pin
            .prepare_merge(vec![append], vec!["id".into()])
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();
        let original = Arc::new(SnapshotContext::service(
            graphit_core::file_lock::Cancellation::default(),
        ));
        let snapshot = updated.module_snapshot();
        let cloned = snapshot
            .spawn_native_clone_local(tempfile::tempdir().unwrap(), Arc::clone(&original))
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        assert!(
            cloned
                .native_metadata(&original)
                .await
                .unwrap()
                .manifest()
                .base_paths
                .is_empty()
        );
        drop(snapshot);
        drop(updated);
        drop(reopened);
        drop(pin);
        drop(branches);
        drop(store);
        std::fs::remove_dir_all(&corpus).unwrap();
        let reopened = LanceModuleStore::open(cloned.directory().to_str().unwrap(), schema)
            .await
            .unwrap();
        let read = reopened.snapshot().scan().try_into_batch().await.unwrap();
        let mut values = read
            .column_by_name("value")
            .unwrap()
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap()
            .values()
            .to_vec();
        values.sort_unstable();
        assert_eq!(values, vec![1, 2, 3]);
    }
}
