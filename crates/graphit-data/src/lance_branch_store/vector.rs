//! Native nullable-vector schema evolution on the exact captured branch.
use super::{BranchError, BranchSnapshot, StrictCommit};
use arrow_schema::{DataType, Schema};
use graphit_core::file_io::SnapshotContext;
use lance::{Dataset, dataset::NewColumnTransform, dataset::builder::DatasetBuilder};
use std::sync::Arc;

fn check(original: &SnapshotContext) -> Result<(), BranchError> {
    original
        .check()
        .map_err(|error| lance::Error::io_source(Box::new(error)).into())
}

impl BranchSnapshot {
    /// Add nullable corpus fields without rewriting existing fragments. Lance
    /// synthesizes NULL for old rows. Existing declarations must match exactly.
    /// The caller retains write admission and its publication fence; a stale
    /// pin cannot rebase this operation onto a newer one.
    pub async fn add_nullable_columns(
        &self,
        columns: Arc<Schema>,
        original: &Arc<SnapshotContext>,
    ) -> Result<Self, BranchError> {
        check(original)?;
        if self.inherited_target.is_some() {
            return Err(BranchError::InheritedSnapshot);
        }
        let mut names = std::collections::HashSet::new();
        let mut missing = Vec::new();
        for field in columns.fields() {
            if !field.is_nullable()
                || field.name().is_empty()
                || field.name().contains('\0')
                || !names.insert(field.name())
            {
                return Err(BranchError::SchemaMismatch);
            }
            match self.schema.field_with_name(field.name()) {
                Ok(existing) if existing == field.as_ref() => (),
                Ok(_) => return Err(BranchError::SchemaMismatch),
                Err(_) => missing.push(Arc::clone(field)),
            }
        }
        if missing.is_empty() {
            return Ok(self.clone());
        }
        let mut dataset = self.vector_evolution_dataset(original).await?;
        check(original)?;
        dataset
            .add_columns(
                NewColumnTransform::AllNulls(Arc::new(Schema::new(missing))),
                None,
                None,
            )
            .await?;
        // The SDK has confirmed the commit. Return its observed pin before the
        // owner performs post-effect Original/authority checks.
        self.evolved_vector_snapshot(dataset)
    }

    /// Withdraw an incompatible vector column using SDK metadata only. Text and
    /// its indices remain. Keep this returned pin before adding a new dimension:
    /// failure of a subsequent add does not undo this already committed effect.
    pub async fn drop_vector(
        &self,
        column: &str,
        original: &Arc<SnapshotContext>,
    ) -> Result<Self, BranchError> {
        check(original)?;
        let Ok(field) = self.schema.field_with_name(column) else {
            return Ok(self.clone());
        };
        if !matches!(field.data_type(), DataType::FixedSizeList(item, _) if item.data_type() == &DataType::Float32)
            || !field.is_nullable()
        {
            return Err(BranchError::SchemaMismatch);
        }
        let mut dataset = self.vector_evolution_dataset(original).await?;
        check(original)?;
        dataset.drop_columns(&[column]).await?;
        // The SDK has confirmed the commit. Return its observed pin before the
        // owner performs post-effect Original/authority checks.
        self.evolved_vector_snapshot(dataset)
    }

    async fn vector_evolution_dataset(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> Result<Dataset, BranchError> {
        if self.inherited_target.is_some() {
            return Err(BranchError::InheritedSnapshot);
        }
        let location = self.dataset.branch_location();
        let guard = Arc::new(StrictCommit {
            delegate: Arc::clone(&self.delegate),
            path: location.path,
            branch: location.branch,
            version: self
                .version()
                .checked_add(1)
                .ok_or(BranchError::InvalidVersion)?,
            original: Some(Arc::clone(original)),
        });
        // A child version need not exist in Main. Start from the captured
        // physical dataset URI before the SDK applies the explicit branch pin.
        let mut builder = DatasetBuilder::from_uri(self.dataset.uri())
            .with_session(self.dataset.session())
            .with_commit_handler(guard);
        if let Some(params) = self.dataset.store_params() {
            builder = builder.with_store_params(params.clone());
        }
        builder = match self.physical_branch() {
            Some(branch) => builder.with_branch(branch, Some(self.version())),
            None => builder.with_version(self.version()),
        };
        let dataset = builder.load().await?;
        if dataset.version_id() != self.version()
            || dataset.manifest().branch.as_deref() != self.physical_branch()
        {
            return Err(BranchError::RecoveryBinding);
        }
        check(original)?;
        Ok(dataset)
    }

    fn evolved_vector_snapshot(&self, dataset: Dataset) -> Result<Self, BranchError> {
        if dataset.version_id() != self.version() + 1
            || dataset.manifest().branch.as_deref() != self.physical_branch()
        {
            return Err(BranchError::InvalidVersion);
        }
        let mut snapshot = self.clone();
        snapshot.schema = Arc::new(Schema::from(dataset.schema()));
        snapshot.dataset = Arc::new(dataset);
        Ok(snapshot)
    }
}
