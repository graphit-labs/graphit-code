//! Captured Lance branch snapshots and strictly fenced incremental publication.
//!
//! Caller authorization, cancellation and record revisions stay with the service.
//! A recovery key identifies storage evidence and grants no authority.
//!
//! ```no_run
//! use std::sync::Arc;
//! use arrow_array::{RecordBatch, StringArray};
//! use arrow_schema::{Schema, Field, DataType};
//! use graphit_data::lance_branch_store::{LanceBranchStore, DeltaResolution};
//! # async fn workflow() -> Result<(), Box<dyn std::error::Error>> {
//! let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Utf8, false)]));
//! let store = LanceBranchStore::open("/authorized/task.lance", schema.clone(), None).await?;
//! let branch = store.capture_target(Some("encoded-feature")).await?.materialize().await?;
//! let patch = RecordBatch::try_new(schema, vec![Arc::new(StringArray::from(vec!["event-1"]))])?;
//! let pending = branch.prepare_merge(vec![patch], vec!["id".into()]).await?;
//! let saved = serde_json::to_vec(pending.recovery_key())?;
//! let next = pending.commit().await?;
//! let key = serde_json::from_slice(&saved)?;
//! assert!(matches!(next.resolve(&key).await?, DeltaResolution::Applied {..}));
//! // After stopping the producers owned by this caller and acquiring its writer:
//! drop(branch);
//! next.retire_named().await?;
//! # Ok(())
//! # }
//! ```

use crate::lance_module_store::{
    RecordBatchStreamAdapter, SendableRecordBatchStream, checked_stream,
};
use arrow_array::{Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Schema};
use futures::{StreamExt, stream::BoxStream};
pub use lance::deps::datafusion::logical_expr::{Expr, col, lit};
use lance::{
    Dataset,
    dataset::{
        CommitBuilder, InsertBuilder, MergeInsertBuilder, WhenMatched, WhenNotMatched,
        WhenNotMatchedBySource, WriteMode, WriteParams,
        builder::DatasetBuilder,
        refs::{BranchContents, BranchIdentifier, Ref},
        scanner::Scanner,
        write::merge_insert::SourceDedupeBehavior,
    },
};
use lance_io::object_store::{ObjectStore, ObjectStoreParams};
use lance_table::{
    format::{IndexMetadata, Manifest, Transaction},
    io::commit::{
        CommitError, CommitHandler, ManifestLocation, ManifestNamingScheme, ManifestWriter,
        PredecessorIdentity, commit_handler_from_url,
    },
};
use object_store::{ObjectStore as PublicObjectStore, path::Path};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, error::Error, fmt, sync::Arc};

mod ast_corpus;
pub use ast_corpus::{AST_INDEX_BATCH, AstCorpusStats, cleanup_ast_corpus};
mod file_replace;
mod fts;
mod fts_state;
mod git_refs;
mod vector;
pub use fts::{JsonFtsIndexMode, PreparedIndex};
pub use fts_state::JsonFtsIndexState;
/// Typed SDK failures already exposed by BranchError::Sdk.
pub use lance::Error as LanceError;

#[derive(Debug)]
pub enum BranchError {
    UnsupportedCommitScheme(String),
    SchemaMismatch,
    InheritedSnapshot,
    DefaultSnapshot,
    EmptyPatch,
    InvalidJoinColumns,
    InvalidFileSelection,
    NullKey,
    DuplicateKey,
    InvalidVersion,
    InvalidGitObjectId,
    GitSnapshotMissing,
    GitSnapshotConflict,
    GitSnapshotBranchMismatch,
    RecoveryBinding,
    InvalidRecoveryKey,
    MissingTransaction,
    JsonIndexColumn(String),
    JsonIndexReplacementMissing {
        column: String,
        name: String,
    },
    JsonIndexReplacementIncompatible {
        column: String,
        name: String,
    },
    NullJsonIndexValue {
        column: String,
        row: usize,
    },
    InvalidJsonIndexValue {
        column: String,
        row: usize,
        cause: serde_json::Error,
    },
    Sdk(Box<lance::Error>),
    CreationConflict {
        cause: Option<Box<lance::Error>>,
    },
    BootstrapIncomplete {
        cause: Option<Box<lance::Error>>,
    },
    BootstrapUnknown {
        cause: Option<Box<lance::Error>>,
        observations: Vec<lance::Error>,
    },
}
impl fmt::Display for BranchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Records branch error: {self:?}")
    }
}
impl Error for BranchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Sdk(e) => Some(e.as_ref()),
            Self::InvalidJsonIndexValue { cause, .. } => Some(cause),
            Self::CreationConflict { cause }
            | Self::BootstrapIncomplete { cause }
            | Self::BootstrapUnknown { cause, .. } => cause.as_deref().map(|e| e as &dyn Error),
            _ => None,
        }
    }
}
impl From<lance::Error> for BranchError {
    fn from(e: lance::Error) -> Self {
        Self::Sdk(Box::new(e))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeltaRecoveryKey {
    corpus_uri: String,
    physical_branch: Option<String>,
    base_version: u64,
    transaction_id: String,
}
impl DeltaRecoveryKey {
    pub fn corpus_uri(&self) -> &str {
        &self.corpus_uri
    }
    pub fn physical_branch(&self) -> Option<&str> {
        self.physical_branch.as_deref()
    }
    pub fn base_version(&self) -> u64 {
        self.base_version
    }
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum DeltaResolution {
    Applied { version: u64 },
    Conflicted { version: u64 },
    Unresolved,
}
#[derive(Debug)]
pub struct DeltaCommitFailure {
    key: DeltaRecoveryKey,
    cause: Box<lance::Error>,
}
impl DeltaCommitFailure {
    pub fn recovery_key(&self) -> &DeltaRecoveryKey {
        &self.key
    }
    pub fn sdk_error(&self) -> &lance::Error {
        &self.cause
    }
}
impl fmt::Display for DeltaCommitFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.cause.fmt(f)
    }
}
impl Error for DeltaCommitFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.cause.as_ref())
    }
}

pub struct LanceBranchStore {
    initial: BranchSnapshot,
}
impl LanceBranchStore {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn open(
        uri: &str,
        schema: Arc<Schema>,
        params: Option<ObjectStoreParams>,
    ) -> Result<Self, BranchError> {
        safe_selector(uri)?;
        tracing::debug!("commit_handler_start");
        let delegate = commit_handler_from_url(uri, &params).await;
        tracing::debug!("commit_handler_end");
        let delegate = delegate?;
        let mut builder = DatasetBuilder::from_uri(uri).with_commit_handler(Arc::clone(&delegate));
        if let Some(p) = params {
            builder = builder.with_store_params(p);
        }
        tracing::debug!("load_start");
        let dataset = builder.load().await;
        tracing::debug!("load_end");
        let dataset = dataset?;
        checked_schema(&dataset, &schema)?;
        Ok(Self {
            initial: BranchSnapshot {
                uri: uri.to_owned(),
                schema,
                dataset: Arc::new(dataset),
                delegate,
                inherited_target: None,
                ast_reader: None,
            },
        })
    }
    pub async fn capture_target(
        &self,
        encoded_branch: Option<&str>,
    ) -> Result<BranchSnapshot, BranchError> {
        let mut default = self.initial.dataset.as_ref().clone();
        default.checkout_latest().await?;
        checked_schema(&default, &self.initial.schema)?;
        let mut captured = self.initial.clone();
        captured.dataset = Arc::new(default);
        if let Some(branch) = encoded_branch {
            // Metadata, rather than a manifest alone, establishes a branch reference.
            match captured.dataset.branches().get(branch).await {
                Ok(_) => {
                    captured.dataset = Arc::new(captured.dataset.checkout_branch(branch).await?);
                    checked_schema(&captured.dataset, &captured.schema)?;
                }
                Err(lance::Error::RefNotFound { .. }) => {
                    captured.inherited_target = Some(branch.to_owned());
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(captured)
    }
}

#[derive(Clone)]
pub struct BranchSnapshot {
    uri: String,
    schema: Arc<Schema>,
    dataset: Arc<Dataset>,
    delegate: Arc<dyn CommitHandler>,
    inherited_target: Option<String>,
    ast_reader: Option<Arc<std::fs::File>>,
}

/// SDK identity observed around the same named-branch capture as its data pin.
/// Consumers may retain and compare it in a cursor; it grants no authority to
/// select a URI, branch or parent, and does not retain files against SDK GC.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BranchProvenance {
    branch: String,
    identifier: BranchIdentifier,
    parent_branch: Option<String>,
    parent_version: u64,
    create_at: u64,
}
impl BranchProvenance {
    fn observed(branch: &str, contents: BranchContents) -> Self {
        Self {
            branch: branch.to_owned(),
            identifier: contents.identifier,
            parent_branch: contents.parent_branch,
            parent_version: contents.parent_version,
            create_at: contents.create_at,
        }
    }
}

impl BranchSnapshot {
    /// Capture the complete schema of the actual selected physical branch.
    /// Named branches may have independent vector widths and schema metadata.
    /// A missing named branch retains the default pin with `inherited_target`;
    /// creating that branch requires the separate `materialize` effect.
    /// The caller supplies its validated selection, authorization and request lifetime.
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn capture_observed(
        uri: &str,
        encoded_branch: Option<&str>,
        params: Option<ObjectStoreParams>,
    ) -> Result<Self, BranchError> {
        safe_selector(uri)?;
        tracing::debug!("commit_handler_start");
        let delegate = commit_handler_from_url(uri, &params).await;
        tracing::debug!("commit_handler_end");
        let delegate = delegate?;
        let mut builder = DatasetBuilder::from_uri(uri).with_commit_handler(Arc::clone(&delegate));
        if let Some(params) = params {
            builder = builder.with_store_params(params);
        }
        tracing::debug!("load_start");
        let dataset = builder.load().await;
        tracing::debug!("load_end");
        let mut dataset = dataset?;
        let mut inherited_target = None;
        if let Some(branch) = encoded_branch {
            match dataset.branches().get(branch).await {
                Ok(_) => dataset = dataset.checkout_branch(branch).await?,
                Err(lance::Error::RefNotFound { .. }) => inherited_target = Some(branch.to_owned()),
                Err(error) => return Err(error.into()),
            }
        }
        Ok(Self {
            uri: uri.to_owned(),
            schema: Arc::new(Schema::from(dataset.schema())),
            dataset: Arc::new(dataset),
            delegate,
            inherited_target,
            ast_reader: None,
        })
    }
    /// Open an exact published version through the caller's private SDK session.
    /// This observes data and grants no write or installation authority.
    pub async fn capture_native(
        uri: &str,
        session: Arc<lance::session::Session>,
        version: u64,
    ) -> Result<(Self, ManifestLocation), BranchError> {
        safe_selector(uri)?;
        if version == 0 {
            return Err(BranchError::InvalidVersion);
        }
        let delegate = commit_handler_from_url(uri, &None).await?;
        let dataset = DatasetBuilder::from_uri(uri)
            .with_session(session)
            .with_version(version)
            .with_commit_handler(Arc::clone(&delegate))
            .load()
            .await?;
        if dataset.version_id() != version {
            return Err(BranchError::InvalidVersion);
        }
        let location = dataset.manifest_location().clone();
        Ok((
            Self {
                uri: uri.to_owned(),
                schema: Arc::new(Schema::from(dataset.schema())),
                dataset: Arc::new(dataset),
                delegate,
                inherited_target: None,
                ast_reader: None,
            },
            location,
        ))
    }
    pub(crate) fn dataset(&self) -> &Arc<Dataset> {
        &self.dataset
    }
    /// Reopen the selected physical URI/version with a fresh SDK session.
    pub(crate) async fn reopen(&self) -> Result<Self, BranchError> {
        let mut builder = DatasetBuilder::from_uri(self.dataset.uri())
            .with_version(self.version())
            .with_session(Arc::new(lance::session::Session::default()))
            .with_commit_handler(Arc::clone(&self.delegate));
        if let Some(params) = self.dataset.store_params() {
            builder = builder.with_store_params(params.clone());
        }
        let dataset = builder.load().await?;
        if dataset.version_id() != self.version()
            || dataset.manifest().branch.as_deref() != self.physical_branch()
        {
            return Err(BranchError::InvalidVersion);
        }
        checked_schema(&dataset, &self.schema)?;
        let mut snapshot = self.clone();
        snapshot.dataset = Arc::new(dataset);
        Ok(snapshot)
    }
    /// Stage a complete replacement using the first batch's complete schema.
    /// All batches must share that schema. An empty vector is invalid; represent
    /// zero rows explicitly with `RecordBatch::new_empty(schema)`.
    /// Inherited snapshots require separate materialization before preparing.
    /// Preparation does not publish; commit retains the existing CAS/recovery rules.
    pub async fn prepare_overwrite(
        &self,
        batches: Vec<RecordBatch>,
    ) -> Result<PreparedOverwrite, BranchError> {
        if self.inherited_target.is_some() {
            return Err(BranchError::InheritedSnapshot);
        }
        self.version()
            .checked_add(1)
            .ok_or(BranchError::InvalidVersion)?;
        let schema = batches.first().ok_or(BranchError::EmptyPatch)?.schema();
        if batches.iter().any(|batch| batch.schema() != schema) {
            return Err(BranchError::SchemaMismatch);
        }
        let transaction = InsertBuilder::new(Arc::clone(&self.dataset))
            .with_params(&WriteParams {
                mode: WriteMode::Overwrite,
                skip_auto_cleanup: true,
                ..Default::default()
            })
            .execute_uncommitted(batches)
            .await?;
        if transaction.read_version != self.version() {
            return Err(BranchError::InvalidVersion);
        }
        let key = DeltaRecoveryKey {
            corpus_uri: self.uri.clone(),
            physical_branch: self.physical_branch().map(str::to_owned),
            base_version: self.version(),
            transaction_id: transaction.uuid.clone(),
        };
        Ok(PreparedOverwrite {
            snapshot: self.clone(),
            transaction,
            key,
        })
    }

    /// Stage a complete one-shot image, validating every batch through SDK EOF.
    /// Empty images need an explicit zero-row batch. Publication and recovery
    /// use the same retained physical snapshot as the batch-based API.
    pub async fn prepare_overwrite_stream(
        &self,
        schema: Arc<Schema>,
        stream: SendableRecordBatchStream,
    ) -> Result<PreparedOverwrite, BranchError> {
        if self.inherited_target.is_some() {
            return Err(BranchError::InheritedSnapshot);
        }
        self.version()
            .checked_add(1)
            .ok_or(BranchError::InvalidVersion)?;
        if stream.schema() != schema {
            return Err(BranchError::SchemaMismatch);
        }
        let transaction = InsertBuilder::new(Arc::clone(&self.dataset))
            .with_params(&WriteParams {
                mode: WriteMode::Overwrite,
                skip_auto_cleanup: true,
                ..Default::default()
            })
            .execute_uncommitted_stream(checked_stream(schema, stream))
            .await?;
        if transaction.read_version != self.version() {
            return Err(BranchError::InvalidVersion);
        }
        let key = DeltaRecoveryKey {
            corpus_uri: self.uri.clone(),
            physical_branch: self.physical_branch().map(str::to_owned),
            base_version: self.version(),
            transaction_id: transaction.uuid.clone(),
        };
        Ok(PreparedOverwrite {
            snapshot: self.clone(),
            transaction,
            key,
        })
    }

    /// Exact authoritative locator captured by this SDK snapshot.
    pub fn corpus_uri(&self) -> &str {
        &self.uri
    }
    pub fn version(&self) -> u64 {
        self.dataset.version_id()
    }
    pub fn physical_branch(&self) -> Option<&str> {
        self.dataset.manifest().branch.as_deref()
    }
    pub fn inherited_target(&self) -> Option<&str> {
        self.inherited_target.as_deref()
    }
    /// Retire this captured physical named branch once, without forced cleanup.
    ///
    /// Default and inherited snapshots are refused before SDK I/O. An SDK error
    /// can follow reference deletion or partial directory cleanup, and keeps its
    /// original cause. Retained readers may lose files after deletion; this is
    /// not a branch identity CAS. The caller must stop the producers it owns
    /// and serialize its mutation with the existing corpus writer.
    pub async fn retire_named(self) -> Result<(), BranchError> {
        if self.inherited_target.is_some() {
            return Err(BranchError::InheritedSnapshot);
        }
        let branch = self.physical_branch().ok_or(BranchError::DefaultSnapshot)?;
        let mut dataset = self.dataset.as_ref().clone();
        dataset.delete_branch(branch).await?;
        Ok(())
    }

    /// Replace an expired transport without changing the captured observation.
    /// The caller still authorizes the destination and owns cancellation/deadline.
    pub async fn with_store_params(&self, params: ObjectStoreParams) -> Result<Self, BranchError> {
        let mut builder = DatasetBuilder::from_uri(&self.uri)
            .with_commit_handler(Arc::clone(&self.delegate))
            .with_session(self.dataset.session())
            .with_store_params(params);
        builder = match self.physical_branch() {
            Some(branch) => builder.with_branch(branch, Some(self.version())),
            None => builder.with_version(self.version()),
        };
        let dataset = builder.load().await?;
        checked_schema(&dataset, &self.schema)?;
        if dataset.version_id() != self.version()
            || dataset.manifest().branch.as_deref() != self.physical_branch()
        {
            return Err(BranchError::RecoveryBinding);
        }
        let mut captured = self.clone();
        captured.dataset = Arc::new(dataset);
        Ok(captured)
    }

    /// Read an exact historical version of this admitted physical branch.
    /// No latest lookup, materialization or write takes place.
    /// An inherited target remains inherited on the captured default branch,
    /// even if that target has since materialized. Write preparation still
    /// requires the separate materialization effect.
    /// The already captured version reuses its immutable pin without SDK I/O.
    pub async fn at_version(&self, version: u64) -> Result<Self, BranchError> {
        if version == 0 {
            return Err(BranchError::InvalidVersion);
        }
        if version == self.version() {
            return Ok(self.clone());
        }
        let branch = self.physical_branch().map(str::to_owned);
        let dataset = self
            .dataset
            .checkout_version(Ref::Version(branch.clone(), Some(version)))
            .await?;
        if dataset.version_id() != version
            || dataset.manifest().branch.as_deref() != branch.as_deref()
        {
            return Err(BranchError::InvalidVersion);
        }
        Ok(Self {
            uri: self.uri.clone(),
            schema: Arc::new(Schema::from(dataset.schema())),
            dataset: Arc::new(dataset),
            delegate: Arc::clone(&self.delegate),
            inherited_target: self.inherited_target.clone(),
            ast_reader: self.ast_reader.clone(),
        })
    }

    /// Capture an exact version and the identity of its admitted named branch.
    /// Named data is reopened with current transport parameters and a fresh SDK
    /// session between two reference reads. A changed reference is a binding
    /// conflict. Default and still-inherited receivers retain their physical
    /// pin and return no named provenance. Callers compare retained provenance
    /// before delivering a continuation; cursor fields do not choose a branch.
    pub async fn at_version_with_provenance(
        &self,
        version: u64,
    ) -> Result<(Self, Option<BranchProvenance>), BranchError> {
        if self.physical_branch().is_none() {
            return Ok((self.at_version(version).await?, None));
        }
        let (snapshot, provenance) = self.capture_named_pin(version, false).await?;
        Ok((snapshot, Some(provenance)))
    }

    /// Read a default version inherited before this admitted child was created.
    /// The SDK reference must identify a direct default parent and a forkpoint
    /// at or after `version`. The reference is checked around fresh child/data
    /// capture; no URI or branch is accepted from a cursor. The returned pin
    /// retains the child's inherited target and refuses parent writes. Its
    /// provenance identifies the child used for this proof, not the parent.
    pub async fn at_inherited_version(
        &self,
        version: u64,
    ) -> Result<(Self, BranchProvenance), BranchError> {
        self.capture_named_pin(version, true).await
    }

    async fn capture_named_pin(
        &self,
        version: u64,
        from_parent: bool,
    ) -> Result<(Self, BranchProvenance), BranchError> {
        if version == 0 {
            return Err(BranchError::InvalidVersion);
        }
        let branch = self.physical_branch().ok_or(BranchError::InvalidVersion)?;
        let before = BranchProvenance::observed(branch, self.dataset.branches().get(branch).await?);
        if from_parent && (before.parent_branch.is_some() || version > before.parent_version) {
            return Err(BranchError::InvalidVersion);
        }
        let captured = async {
            let mut builder = DatasetBuilder::from_uri(&self.uri)
                .with_commit_handler(Arc::clone(&self.delegate))
                .with_session(Arc::new(lance::session::Session::default()))
                .with_branch(branch, None);
            if let Some(params) = self.dataset.store_params() {
                builder = builder.with_store_params(params.clone());
            }
            let child = builder.load().await?;
            if child.manifest().branch.as_deref() != Some(branch) {
                return Err(BranchError::RecoveryBinding);
            }
            let physical_branch = if from_parent { None } else { Some(branch) };
            // The pinned SDK builder loads the root before a delayed named
            // checkout. Resolve the exact version only on the selected chain;
            // a named version need not exist on the default branch.
            let dataset = child
                .checkout_version(Ref::Version(
                    physical_branch.map(str::to_owned),
                    Some(version),
                ))
                .await?;
            if dataset.version_id() != version
                || dataset.manifest().branch.as_deref() != physical_branch
            {
                return Err(BranchError::RecoveryBinding);
            }
            Ok(dataset)
        }
        .await;
        let after = match self.dataset.branches().get(branch).await {
            Ok(contents) => BranchProvenance::observed(branch, contents),
            Err(lance::Error::RefNotFound { .. }) => return Err(BranchError::RecoveryBinding),
            Err(error) => return Err(error.into()),
        };
        if before != after {
            return Err(BranchError::RecoveryBinding);
        }
        let dataset = captured?;
        Ok((
            Self {
                uri: self.uri.clone(),
                schema: Arc::new(Schema::from(dataset.schema())),
                dataset: Arc::new(dataset),
                delegate: Arc::clone(&self.delegate),
                ast_reader: self.ast_reader.clone(),
                inherited_target: if from_parent {
                    Some(branch.to_owned())
                } else {
                    self.inherited_target.clone()
                },
            },
            before,
        ))
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }
    pub fn scan(&self) -> Scanner {
        Scanner::new(Arc::clone(&self.dataset))
    }
    /// Borrow this exact physical pin for read-only module consumers. No latest
    /// lookup, SDK clone, row copy or branch materialization takes place.
    pub fn module_snapshot(&self) -> crate::lance_module_store::ModuleSnapshot {
        crate::lance_module_store::ModuleSnapshot {
            uri: self.uri.clone(),
            schema: Arc::clone(&self.schema),
            dataset: Arc::clone(&self.dataset),
            ast_reader: self.ast_reader.clone(),
        }
    }
    pub async fn materialize(mut self) -> Result<Self, BranchError> {
        let Some(branch) = self.inherited_target.clone() else {
            return Ok(self);
        };
        match self.observe_bootstrap(&branch).await {
            BootstrapObservation::Absent => (),
            observation => return Err(observation.into_error(None)),
        }
        let mut source = self.dataset.as_ref().clone();
        match source
            .create_branch(&branch, Ref::Version(None, Some(self.version())), None)
            .await
        {
            Ok(dataset) => {
                checked_schema(&dataset, &self.schema)?;
                if dataset.version_id() != self.version()
                    || dataset.manifest().branch.as_deref() != Some(branch.as_str())
                {
                    return Err(BranchError::InvalidVersion);
                }
                self.dataset = Arc::new(dataset);
                self.inherited_target = None;
                Ok(self)
            }
            Err(cause) => {
                let observation = self.observe_bootstrap(&branch).await;
                Err(observation.into_error(Some(Box::new(cause))))
            }
        }
    }
    async fn observe_bootstrap(&self, branch: &str) -> BootstrapObservation {
        match self.dataset.branches().get(branch).await {
            Ok(_) => {
                return match self.dataset.checkout_branch(branch).await {
                    Ok(_) => BootstrapObservation::Valid,
                    Err(e) => BootstrapObservation::Unknown(vec![e]),
                };
            }
            Err(lance::Error::RefNotFound { .. }) => (),
            Err(e) => return BootstrapObservation::Unknown(vec![e]),
        }
        match self
            .dataset
            .checkout_version(Ref::Version(Some(branch.to_owned()), Some(self.version())))
            .await
        {
            Ok(_) => BootstrapObservation::Orphan,
            Err(e)
                if !e.is_commit_status_unknown()
                    && missing_checkout_manifest(&e)
                    && self.delegate.is_version_not_found_definitive() =>
            {
                match self.dataset.checkout_branch(branch).await {
                    Ok(_) => BootstrapObservation::Orphan,
                    Err(e)
                        if !e.is_commit_status_unknown()
                            && missing_checkout_manifest(&e)
                            && self.delegate.is_version_not_found_definitive() =>
                    {
                        BootstrapObservation::Absent
                    }
                    Err(e) => BootstrapObservation::Unknown(vec![e]),
                }
            }
            Err(e) => BootstrapObservation::Unknown(vec![e]),
        }
    }
    pub async fn prepare_merge(
        &self,
        batches: Vec<RecordBatch>,
        join_columns: Vec<String>,
    ) -> Result<PreparedDelta, BranchError> {
        if self.inherited_target.is_some() {
            return Err(BranchError::InheritedSnapshot);
        }
        self.version()
            .checked_add(1)
            .ok_or(BranchError::InvalidVersion)?;
        validate_patch(&self.schema, &batches, &join_columns)?;
        let mut builder = MergeInsertBuilder::try_new(Arc::clone(&self.dataset), join_columns)?;
        builder
            .when_matched(WhenMatched::UpdateAll)
            .when_not_matched(WhenNotMatched::InsertAll)
            .when_not_matched_by_source(WhenNotMatchedBySource::Keep)
            .source_dedupe_behavior(SourceDedupeBehavior::Fail)
            .use_index(true)
            .conflict_retries(0);
        let merge = Box::pin(builder.try_build()?.execute_uncommitted_batches(batches)).await?;
        if merge.transaction.read_version != self.version() {
            return Err(BranchError::InvalidVersion);
        }
        self.prepared_delta(merge.transaction, merge.affected_rows)
    }

    /// Consume a one-shot full-schema patch through the SDK merge executor.
    /// `None` keeps unmatched target rows; `Some` deletes only rows satisfying
    /// the SDK target expression. An explicit empty batch supports deletion-only
    /// deltas. NULL and duplicate keys fail during consumption. Only join keys
    /// are retained for validation; the SDK also keeps its dedupe Fail policy.
    pub async fn prepare_merge_stream(
        &self,
        stream: SendableRecordBatchStream,
        join_columns: Vec<String>,
        delete_unmatched: Option<Expr>,
    ) -> Result<PreparedDelta, BranchError> {
        if self.inherited_target.is_some() {
            return Err(BranchError::InheritedSnapshot);
        }
        self.version()
            .checked_add(1)
            .ok_or(BranchError::InvalidVersion)?;
        if stream.schema() != self.schema {
            return Err(BranchError::SchemaMismatch);
        }
        let columns = checked_join_columns(&self.schema, &join_columns)?;
        let schema = Arc::clone(&self.schema);
        let mut keys = HashSet::new();
        let batches = checked_stream(Arc::clone(&schema), stream).map(move |result| {
            let batch = result?;
            let arrays: Vec<&StringArray> = columns
                .iter()
                .map(|index| {
                    batch
                        .column(*index)
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .expect("validated UTF8 Arrow schema")
                })
                .collect();
            if arrays.iter().any(|array| array.null_count() != 0) {
                return Err(lance::deps::datafusion::error::DataFusionError::External(
                    Box::new(BranchError::NullKey),
                ));
            }
            for row in 0..batch.num_rows() {
                let key: Vec<String> = arrays
                    .iter()
                    .map(|array| array.value(row).to_owned())
                    .collect();
                if !keys.insert(key) {
                    return Err(lance::deps::datafusion::error::DataFusionError::External(
                        Box::new(BranchError::DuplicateKey),
                    ));
                }
            }
            Ok(batch)
        });
        let stream: SendableRecordBatchStream =
            Box::pin(RecordBatchStreamAdapter::new(schema, batches));
        let mut builder = MergeInsertBuilder::try_new(Arc::clone(&self.dataset), join_columns)?;
        builder
            .when_matched(WhenMatched::UpdateAll)
            .when_not_matched(WhenNotMatched::InsertAll)
            .when_not_matched_by_source(delete_unmatched.map_or(
                WhenNotMatchedBySource::Keep,
                WhenNotMatchedBySource::DeleteIf,
            ))
            .source_dedupe_behavior(SourceDedupeBehavior::Fail)
            .use_index(true)
            .conflict_retries(0);
        let merge = builder.try_build()?.execute_uncommitted(stream).await?;
        if merge.transaction.read_version != self.version() {
            return Err(BranchError::InvalidVersion);
        }
        self.prepared_delta(merge.transaction, merge.affected_rows)
    }
    fn prepared_delta(
        &self,
        transaction: lance::dataset::transaction::Transaction,
        affected_rows: Option<lance_select::RowAddrTreeMap>,
    ) -> Result<PreparedDelta, BranchError> {
        if transaction.read_version != self.version() || self.inherited_target.is_some() {
            return Err(BranchError::InvalidVersion);
        }
        let target_version = self
            .version()
            .checked_add(1)
            .ok_or(BranchError::InvalidVersion)?;
        let key = DeltaRecoveryKey {
            corpus_uri: self.uri.clone(),
            physical_branch: self.physical_branch().map(str::to_owned),
            base_version: self.version(),
            transaction_id: transaction.uuid.clone(),
        };
        let location = self.dataset.branch_location();
        let guard = Arc::new(StrictCommit {
            delegate: Arc::clone(&self.delegate),
            path: location.path.clone(),
            branch: location.branch,
            version: target_version,
            original: None,
        });
        Ok(PreparedDelta {
            snapshot: self.clone(),
            transaction,
            affected_rows,
            key,
            guard,
        })
    }
    pub async fn resolve(&self, key: &DeltaRecoveryKey) -> Result<DeltaResolution, BranchError> {
        if self.inherited_target.is_some() {
            return Err(BranchError::InheritedSnapshot);
        }
        if key.corpus_uri != self.uri || key.physical_branch.as_deref() != self.physical_branch() {
            return Err(BranchError::RecoveryBinding);
        }
        let version = key
            .base_version
            .checked_add(1)
            .filter(|_| !key.transaction_id.is_empty())
            .ok_or(BranchError::InvalidRecoveryKey)?;
        let observed = match self.dataset.read_version_transaction(version).await {
            Ok(v) => v,
            Err(e)
                if !e.is_commit_status_unknown()
                    && missing_version(&e)
                    && self.delegate.is_version_not_found_definitive() =>
            {
                return Ok(DeltaResolution::Unresolved);
            }
            Err(e) => return Err(e.into()),
        };
        if observed.version != version {
            return Err(BranchError::InvalidVersion);
        }
        let transaction = observed
            .transaction
            .ok_or(BranchError::MissingTransaction)?;
        Ok(if transaction.uuid == key.transaction_id {
            DeltaResolution::Applied { version }
        } else {
            DeltaResolution::Conflicted { version }
        })
    }
}
enum BootstrapObservation {
    Absent,
    Valid,
    Orphan,
    Unknown(Vec<lance::Error>),
}
impl BootstrapObservation {
    fn into_error(self, cause: Option<Box<lance::Error>>) -> BranchError {
        match self {
            Self::Valid => BranchError::CreationConflict { cause },
            Self::Orphan => BranchError::BootstrapIncomplete { cause },
            Self::Absent => BranchError::BootstrapUnknown {
                cause,
                observations: Vec::new(),
            },
            Self::Unknown(observations) => BranchError::BootstrapUnknown {
                cause,
                observations,
            },
        }
    }
}
/// Opaque SDK overwrite prepared against one retained physical branch/version.
/// The caller retains its real authorization, capture and Original through commit.
pub struct PreparedOverwrite {
    snapshot: BranchSnapshot,
    transaction: lance::dataset::transaction::Transaction,
    key: DeltaRecoveryKey,
}
impl PreparedOverwrite {
    /// Attach domain metadata to the exact retained transaction executed at commit.
    pub(crate) fn set_transaction_property(&mut self, key: String, value: String) {
        let properties = self
            .transaction
            .transaction_properties
            .get_or_insert_with(Default::default);
        Arc::make_mut(properties).insert(key, value);
    }

    /// Capture before commit so an uncertain SDK result can be resolved later.
    pub fn recovery_key(&self) -> &DeltaRecoveryKey {
        &self.key
    }
    /// Publish once with the existing zero-retry version CAS and no automatic cleanup.
    /// Failure preserves both the SDK cause and this operation's recovery key.
    pub async fn commit(self) -> Result<BranchSnapshot, DeltaCommitFailure> {
        let dataset = CommitBuilder::new(Arc::clone(&self.snapshot.dataset))
            .with_max_retries(0)
            .with_skip_auto_cleanup(true)
            .with_commit_handler(Arc::clone(&self.snapshot.delegate))
            .execute(self.transaction)
            .await
            .map_err(|cause| DeltaCommitFailure {
                key: self.key,
                cause: Box::new(cause),
            })?;
        let mut snapshot = self.snapshot;
        snapshot.schema = Arc::new(Schema::from(dataset.schema()));
        snapshot.dataset = Arc::new(dataset);
        Ok(snapshot)
    }
}

pub struct PreparedDelta {
    snapshot: BranchSnapshot,
    transaction: lance::dataset::transaction::Transaction,
    affected_rows: Option<lance_select::RowAddrTreeMap>,
    key: DeltaRecoveryKey,
    guard: Arc<StrictCommit>,
}
impl PreparedDelta {
    /// Attach domain metadata to the exact retained transaction executed at commit.
    pub(crate) fn set_transaction_property(&mut self, key: String, value: String) {
        let properties = self
            .transaction
            .transaction_properties
            .get_or_insert_with(Default::default);
        Arc::make_mut(properties).insert(key, value);
    }

    pub fn recovery_key(&self) -> &DeltaRecoveryKey {
        &self.key
    }
    pub async fn commit(self) -> Result<BranchSnapshot, DeltaCommitFailure> {
        let mut builder = CommitBuilder::new(Arc::clone(&self.snapshot.dataset))
            .with_max_retries(0)
            .with_skip_auto_cleanup(true)
            .with_commit_handler(self.guard);
        if let Some(rows) = self.affected_rows {
            builder = builder.with_affected_rows(rows);
        }
        let dataset =
            builder
                .execute(self.transaction)
                .await
                .map_err(|cause| DeltaCommitFailure {
                    key: self.key,
                    cause: Box::new(cause),
                })?;
        let mut snapshot = self.snapshot;
        snapshot.schema = Arc::new(Schema::from(dataset.schema()));
        snapshot.dataset = Arc::new(dataset);
        Ok(snapshot)
    }
}
fn checked_schema(dataset: &Dataset, schema: &Schema) -> Result<(), BranchError> {
    if Schema::from(dataset.schema()) != *schema {
        Err(BranchError::SchemaMismatch)
    } else {
        Ok(())
    }
}
fn safe_selector(uri: &str) -> Result<(), BranchError> {
    if let Ok(url) = url::Url::parse(uri) {
        let scheme = url.scheme();
        if cfg!(windows) && scheme.len() == 1 {
            return Ok(());
        }
        if !matches!(
            scheme,
            "file"
                | "file-object-store"
                | "s3"
                | "gs"
                | "az"
                | "abfss"
                | "memory"
                | "oss"
                | "tos"
                | "shared-memory"
                | "goosefs"
                | "cos"
        ) {
            return Err(BranchError::UnsupportedCommitScheme(scheme.to_owned()));
        }
    }
    Ok(())
}
fn validate_patch(
    schema: &Schema,
    batches: &[RecordBatch],
    names: &[String],
) -> Result<(), BranchError> {
    if batches.iter().all(|b| b.num_rows() == 0) {
        return Err(BranchError::EmptyPatch);
    }
    if batches.iter().any(|b| b.schema().as_ref() != schema) {
        return Err(BranchError::SchemaMismatch);
    }
    let columns = checked_join_columns(schema, names)?;
    let mut keys = HashSet::new();
    for batch in batches {
        let arrays: Vec<&StringArray> = columns
            .iter()
            .map(|i| {
                batch
                    .column(*i)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .expect("validated UTF8 Arrow schema")
            })
            .collect();
        if arrays.iter().any(|a| a.null_count() != 0) {
            return Err(BranchError::NullKey);
        }
        for row in 0..batch.num_rows() {
            let key: Vec<&str> = arrays.iter().map(|a| a.value(row)).collect();
            if !keys.insert(key) {
                return Err(BranchError::DuplicateKey);
            }
        }
    }
    Ok(())
}

fn checked_join_columns(schema: &Schema, names: &[String]) -> Result<Vec<usize>, BranchError> {
    if names.is_empty() {
        return Err(BranchError::InvalidJoinColumns);
    }
    let mut columns = Vec::with_capacity(names.len());
    let mut unique = HashSet::with_capacity(names.len());
    for name in names {
        if !unique.insert(name) {
            return Err(BranchError::InvalidJoinColumns);
        }
        let index = schema
            .index_of(name)
            .map_err(|_| BranchError::InvalidJoinColumns)?;
        if schema.field(index).data_type() != &DataType::Utf8 {
            return Err(BranchError::InvalidJoinColumns);
        }
        columns.push(index);
    }
    Ok(columns)
}

struct StrictCommit {
    delegate: Arc<dyn CommitHandler>,
    path: Path,
    branch: Option<String>,
    version: u64,
    original: Option<Arc<graphit_core::file_io::SnapshotContext>>,
}
impl std::fmt::Debug for StrictCommit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StrictCommit")
            .field("path", &self.path)
            .field("branch", &self.branch)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}
impl StrictCommit {
    fn check(&self, manifest: &Manifest, path: &Path) -> Result<(), CommitError> {
        if let Some(original) = &self.original {
            original.check().map_err(|error| {
                CommitError::OtherError(lance_core::Error::io_source(Box::new(error)))
            })?;
        }
        if path != &self.path || manifest.branch != self.branch || manifest.version != self.version
        {
            Err(CommitError::CommitConflict)
        } else {
            Ok(())
        }
    }
}

#[async_trait::async_trait]
#[allow(clippy::too_many_arguments)]
impl CommitHandler for StrictCommit {
    fn is_version_not_found_definitive(&self) -> bool {
        self.delegate.is_version_not_found_definitive()
    }
    fn propagate_commit_error_after_success(&self) -> bool {
        self.delegate.propagate_commit_error_after_success()
    }
    async fn resolve_latest_location(
        &self,
        path: &Path,
        store: &ObjectStore,
    ) -> lance_core::Result<ManifestLocation> {
        self.delegate.resolve_latest_location(path, store).await
    }
    async fn resolve_version_location(
        &self,
        path: &Path,
        version: u64,
        store: &dyn PublicObjectStore,
    ) -> lance_core::Result<ManifestLocation> {
        self.delegate
            .resolve_version_location(path, version, store)
            .await
    }
    async fn version_exists(
        &self,
        path: &Path,
        version: u64,
        store: &dyn PublicObjectStore,
        scheme: ManifestNamingScheme,
    ) -> lance_core::Result<bool> {
        self.delegate
            .version_exists(path, version, store, scheme)
            .await
    }
    fn list_detached_manifest_locations<'a>(
        &self,
        path: &Path,
        store: &'a ObjectStore,
    ) -> BoxStream<'a, lance_core::Result<ManifestLocation>> {
        self.delegate.list_detached_manifest_locations(path, store)
    }
    fn list_manifest_locations<'a>(
        &self,
        path: &Path,
        store: &'a ObjectStore,
        descending: bool,
    ) -> BoxStream<'a, lance_core::Result<ManifestLocation>> {
        self.delegate
            .list_manifest_locations(path, store, descending)
    }
    fn list_manifest_locations_since<'a>(
        &self,
        path: &Path,
        store: &'a ObjectStore,
        version: u64,
    ) -> BoxStream<'a, lance_core::Result<ManifestLocation>> {
        self.delegate
            .list_manifest_locations_since(path, store, version)
    }
    async fn commit(
        &self,
        manifest: &mut Manifest,
        indices: Option<Vec<IndexMetadata>>,
        path: &Path,
        store: &ObjectStore,
        writer: ManifestWriter,
        scheme: ManifestNamingScheme,
        transaction: Option<Transaction>,
    ) -> Result<ManifestLocation, CommitError> {
        self.check(manifest, path)?;
        self.delegate
            .commit(manifest, indices, path, store, writer, scheme, transaction)
            .await
    }
    fn supports_predecessor_condition(&self) -> bool {
        self.delegate.supports_predecessor_condition()
    }
    async fn resolve_latest_identity(
        &self,
        path: &Path,
        store: &ObjectStore,
    ) -> lance_core::Result<Option<PredecessorIdentity>> {
        self.delegate.resolve_latest_identity(path, store).await
    }
    async fn resolve_identity(
        &self,
        path: &Path,
        store: &ObjectStore,
        version: u64,
    ) -> lance_core::Result<Option<PredecessorIdentity>> {
        self.delegate.resolve_identity(path, store, version).await
    }
    async fn commit_after(
        &self,
        manifest: &mut Manifest,
        indices: Option<Vec<IndexMetadata>>,
        path: &Path,
        store: &ObjectStore,
        writer: ManifestWriter,
        scheme: ManifestNamingScheme,
        transaction: Option<Transaction>,
        predecessor: &PredecessorIdentity,
    ) -> Result<ManifestLocation, CommitError> {
        self.check(manifest, path)?;
        self.delegate
            .commit_after(
                manifest,
                indices,
                path,
                store,
                writer,
                scheme,
                transaction,
                predecessor,
            )
            .await
    }
    async fn forget_version(
        &self,
        path: &Path,
        version: u64,
        identity: &str,
    ) -> lance_core::Result<()> {
        self.delegate.forget_version(path, version, identity).await
    }
    async fn delete(&self, path: &Path) -> lance_core::Result<()> {
        self.delegate.delete(path).await
    }
}

fn missing_version(error: &lance::Error) -> bool {
    matches!(
        error,
        lance::Error::DatasetNotFound { .. } | lance::Error::VersionNotFound { .. }
    )
}

// checkout resolves/loads a manifest; unlike transaction recovery it does not read external transaction objects.
fn missing_checkout_manifest(error: &lance::Error) -> bool {
    missing_version(error) || matches!(error, lance::Error::NotFound { .. })
}
