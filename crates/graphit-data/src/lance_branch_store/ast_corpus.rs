//! Local AST search corpus maintenance. Source and the record stores keep their history.
use super::{BranchSnapshot, PreparedIndex};
use graphit_core::file_io::SnapshotContext;
use lance::{
    Dataset,
    dataset::{
        cleanup::CleanupPolicy,
        optimize::{CompactionOptions, compact_files},
    },
    index::{CreateIndexBuilder, DatasetIndexExt, DatasetIndexInternalExt},
};
use lance_index::{
    IndexType,
    optimize::OptimizeOptions,
    scalar::{BuiltinIndexType, InvertedIndexParams, ScalarIndexParams},
};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions, TryLockError},
    io,
    path::Path,
    sync::Arc,
    time::Duration,
};

const LOOKUPS: &[&str] = &["path", "uid", "data_key"];
const TEXT: &[&str] = &["name", "content"];
pub const AST_INDEX_BATCH: usize = 32;
const SEGMENT_ROWS: u64 = 65_536;

fn lock_file(path: &Path) -> io::Result<File> {
    let mut file = OpenOptions::new();
    file.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        file.mode(0o600);
    }
    file.open(path.join(".graphit-corpus.lock"))
}

impl BranchSnapshot {
    /// Retain a shared lease across clones and historical pins of the local AST corpus.
    pub async fn capture_ast_corpus(
        path: &Path,
        branch: Option<&str>,
        original: &SnapshotContext,
    ) -> io::Result<Self> {
        original.check()?;
        let uri = path.to_str().ok_or(io::ErrorKind::InvalidData)?;
        match std::fs::symlink_metadata(path.join(".graphit-corpus.lock")) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                // A read of an absent/empty corpus must not create storage.
                // Only first use needs this SDK existence check. Capture again
                // under the shared lock so cleanup cannot invalidate the reader.
                Self::capture_observed(uri, branch, None)
                    .await
                    .map_err(io::Error::other)?;
                original.check()?;
            }
            Err(error) => return Err(error),
            Ok(_) => (),
        }
        let file = lock_file(path)?;
        loop {
            match file.try_lock_shared() {
                Ok(()) => break,
                Err(TryLockError::Error(error)) => return Err(error),
                Err(TryLockError::WouldBlock) => {
                    tokio::select! {
                        () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
                        () = tokio::time::sleep(Duration::from_millis(20)) => original.check()?,
                    }
                }
            }
        }
        let mut snapshot = Self::capture_observed(uri, branch, None)
            .await
            .map_err(io::Error::other)?;
        original.check()?;
        snapshot.ast_reader = Some(Arc::new(file));
        Ok(snapshot)
    }

    /// Seed the code analyzer and lookup indexes before the first FILE is
    /// published. Existing corpora only build indexes that are absent.
    pub async fn prepare_ast_indices(&self) -> io::Result<Option<PreparedIndex>> {
        if self.inherited_target().is_some() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let indices = self
            .dataset
            .load_indices()
            .await
            .map_err(io::Error::other)?;
        let code = InvertedIndexParams::code().max_token_length(None);
        let btree = ScalarIndexParams::for_builtin(BuiltinIndexType::BTree);
        let mut built = Vec::new();
        let fields = TEXT
            .iter()
            .map(|field| (*field, format!("{field}_code_fts"), IndexType::Inverted))
            .chain(
                LOOKUPS
                    .iter()
                    .map(|field| (*field, format!("{field}_entity"), IndexType::BTree)),
            );
        let mut captured = self.dataset.as_ref().clone();
        for (field, name, kind) in fields {
            if indices.iter().any(|index| index.name == name) {
                continue;
            }
            let params: &dyn lance_index::IndexParams = if kind == IndexType::Inverted {
                &code
            } else {
                &btree
            };
            built.push(
                CreateIndexBuilder::new(&mut captured, &[field], kind, params)
                    .name(name)
                    .execute_uncommitted()
                    .await
                    .map_err(io::Error::other)?,
            );
        }
        if built.is_empty() {
            return Ok(None);
        }
        self.prepare_index_publication(built, Vec::new())
            .map(Some)
            .map_err(io::Error::other)
    }

    /// Reconcile accumulated fragments after publication. The caller serializes
    /// maintenance with writers on this physical branch. Reads may keep older pins.
    pub async fn maintain_ast_indices(
        &self,
        original: &SnapshotContext,
        recheck: &impl Fn() -> io::Result<()>,
    ) -> io::Result<bool> {
        original.check()?;
        let indices = self
            .dataset
            .load_indices()
            .await
            .map_err(io::Error::other)?;
        let mut names = BTreeMap::<&str, usize>::new();
        for index in indices.iter() {
            let small = index.fragment_bitmap.as_ref().is_none_or(|fragments| {
                fragments
                    .iter()
                    .filter_map(|id| self.dataset.get_fragment(id as usize))
                    .try_fold(0usize, |total, fragment| {
                        let total =
                            total.saturating_add(fragment.metadata().num_rows().unwrap_or(0));
                        (total < SEGMENT_ROWS as usize / 2).then_some(total)
                    })
                    .is_some()
            });
            *names.entry(&index.name).or_default() += usize::from(small);
        }
        let merge = if names.values().any(|&small| small >= AST_INDEX_BATCH) {
            AST_INDEX_BATCH
        } else {
            1
        };
        let mut due = merge > 1
            || self.dataset.count_fragments() >= AST_INDEX_BATCH
                && LOOKUPS
                    .iter()
                    .any(|field| !names.contains_key(format!("{field}_entity").as_str()));
        if !due {
            due = self
                .dataset
                .manifest()
                .fragments
                .iter()
                .filter(|fragment| fragment.deletion_file.is_some())
                .take(AST_INDEX_BATCH)
                .count()
                == AST_INDEX_BATCH;
        }
        for name in names.keys() {
            if due
                || self
                    .dataset
                    .unindexed_fragments(name)
                    .await
                    .map_err(io::Error::other)?
                    .len()
                    >= AST_INDEX_BATCH
            {
                due = true;
                break;
            }
        }
        if !due {
            return Ok(false);
        }
        recheck()?;
        let mut snapshot = self.clone();
        if let Some(initial) = self.prepare_ast_indices().await? {
            recheck()?;
            snapshot = initial.commit().await.map_err(io::Error::other)?;
        }
        let mut dataset = snapshot.dataset.as_ref().clone();
        // Compact small fragments in one pass and let the SDK remap retained
        // indexes. Segment sizes keep later incremental merges local.
        let options = CompactionOptions {
            target_rows_per_fragment: SEGMENT_ROWS as usize,
            num_threads: Some(1),
            ..CompactionOptions::default()
        };
        original.check()?;
        recheck()?;
        compact_files(&mut dataset, options, None)
            .await
            .map_err(io::Error::other)?;
        original.check()?;
        recheck()?;
        dataset
            .optimize_indices(
                &OptimizeOptions::merge(merge)
                    .max_rows_per_task(Some(SEGMENT_ROWS))
                    .max_rows_per_segment(Some(SEGMENT_ROWS)),
            )
            .await
            .map_err(io::Error::other)?;
        recheck()?;
        Ok(dataset.version_id() != self.version())
    }

    pub async fn ast_history_due(&self) -> io::Result<bool> {
        self.dataset
            .count_versions()
            .await
            .map(|versions| versions >= AST_INDEX_BATCH as u64)
            .map_err(io::Error::other)
    }
}

#[derive(Debug, serde::Serialize)]
pub struct AstCorpusStats {
    pub branch: Option<String>,
    pub head: u64,
    pub manifests: u64,
    pub fragments: usize,
    pub index_segments: usize,
    pub rows: usize,
}

async fn stats(dataset: &Dataset) -> io::Result<AstCorpusStats> {
    Ok(AstCorpusStats {
        branch: dataset.manifest().branch.clone(),
        head: dataset.version_id(),
        manifests: dataset.count_versions().await.map_err(io::Error::other)?,
        fragments: dataset.count_fragments(),
        index_segments: dataset
            .load_indices()
            .await
            .map_err(io::Error::other)?
            .len(),
        rows: dataset.count_rows(None).await.map_err(io::Error::other)?,
    })
}

/// Trim the corpus through the SDK. A busy corpus is left intact. Unverified
/// files retain the SDK safety window. Tags and descendant file references are
/// observed by Lance, and branch heads are verified after the pass.
pub async fn cleanup_ast_corpus(
    path: &Path,
    original: &SnapshotContext,
) -> io::Result<Option<Vec<(AstCorpusStats, AstCorpusStats)>>> {
    original.check()?;
    if !std::fs::symlink_metadata(path)?.is_dir() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let lock = lock_file(path)?;
    match lock.try_lock() {
        Ok(()) => (),
        Err(TryLockError::WouldBlock) => return Ok(None),
        Err(TryLockError::Error(error)) => return Err(error),
    }
    let uri = path.to_str().ok_or(io::ErrorKind::InvalidData)?;
    let root = Dataset::open(uri).await.map_err(io::Error::other)?;
    let branches = root.branches().list().await.map_err(io::Error::other)?;
    // Clean each named branch before its parent. The SDK retains base files
    // still referenced by surviving descendants, even after deleting a fork
    // manifest. Sorting by lineage depth is independent of branch-name syntax.
    let mut ordered = branches
        .keys()
        .map(|name| {
            let mut depth = 0;
            let mut parent = Some(name.as_str());
            while let Some(key) = parent {
                depth += 1;
                if depth > branches.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "cyclic Lance branch lineage",
                    ));
                }
                parent = branches
                    .get(key)
                    .and_then(|entry| entry.parent_branch.as_deref());
            }
            Ok((depth, name))
        })
        .collect::<io::Result<Vec<_>>>()?;
    ordered.sort_unstable_by(|a, b| b.cmp(a));
    let mut observed = Vec::with_capacity(ordered.len() + 1);
    for branch in ordered
        .into_iter()
        .map(|(_, name)| Some(name.as_str()))
        .chain([None])
    {
        original.check()?;
        let dataset = match branch {
            Some(branch) => root
                .checkout_branch(branch)
                .await
                .map_err(io::Error::other)?,
            None => root.clone(),
        };
        let before = stats(&dataset).await?;
        if before.manifests > 1 {
            dataset
                .cleanup(CleanupPolicy {
                    before_version: Some(dataset.version_id()),
                    error_if_tagged_old_versions: false,
                    ..CleanupPolicy::default()
                })
                .execute()
                .await
                .map_err(io::Error::other)?;
        }
        // Verify the persisted head through a new SDK handle, rather than the
        // manifest retained by the cleanup caller's snapshot.
        let reopened = Dataset::open(uri).await.map_err(io::Error::other)?;
        let reopened = match branch {
            Some(branch) => reopened
                .checkout_branch(branch)
                .await
                .map_err(io::Error::other)?,
            None => reopened,
        };
        let after = stats(&reopened).await?;
        if before.head != after.head || before.rows != after.rows {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Lance branch head changed during cleanup",
            ));
        }
        observed.push((before, after));
    }
    original.check()?;
    Ok(Some(observed))
}
