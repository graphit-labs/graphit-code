//! Git commit references are immutable native Lance tags, not copied datasets.

use super::{BranchError, BranchSnapshot, PreparedDelta, Ref, checked_schema};
use arrow_schema::Schema;
use lance::dataset::transaction::{Operation, Transaction};
use std::sync::Arc;

fn tag_name(commit: &str) -> Result<String, BranchError> {
    if !matches!(commit.len(), 40 | 64) || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(BranchError::InvalidGitObjectId);
    }
    Ok(format!("git-{}", commit.to_ascii_lowercase()))
}

impl BranchSnapshot {
    /// Protect this exact physical branch/version from native Lance cleanup.
    /// Repeating the operation is harmless only for the same snapshot. Existing
    /// tags are never moved, including when a different branch has the same data.
    /// The caller owns authorization and serialization with its corpus writer.
    pub async fn tag_git_commit(&self, commit: &str) -> Result<Self, BranchError> {
        let tag = tag_name(commit)?;
        let tags = self.dataset.tags();
        match tags.get(&tag).await {
            Ok(existing) => {
                if existing.branch.as_deref() == self.physical_branch()
                    && existing.version == self.version()
                {
                    return Ok(self.clone());
                }
                return Err(BranchError::GitSnapshotConflict);
            }
            Err(lance::Error::RefNotFound { .. }) => (),
            Err(error) => return Err(error.into()),
        }
        match tags
            .create(
                &tag,
                Ref::Version(
                    self.physical_branch().map(str::to_owned),
                    Some(self.version()),
                ),
            )
            .await
        {
            Ok(()) => Ok(self.clone()),
            Err(lance::Error::RefConflict { .. }) => {
                // Concurrent identical publication is idempotent; a competing
                // different snapshot never replaces the winner's native tag.
                let existing = tags.get(&tag).await?;
                if existing.branch.as_deref() == self.physical_branch()
                    && existing.version == self.version()
                {
                    Ok(self.clone())
                } else {
                    Err(BranchError::GitSnapshotConflict)
                }
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Reopen a Git tag in this already admitted corpus. Missing commits return
    /// `None`; no branch or latest snapshot is substituted. The tag retains the
    /// branch-specific version and schema, independently of this reader's pin.
    pub async fn at_git_commit(&self, commit: &str) -> Result<Option<Self>, BranchError> {
        let tag = tag_name(commit)?;
        let contents = match self.dataset.tags().get(&tag).await {
            Ok(contents) => contents,
            Err(lance::Error::RefNotFound { .. }) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let dataset = self.dataset.checkout_version(Ref::Tag(tag)).await?;
        if dataset.version_id() != contents.version || dataset.manifest().branch != contents.branch
        {
            return Err(BranchError::GitSnapshotConflict);
        }
        Ok(Some(Self {
            uri: self.uri.clone(),
            schema: Arc::new(Schema::from(dataset.schema())),
            dataset: Arc::new(dataset),
            delegate: Arc::clone(&self.delegate),
            inherited_target: None,
            ast_reader: self.ast_reader.clone(),
        }))
    }

    /// Create a new named branch from the exact native Git snapshot, sharing its
    /// data/index files through Lance's shallow clone. An existing branch is a
    /// conflict; this operation never resets it or copies the corpus.
    pub async fn materialize_git_branch(
        &self,
        branch: &str,
        commit: &str,
    ) -> Result<Self, BranchError> {
        let source = self
            .at_git_commit(commit)
            .await?
            .ok_or(BranchError::GitSnapshotMissing)?;
        match source.observe_bootstrap(branch).await {
            super::BootstrapObservation::Absent => (),
            observation => return Err(observation.into_error(None)),
        }
        let mut dataset = source.dataset.as_ref().clone();
        let reference = Ref::Version(
            source.physical_branch().map(str::to_owned),
            Some(source.version()),
        );
        let dataset = match dataset.create_branch(branch, reference, None).await {
            Ok(dataset) => dataset,
            Err(cause) => {
                return Err(source
                    .observe_bootstrap(branch)
                    .await
                    .into_error(Some(Box::new(cause))));
            }
        };
        checked_schema(&dataset, &source.schema)?;
        if dataset.version_id() != source.version()
            || dataset.manifest().branch.as_deref() != Some(branch)
        {
            return Err(BranchError::GitSnapshotConflict);
        }
        Ok(Self {
            uri: source.uri,
            schema: source.schema,
            dataset: Arc::new(dataset),
            delegate: source.delegate,
            inherited_target: None,
            ast_reader: source.ast_reader,
        })
    }

    /// Prepare a same-physical-branch restore without rewriting rows or files.
    /// Publication uses the existing zero-retry optimistic commit/recovery path.
    /// A cross-branch restore must instead apply the caller's record delta; the
    /// SDK's Restore operation addresses only this branch's version history.
    pub async fn prepare_restore_git_commit(
        &self,
        commit: &str,
    ) -> Result<PreparedDelta, BranchError> {
        if self.inherited_target.is_some() {
            return Err(BranchError::InheritedSnapshot);
        }
        let source = self
            .at_git_commit(commit)
            .await?
            .ok_or(BranchError::GitSnapshotMissing)?;
        if source.physical_branch() != self.physical_branch() {
            return Err(BranchError::GitSnapshotBranchMismatch);
        }
        self.prepared_delta(
            Transaction::new(
                self.version(),
                Operation::Restore {
                    version: source.version(),
                },
                None,
            ),
            None,
        )
    }
}
