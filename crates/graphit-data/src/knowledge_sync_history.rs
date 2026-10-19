//! Committed synchronizations from Native transactions, including an initial
//! branch Clone's referenced Main create transaction.
//!
//! This reader scans attached transaction metadata, never historical datasets or
//! document bodies. The SDK transiently materializes a Vec of lightweight refs.
use crate::{
    knowledge_index::{
        self as index, KnowledgeGeneration, KnowledgeIndexError, KnowledgeIndexReadLimits,
    },
    lance_module_store::StoreError,
};
use arrow_array::{RecordBatch, StringArray, UInt8Array};
use graphit_auth::ProjectReadAdmission;
use graphit_core::{artifact_store::StoreOwner, file_io::SnapshotContext};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
    future::Future,
    num::NonZeroUsize,
    sync::Arc,
};

pub(crate) const SYNC_PROPERTY: &str = "graphit.knowledge.sync.v1";

/// Actual changed slugs; counts derive from these lists. No document text is kept.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeSyncChanges {
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub removed: Vec<String>,
}
impl KnowledgeSyncChanges {
    pub(crate) fn into_property(mut self) -> Result<String, KnowledgeIndexError> {
        self.added.sort_unstable();
        self.updated.sort_unstable();
        self.removed.sort_unstable();
        serde_json::to_string(&self)
            .map_err(|e| KnowledgeIndexError::Request(std::io::Error::other(e)))
    }
    pub(crate) fn into_properties(
        self,
    ) -> Result<Arc<HashMap<String, String>>, KnowledgeIndexError> {
        Ok(Arc::new(HashMap::from([(
            SYNC_PROPERTY.into(),
            self.into_property()?,
        )])))
    }
    /// The non-stream first producer already owns its validated physical image.
    /// Read just its slug/state columns, without copying or scanning source text.
    pub(crate) fn first_batch(
        batch: &RecordBatch,
        before: &impl Fn() -> Result<(), KnowledgeIndexError>,
    ) -> Result<Self, KnowledgeIndexError> {
        let slugs: &StringArray = index::array(batch, 0)?;
        let states: &UInt8Array = index::array(batch, 16)?;
        let mut added = Vec::new();
        added.try_reserve_exact(batch.num_rows()).map_err(|e| {
            KnowledgeIndexError::Request(std::io::Error::new(std::io::ErrorKind::OutOfMemory, e))
        })?;
        for row in 0..batch.num_rows() {
            before()?;
            if states.value(row)
                == crate::knowledge_references::KnowledgeDocumentState::Active as u8
            {
                added.push(slugs.value(row).to_owned());
            }
        }
        Ok(Self {
            added,
            ..Self::default()
        })
    }
    fn validate(
        &self,
        limits: KnowledgeIndexReadLimits,
        before: &impl Fn() -> Result<(), KnowledgeIndexError>,
    ) -> Result<(), KnowledgeIndexError> {
        let mut seen = HashSet::new();
        for slug in self.added.iter().chain(&self.updated).chain(&self.removed) {
            before()?;
            index::validate_slug(slug)?;
            if slug.len() > limits.content.max_identifier_bytes.get() {
                return Err(KnowledgeIndexError::LimitExceeded("identifier_bytes"));
            }
            if !seen.insert(slug.as_str()) {
                return Err(KnowledgeIndexError::InvalidField("sync_duplicate_slug"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct KnowledgeSyncHistoryQuery {
    pub limit: NonZeroUsize,
    pub cursor: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct KnowledgeSyncHistoryEntry {
    pub version: u64,
    /// The SDK commit timestamp in RFC3339 UTC, not a duplicated property field.
    pub timestamp: String,
    pub changes: KnowledgeSyncChanges,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum KnowledgeSyncHistoryRead {
    NoCorpus,
    Published {
        generation: KnowledgeGeneration,
        entries: Vec<KnowledgeSyncHistoryEntry>,
        next_cursor: Option<String>,
    },
}
#[derive(Debug)]
pub enum KnowledgeSyncHistoryError {
    Data(KnowledgeIndexError),
    InvalidCursor,
    CursorBinding,
    HistoryChanged,
    MalformedProperty(serde_json::Error),
}
impl KnowledgeSyncHistoryError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Data(_) => "knowledge_sync_data",
            Self::InvalidCursor => "invalid_cursor",
            Self::CursorBinding => "cursor_binding",
            Self::HistoryChanged => "history_changed",
            Self::MalformedProperty(_) => "malformed_property",
        }
    }
}
impl fmt::Display for KnowledgeSyncHistoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Knowledge synchronization history: {}", self.code())
    }
}
impl Error for KnowledgeSyncHistoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Data(e) => Some(e),
            Self::MalformedProperty(e) => Some(e),
            _ => None,
        }
    }
}
impl From<KnowledgeIndexError> for KnowledgeSyncHistoryError {
    fn from(e: KnowledgeIndexError) -> Self {
        Self::Data(e)
    }
}

// A cursor retains observations only. It grants no authority, holds no SDK owner
// and does not expose a physical locator or a recovery key.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    project: String,
    selection: String,
    corpus: [u8; 32],
    physical_branch: Option<String>,
    inherited_target: Option<String>,
    cutoff: u64,
    cutoff_transaction: Option<String>,
    before: u64,
    next: u64,
    next_transaction: Option<String>,
    limit: usize,
}
async fn guarded<T>(
    future: impl Future<Output = T>,
    original: &SnapshotContext,
    before: &impl Fn() -> Result<(), KnowledgeIndexError>,
) -> Result<T, KnowledgeIndexError> {
    before()?;
    let deadline = original
        .required_deadline()
        .map_err(KnowledgeIndexError::Request)?;
    let result = tokio::select! {
        value = future => Ok(value),
        _ = original.cancellation().cancelled() => {
            Err(KnowledgeIndexError::Request(std::io::ErrorKind::Interrupted.into()))
        }
        _ = tokio::time::sleep_until(deadline.into()) => {
            Err(KnowledgeIndexError::Request(std::io::ErrorKind::TimedOut.into()))
        }
    };
    // Revalidate the same authority/capture at the SDK boundary and preserve
    // Original's causal cancellation/deadline error if a signal won the race.
    before()?;
    result
}

/// Read one admitted page, newest first, from actual attached version references.
/// Every call captures and reauthorizes the selected PROJECT/version/branch.
/// Commits after the first page's cutoff are excluded. Removed/cleaned metadata
/// and malformed properties are errors, never a fabricated empty page or EOF.
pub async fn read_native_knowledge_sync_history(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    query: KnowledgeSyncHistoryQuery,
    limits: KnowledgeIndexReadLimits,
) -> Result<KnowledgeSyncHistoryRead, KnowledgeSyncHistoryError> {
    index::check_read(original, admission)?;
    original
        .required_deadline()
        .map_err(KnowledgeIndexError::Request)?;
    let previous: Option<Cursor> = query
        .cursor
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| KnowledgeSyncHistoryError::InvalidCursor)?;
    let project = admission.project().map_err(KnowledgeIndexError::from)?;
    let scope = project.scope.live;
    let corpus = index::local_corpus(scope, project.version.version(), original, limits.native)?;
    let corpus_binding: [u8; 32] = Sha256::digest(corpus.uri.as_bytes()).into();
    let StoreOwner::Project { project_id } = scope.owner() else {
        return Err(KnowledgeIndexError::WrongAdmission.into());
    };
    let project_id = project_id.to_string();
    let selection = project.version.version().to_owned();
    let branch = project.version.lance_branch();
    if let Some(cursor) = &previous {
        if cursor.project != project_id
            || cursor.selection != selection
            || cursor.corpus != corpus_binding
            || cursor.limit != query.limit.get()
        {
            return Err(KnowledgeSyncHistoryError::CursorBinding);
        }
        if cursor.before == 0
            || cursor.before > cursor.cutoff
            || cursor.next == 0
            || cursor.next >= cursor.before
        {
            return Err(KnowledgeSyncHistoryError::InvalidCursor);
        }
    }
    let Some(guard) = index::capture_index(scope, &corpus, original, limits.native, || {
        index::check_read(original, admission)
    })?
    else {
        index::check_read(original, admission)?;
        return if previous.is_some() {
            Err(KnowledgeSyncHistoryError::HistoryChanged)
        } else {
            Ok(KnowledgeSyncHistoryRead::NoCorpus)
        };
    };
    let before = || {
        index::check_read(original, admission)?;
        guard.revalidate(original)?;
        Ok(())
    };
    let uri = corpus.uri.clone();
    let pin = guarded(
        crate::lance_branch_store::BranchSnapshot::capture_observed(
            &uri,
            (!branch.is_empty()).then_some(branch.as_str()),
            None,
        ),
        original,
        &before,
    )
    .await?
    .map_err(KnowledgeIndexError::from)?;
    index::semantic::dimensions(pin.schema())?;
    let mut generation = index::generation(&uri, &pin);
    if let Some(cursor) = &previous
        && (cursor.physical_branch != generation.physical_branch
            || cursor.inherited_target != generation.inherited_target)
    {
        return Err(KnowledgeSyncHistoryError::CursorBinding);
    }
    let cutoff = previous.as_ref().map_or(pin.version(), |c| c.cutoff);
    let refs = guarded(pin.dataset().version_refs(), original, &before)
        .await?
        .map_err(StoreError::from)
        .map_err(KnowledgeIndexError::from)?;
    if !refs.iter().any(|r| r.version == cutoff)
        || previous.as_ref().is_some_and(|c| {
            !refs.iter().any(|r| r.version == c.before) || !refs.iter().any(|r| r.version == c.next)
        })
    {
        return Err(KnowledgeSyncHistoryError::HistoryChanged);
    }
    let cutoff_record = guarded(
        pin.dataset().read_version_transaction(cutoff),
        original,
        &before,
    )
    .await?
    .map_err(StoreError::from)
    .map_err(KnowledgeIndexError::from)?;
    if cutoff_record.version != cutoff {
        return Err(KnowledgeSyncHistoryError::HistoryChanged);
    }
    let cutoff_transaction = cutoff_record.transaction.as_ref().map(|t| t.uuid.clone());
    if previous
        .as_ref()
        .is_some_and(|c| c.cutoff_transaction != cutoff_transaction)
    {
        return Err(KnowledgeSyncHistoryError::HistoryChanged);
    }
    let mut cutoff_record = previous.is_none().then_some(cutoff_record);
    let mut entries = Vec::new();
    let mut continuation = None;
    for reference in refs.into_iter().rev() {
        before()?;
        if reference.version > cutoff
            || previous
                .as_ref()
                .is_some_and(|c| reference.version >= c.before)
        {
            continue;
        }
        let record = if reference.version == cutoff {
            cutoff_record.take().expect("observed cutoff metadata")
        } else {
            guarded(
                pin.dataset().read_version_transaction(reference.version),
                original,
                &before,
            )
            .await?
            .map_err(StoreError::from)
            .map_err(KnowledgeIndexError::from)?
        };
        if record.version != reference.version {
            return Err(KnowledgeSyncHistoryError::HistoryChanged);
        }
        if previous.as_ref().is_some_and(|c| {
            reference.version == c.next
                && record.transaction.as_ref().map(|tx| &tx.uuid) != c.next_transaction.as_ref()
        }) {
            return Err(KnowledgeSyncHistoryError::HistoryChanged);
        }
        // The SDK's branch Clone has no user transaction properties. A first
        // publication inherits Main version 1, whose real create transaction
        // already records the complete initial synchronization. Read only that
        // immutable metadata at the same authorized table, never a foreign URI.
        let initial = if let Some(transaction) = record.transaction.as_ref()
            && !transaction
                .transaction_properties
                .as_ref()
                .is_some_and(|p| p.contains_key(SYNC_PROPERTY))
            && let lance::dataset::transaction::Operation::Clone {
                is_shallow: true,
                ref_name: None,
                ref_version: 1,
                ref_path,
                branch_name: Some(branch),
            } = &transaction.operation
            && Some(branch.as_str()) == pin.physical_branch()
            && *ref_path
                == pin
                    .dataset()
                    .branch_location()
                    .find_main()
                    .map_err(StoreError::from)
                    .map_err(KnowledgeIndexError::from)?
                    .uri
        {
            let parent = guarded(
                pin.dataset()
                    .checkout_version(lance::dataset::refs::Ref::Version(None, Some(1))),
                original,
                &before,
            )
            .await?
            .map_err(StoreError::from)
            .map_err(KnowledgeIndexError::from)?;
            Some(
                guarded(parent.read_version_transaction(1), original, &before)
                    .await?
                    .map_err(StoreError::from)
                    .map_err(KnowledgeIndexError::from)?,
            )
        } else {
            None
        };
        let Some(value) = record
            .transaction
            .as_ref()
            .and_then(|t| t.transaction_properties.as_ref())
            .and_then(|properties| properties.get(SYNC_PROPERTY))
            .or_else(|| {
                initial
                    .as_ref()
                    .and_then(|record| record.transaction.as_ref())
                    .and_then(|t| t.transaction_properties.as_ref())
                    .and_then(|p| p.get(SYNC_PROPERTY))
            })
        else {
            continue;
        };
        let changes: KnowledgeSyncChanges =
            serde_json::from_str(value).map_err(KnowledgeSyncHistoryError::MalformedProperty)?;
        changes.validate(limits, &before)?;
        if entries.len() == query.limit.get() {
            continuation = Some((
                record.version,
                record.transaction.as_ref().map(|tx| tx.uuid.clone()),
            ));
            break;
        }
        entries.push(KnowledgeSyncHistoryEntry {
            version: record.version,
            timestamp: record.timestamp.to_rfc3339(),
            changes,
        });
    }
    let next_cursor = if let Some((next, next_transaction)) = continuation {
        let position = entries
            .last()
            .expect("nonzero page limit with lookahead")
            .version;
        Some(
            serde_json::to_string(&Cursor {
                project: project_id,
                selection,
                corpus: corpus_binding,
                physical_branch: generation.physical_branch.clone(),
                inherited_target: generation.inherited_target.clone(),
                cutoff,
                cutoff_transaction,
                before: position,
                next,
                next_transaction,
                limit: query.limit.get(),
            })
            .map_err(|_| KnowledgeSyncHistoryError::InvalidCursor)?,
        )
    } else {
        None
    };
    generation.version = cutoff;
    before()?;
    Ok(KnowledgeSyncHistoryRead::Published {
        generation,
        entries,
        next_cursor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::{
        future::pending,
        io,
        time::{Duration, Instant},
    };

    #[tokio::test]
    async fn knowledge_sync_history_original_interrupts_pending_read_without_polling() {
        let cancellation = Cancellation::default();
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            cancellation.clone(),
        );
        let cause = guarded(
            async {
                cancellation.cancel();
                pending::<()>().await
            },
            &original,
            &|| original.check().map_err(KnowledgeIndexError::Request),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(cause, KnowledgeIndexError::Request(error) if error.kind() == io::ErrorKind::Interrupted)
        );

        let original = SnapshotContext::new(
            Instant::now() + Duration::from_millis(1),
            Cancellation::default(),
        );
        let cause = guarded(pending::<()>(), &original, &|| {
            original.check().map_err(KnowledgeIndexError::Request)
        })
        .await
        .unwrap_err();
        assert!(
            matches!(cause, KnowledgeIndexError::Request(error) if error.kind() == io::ErrorKind::TimedOut)
        );
    }
}
