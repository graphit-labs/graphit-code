//! Incremental semantic projections in the authoritative Task table.
//! Specifications and comments remain immediately searchable through FTS. Derived
//! vectors are bound to the public text, independently of claims and timestamps.
use super::{
    NativeTaskStore, TaskGeneration, TaskLexicalHit, TaskLexicalMatchLocation, TaskLexicalQuery,
    TaskLexicalSearchRead, TaskSearchMode, TaskStoreError, TaskStoreReadLimits,
    TaskStoreWriteLimits, model::id, native, public_search, read, rows, write,
};
use crate::task_spec::TaskId;
use arrow_array::{
    ListArray, RecordBatch,
    builder::{Float32Builder, ListBuilder},
};
use futures::TryStreamExt;
use graphit_auth::{ProjectReadAdmission, ProjectWriteAdmission, ProjectWriteOperation};
use graphit_core::file_io::SnapshotContext;
use graphit_data::{lance_branch_store::BranchSnapshot, lance_module_store::StoreError};
use lance::dataset::scanner::DatasetRecordBatchStream;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    num::{NonZeroU32, NonZeroUsize},
    sync::Arc,
};

pub const TASK_DOCUMENT_RECIPE: &str = "task-public-document-json-v1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskEmbeddingIdentity {
    vector_id: [u8; 32],
    dimensions: NonZeroU32,
    recipe: String,
}
impl TaskEmbeddingIdentity {
    pub fn new(
        vector_id: [u8; 32],
        dimensions: NonZeroU32,
        recipe: String,
    ) -> Result<Self, TaskStoreError> {
        if recipe != TASK_DOCUMENT_RECIPE {
            return Err(TaskStoreError::Invalid("Task embedding recipe"));
        }
        Ok(Self {
            vector_id,
            dimensions,
            recipe,
        })
    }
    pub fn vector_id(&self) -> &[u8; 32] {
        &self.vector_id
    }
    pub fn dimensions(&self) -> NonZeroU32 {
        self.dimensions
    }
    pub fn recipe(&self) -> &str {
        &self.recipe
    }
}

/// Owned inference input. Only its association metadata is persisted with a vector;
/// the public source text already belongs to its specification/comment row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskEmbeddingDocument {
    pub(super) task_id: TaskId,
    pub(super) source_key: String,
    location: TaskLexicalMatchLocation,
    fingerprint: [u8; 32],
    identity: TaskEmbeddingIdentity,
    #[serde(skip)]
    text: String,
}
impl TaskEmbeddingDocument {
    pub fn task_id(&self) -> TaskId {
        self.task_id
    }
    pub fn source_key(&self) -> &str {
        &self.source_key
    }
    pub fn location(&self) -> TaskLexicalMatchLocation {
        self.location
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn into_text(self) -> String {
        self.text
    }
    pub fn take_text(&mut self) -> String {
        std::mem::take(&mut self.text)
    }
    pub fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }
    pub fn identity(&self) -> &TaskEmbeddingIdentity {
        &self.identity
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredTaskEmbedding {
    pub document: TaskEmbeddingDocument,
}
impl StoredTaskEmbedding {
    pub(super) fn validate(&self) -> Result<(), TaskStoreError> {
        let d = &self.document;
        if d.identity.recipe != TASK_DOCUMENT_RECIPE || !d.text.is_empty() {
            return Err(TaskStoreError::Invalid("Task embedding vector"));
        }
        let expected = match d.location {
            TaskLexicalMatchLocation::Specification { revision } if revision > 0 => {
                format!("task/{}", id(d.task_id.get()))
            }
            TaskLexicalMatchLocation::Comment { sequence, .. } if sequence > 0 => {
                format!("task_comment/{}/{sequence}", id(d.task_id.get()))
            }
            _ => return Err(TaskStoreError::Invalid("Task embedding source location")),
        };
        if d.source_key != expected {
            return Err(TaskStoreError::Invalid("Task embedding source key"));
        }
        Ok(())
    }
}

pub enum TaskEmbeddingCorpusRead {
    NoCorpus,
    Published(Box<TaskEmbeddingCorpus>),
}
/// One captured native source scan. Only association metadata and one source
/// batch survive between inference calls; no document corpus or vectors do.
pub struct TaskEmbeddingCorpus {
    original: Arc<SnapshotContext>,
    read: native::ReadPin,
    generation: TaskGeneration,
    project_id: graphit_core::project_identity::ProjectId,
    project_version: String,
    identity: TaskEmbeddingIdentity,
    limits: TaskStoreReadLimits,
    existing: HashMap<String, (TaskEmbeddingIdentity, [u8; 32])>,
    source_keys: HashSet<String>,
    stream: Option<DatasetRecordBatchStream>,
    batch: Option<RecordBatch>,
    next_row: usize,
    source_offset: u64,
    done: bool,
}
impl TaskEmbeddingCorpus {
    pub fn generation(&self) -> &TaskGeneration {
        &self.generation
    }
    pub fn identity(&self) -> &TaskEmbeddingIdentity {
        &self.identity
    }
    fn check_read(
        &self,
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
    ) -> Result<(), TaskStoreError> {
        if !Arc::ptr_eq(original, &self.original) {
            return Err(TaskStoreError::DifferentRequest);
        }
        native::check_read(original, admission)?;
        let project = admission.project()?;
        if native::project_id(project.scope.live)? != self.project_id
            || project.version.version() != self.project_version
        {
            return Err(TaskStoreError::Invalid("Task embedding capture binding"));
        }
        self.read.guard.revalidate(original)?;
        Ok(())
    }
    async fn renew(
        &mut self,
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
    ) -> Result<(), TaskStoreError> {
        if !self.read.guard.transport_expired() {
            return Ok(());
        }
        let storage = admission
            .record_storage()
            .await?
            .ok_or(TaskStoreError::Invalid(
                "Task embedding storage changed during capture",
            ))?;
        let (pin, guard) =
            crate::storage::renew_snapshot(original, &self.read.pin, &self.read.guard, storage)
                .await?;
        self.check_read(original, admission)?;
        self.read.pin = pin;
        self.read.guard = guard;
        // Reopen only the same pinned physical snapshot. SDK offset resumes the
        // ordered filtered scan, including an only-partly-consumed old batch.
        self.stream = None;
        self.batch = None;
        self.next_row = 0;
        Ok(())
    }
    async fn open_stream(
        &mut self,
        original: &Arc<SnapshotContext>,
        batch_size: NonZeroUsize,
    ) -> Result<(), TaskStoreError> {
        let mut scan = self.read.pin.scan();
        let base = rows::schema();
        let fields: Vec<_> = base.fields().iter().map(|f| f.name().as_str()).collect();
        scan.filter("kind IN ('task','task_comment')")
            .map_err(StoreError::from)?
            .project(&fields)
            .map_err(StoreError::from)?
            .batch_size(batch_size.get().min(self.limits.batch_rows.get() as usize))
            .fragment_readahead(1)
            .batch_readahead(1)
            .scan_in_order(true)
            .limit(
                None,
                Some(
                    i64::try_from(self.source_offset)
                        .map_err(|_| TaskStoreError::Invalid("Task source offset"))?,
                ),
            )
            .map_err(StoreError::from)?;
        self.stream = Some(
            crate::storage::read(original, scan.try_into_stream())
                .await?
                .map_err(StoreError::from)?,
        );
        Ok(())
    }
    /// Continue this snapshot to EOF exactly once. Fresh writes are checked by
    /// attach, and a stale source is left for its next ordinary record hint.
    pub async fn next_documents(
        &mut self,
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        batch_size: NonZeroUsize,
    ) -> Result<Option<Vec<TaskEmbeddingDocument>>, TaskStoreError> {
        self.check_read(original, admission)?;
        if self.done {
            return Ok(None);
        }
        self.renew(original, admission).await?;
        if self.stream.is_none() {
            self.open_stream(original, batch_size).await?;
        }
        let mut documents = Vec::new();
        while documents.len() < batch_size.get() {
            self.check_read(original, admission)?;
            if self
                .batch
                .as_ref()
                .is_none_or(|batch| self.next_row == batch.num_rows())
            {
                self.batch = None;
                self.next_row = 0;
                let next = crate::storage::read(
                    original,
                    self.stream
                        .as_mut()
                        .ok_or(TaskStoreError::Invalid("Task source stream"))?
                        .try_next(),
                )
                .await?;
                self.check_read(original, admission)?;
                match next {
                    Ok(Some(batch)) => self.batch = Some(batch),
                    Ok(None) => {
                        self.done = true;
                        self.stream = None;
                        self.existing.clear();
                        break;
                    }
                    Err(_) if self.read.guard.transport_expired() => {
                        self.renew(original, admission).await?;
                        self.open_stream(original, batch_size).await?;
                        continue;
                    }
                    Err(error) => return Err(StoreError::from(error).into()),
                }
            }
            let batch = self
                .batch
                .as_ref()
                .ok_or(TaskStoreError::Invalid("Task source batch"))?;
            if self.next_row == batch.num_rows() {
                continue;
            }
            let source = rows::decode(batch, self.next_row, original)?;
            self.next_row += 1;
            self.source_offset = self
                .source_offset
                .checked_add(1)
                .ok_or(TaskStoreError::Invalid("Task source offset"))?;
            let key = source.key();
            if !self.source_keys.insert(key.clone()) {
                return Err(TaskStoreError::Invalid("duplicate Task public source"));
            }
            let d = source_document(source, &self.identity)?;
            if self
                .existing
                .get(&key)
                .is_some_and(|e| e.0 == d.identity && e.1 == d.fingerprint)
            {
                continue;
            }
            documents.push(d);
        }
        self.check_read(original, admission)?;
        Ok((!documents.is_empty()).then_some(documents))
    }
}
pub struct TaskEmbeddingAttachment {
    pub document: TaskEmbeddingDocument,
    pub vector: Vec<f32>,
}
#[derive(Debug, Serialize)]
pub struct TaskEmbeddingPublication {
    pub generation: TaskGeneration,
    pub attached: usize,
    pub stale: usize,
    pub reused: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SemanticBinding {
    identity: TaskEmbeddingIdentity,
    query_vector: Vec<f32>,
}
impl super::TaskSearchCursor {
    /// Continuation data grants no access. The caller still admits its request,
    /// resolves the configured model, and the store validates the full binding.
    pub fn query_vector(
        &self,
        identity: &TaskEmbeddingIdentity,
    ) -> Result<Option<&[f32]>, TaskStoreError> {
        let Some(semantic) = &self.binding.semantic else {
            return if self.binding.mode == TaskSearchMode::Fts {
                Ok(None)
            } else {
                Err(TaskStoreError::InvalidCursor)
            };
        };
        if self.binding.mode == TaskSearchMode::Fts
            || semantic.identity != *identity
            || semantic.query_vector.len() != identity.dimensions.get() as usize
            || semantic.query_vector.iter().any(|v| !v.is_finite())
        {
            return Err(TaskStoreError::InvalidCursor);
        }
        Ok(Some(&semantic.query_vector))
    }
}

fn generation(pin: &BranchSnapshot) -> TaskGeneration {
    TaskGeneration {
        version: pin.version(),
        physical_branch: pin.physical_branch().map(str::to_owned),
        inherited_target: pin.inherited_target().map(str::to_owned),
    }
}
fn document(
    task_id: TaskId,
    source_key: String,
    location: TaskLexicalMatchLocation,
    text: String,
    identity: &TaskEmbeddingIdentity,
) -> Result<TaskEmbeddingDocument, TaskStoreError> {
    let fingerprint = Sha256::digest(serde_json::to_vec(&(identity, "Document", &text))?).into();
    Ok(TaskEmbeddingDocument {
        task_id,
        source_key,
        location,
        fingerprint,
        identity: identity.clone(),
        text,
    })
}
fn source_document(
    source: rows::Row,
    identity: &TaskEmbeddingIdentity,
) -> Result<TaskEmbeddingDocument, TaskStoreError> {
    let (task_id, location) = match &source {
        rows::Row::Task(t) => (
            t.record.specification.id,
            TaskLexicalMatchLocation::Specification {
                revision: t.record.specification.revision,
            },
        ),
        rows::Row::TaskComment(c) => (
            c.task_id,
            TaskLexicalMatchLocation::Comment {
                comment_id: c.id,
                sequence: c.sequence.get(),
            },
        ),
        _ => return Err(TaskStoreError::Invalid("Task public source family")),
    };
    document(
        task_id,
        source.key(),
        location,
        public_search::encode(&source)?,
        identity,
    )
}
/// Stream public heads/comments only. No task histories or corpus of document
/// bodies is retained; the callback can stop after its inference batch is full.
pub(super) async fn visit_documents(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    identity: &TaskEmbeddingIdentity,
    limits: TaskStoreReadLimits,
    selected: Option<&HashSet<String>>,
    before: impl Fn() -> Result<(), TaskStoreError>,
    mut visit: impl FnMut(TaskEmbeddingDocument) -> Result<bool, TaskStoreError>,
) -> Result<(), TaskStoreError> {
    before()?;
    let mut scan = pin.scan();
    let filter = if let Some(keys) = selected {
        if keys.is_empty() {
            return Ok(());
        }
        format!(
            "kind IN ('task','task_comment') AND row_key IN ({})",
            keys.iter()
                .map(|key| format!("'{}'", key.replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(",")
        )
    } else {
        "kind IN ('task','task_comment')".to_owned()
    };
    let base = rows::schema();
    let fields: Vec<_> = base.fields().iter().map(|f| f.name().as_str()).collect();
    scan.filter(&filter)
        .map_err(StoreError::from)?
        .project(&fields)
        .map_err(StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize)
        .fragment_readahead(1)
        .batch_readahead(1);
    let mut stream = crate::storage::read(original, scan.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    loop {
        before()?;
        let next = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = next else { break };
        for row in 0..batch.num_rows() {
            original.check()?;
            let source = rows::decode(&batch, row, original)?;
            let d = source_document(source, identity)?;
            if !visit(d)? {
                before()?;
                return Ok(());
            }
        }
    }
    before()
}
async fn visit_embeddings(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    limits: TaskStoreReadLimits,
    before: impl Fn() -> Result<(), TaskStoreError>,
    read_vectors: bool,
    selected: Option<&HashSet<String>>,
    mut visit: impl FnMut(
        StoredTaskEmbedding,
        Option<(&ListArray, usize)>,
    ) -> Result<(), TaskStoreError>,
) -> Result<(), TaskStoreError> {
    before()?;
    let mut scan = pin.scan();
    if !read_vectors {
        let base = rows::schema();
        let fields: Vec<_> = base.fields().iter().map(|f| f.name().as_str()).collect();
        scan.project(&fields).map_err(StoreError::from)?;
    }
    let filter = if let Some(keys) = selected {
        if keys.is_empty() {
            return Ok(());
        }
        format!(
            "kind = 'embedding' AND row_key IN ({})",
            keys.iter()
                .map(|key| format!("'embedding/{}'", key.replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(",")
        )
    } else {
        "kind = 'embedding'".to_owned()
    };
    scan.filter(&filter)
        .map_err(StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize)
        .fragment_readahead(1)
        .batch_readahead(1);
    let mut stream = crate::storage::read(original, scan.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    let mut keys = HashSet::new();
    loop {
        before()?;
        let next = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = next else { break };
        let vectors = if read_vectors {
            Some(
                batch
                    .column_by_name("embedding_vector")
                    .and_then(|c| c.as_any().downcast_ref::<ListArray>())
                    .ok_or(TaskStoreError::Invalid("Task embedding vector column"))?,
            )
        } else {
            None
        };
        for i in 0..batch.num_rows() {
            original.check()?;
            let rows::Row::Embedding(e) = rows::decode(&batch, i, original)? else {
                return Err(TaskStoreError::Invalid("Task embedding family"));
            };
            if !keys.insert(e.document.source_key.clone()) {
                return Err(TaskStoreError::Invalid("duplicate Task embedding"));
            }
            visit(e, vectors.map(|v| (v, i)))?;
        }
    }
    before()?;
    Ok(())
}
impl NativeTaskStore {
    /// Capture one source scan and existing association metadata. Model work is
    /// outside this read cursor; every attachment still acquires a fresh writer.
    pub async fn capture_embeddings(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        identity: TaskEmbeddingIdentity,
        limits: TaskStoreReadLimits,
    ) -> Result<TaskEmbeddingCorpusRead, TaskStoreError> {
        native::check_read(original, admission)?;
        let Some(read) = native::read_pin_with_parent_check(original, admission, limits).await?
        else {
            return Ok(TaskEmbeddingCorpusRead::NoCorpus);
        };
        let before = || {
            native::check_read(original, admission)?;
            read.guard.revalidate(original)?;
            Ok::<_, TaskStoreError>(())
        };
        let mut existing = HashMap::new();
        visit_embeddings(&read.pin, original, limits, &before, false, None, |e, _| {
            existing.insert(
                e.document.source_key,
                (e.document.identity, e.document.fingerprint),
            );
            Ok(())
        })
        .await?;
        before()?;
        let project = admission.project()?;
        let project_id = native::project_id(project.scope.live)?;
        let project_version = project.version.version().to_owned();
        Ok(TaskEmbeddingCorpusRead::Published(Box::new(
            TaskEmbeddingCorpus {
                original: Arc::clone(original),
                generation: generation(&read.pin),
                read,
                project_id,
                project_version,
                identity,
                limits,
                existing,
                source_keys: HashSet::new(),
                stream: None,
                batch: None,
                next_row: 0,
                source_offset: 0,
                done: false,
            },
        )))
    }
    /// No model work holds a writer. Recompute public fingerprints against this
    /// fresh admitted pin, then merge with the existing strict SDK CAS.
    pub async fn attach_embeddings(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        attachments: Vec<TaskEmbeddingAttachment>,
        limits: TaskStoreWriteLimits,
    ) -> Result<TaskEmbeddingPublication, TaskStoreError> {
        native::require_operation(admission, &[ProjectWriteOperation::Task])?;
        let capture = write::acquire_unmaterialized(original, admission, limits).await?;
        let before = || {
            native::check_write(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, TaskStoreError>(())
        };
        let keys = attachments
            .iter()
            .map(|a| a.document.source_key.clone())
            .collect::<HashSet<_>>();
        let mut stored = HashMap::new();
        visit_embeddings(
            &capture.pin,
            original,
            limits.read,
            &before,
            false,
            Some(&keys),
            |e, _| {
                stored.insert(
                    e.document.source_key,
                    (e.document.identity, e.document.fingerprint),
                );
                Ok(())
            },
        )
        .await?;
        let identity = attachments.first().map(|a| a.document.identity.clone());
        let mut current = HashMap::new();
        if let Some(identity) = &identity {
            visit_documents(
                &capture.pin,
                original,
                identity,
                limits.read,
                Some(&keys),
                &before,
                |mut d| {
                    d.text = String::new();
                    current.insert(d.source_key.clone(), d);
                    Ok(true)
                },
            )
            .await?;
        }
        let mut seen = HashSet::new();
        let mut rows = Vec::new();
        let mut vectors = Vec::new();
        let mut stale = 0;
        let mut reused = 0;
        for attachment in attachments {
            before()?;
            let d = attachment.document;
            if !seen.insert(d.source_key.clone()) {
                return Err(TaskStoreError::Invalid("duplicate Task attachment"));
            }
            if attachment.vector.len() != d.identity.dimensions.get() as usize
                || attachment.vector.iter().any(|v| !v.is_finite())
            {
                return Err(TaskStoreError::Invalid("Task embedding vector"));
            }
            if Some(&d.identity) != identity.as_ref() {
                return Err(TaskStoreError::Invalid("Task mixed attachment identities"));
            }
            let Some(mut current) = current
                .remove(&d.source_key)
                .filter(|v| v.fingerprint == d.fingerprint && v.task_id == d.task_id)
            else {
                stale += 1;
                continue;
            };
            if stored
                .remove(&d.source_key)
                .is_some_and(|e| e.0 == current.identity && e.1 == current.fingerprint)
            {
                reused += 1;
                continue;
            }
            current.text = String::new();
            rows.push(rows::Row::Embedding(StoredTaskEmbedding {
                document: current,
            }));
            vectors.push(attachment.vector);
        }
        let mut publication = TaskEmbeddingPublication {
            generation: generation(&capture.pin),
            attached: rows.len(),
            stale,
            reused,
        };
        before()?;
        if rows.is_empty() {
            return Ok(publication);
        }
        let mut capture = write::materialize(capture, original, admission, None).await?;
        capture.pin = capture
            .pin
            .add_nullable_columns(rows::vector_schema(), original)
            .await?;
        // Commit directly: derived-only rows do not require a redundant FTS rebuild.
        let before = || {
            native::check_write(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, TaskStoreError>(())
        };
        let base = rows::batch(&rows)?;
        let mut builder = ListBuilder::new(Float32Builder::new());
        for vector in vectors {
            builder.values().append_slice(&vector);
            builder.append(true);
        }
        let mut columns = base.columns().to_vec();
        columns.push(Arc::new(builder.finish()));
        let batch = RecordBatch::try_new(Arc::new(capture.pin.schema().clone()), columns)?;
        before()?;
        let pending = capture
            .pin
            .prepare_merge(vec![batch], vec!["row_key".into()])
            .await?;
        before()?;
        let committed = pending.commit().await?;
        publication.generation = generation(&committed);
        before().map_err(|cause| TaskStoreError::Confirmed {
            version: committed.version(),
            cause: Box::new(cause),
        })?;
        Ok(publication)
    }

    /// Exact cosine over currently compatible vectors; missing or obsolete
    /// vectors are skipped, never a corpus-wide barrier. Hybrid uses rank fusion.
    pub async fn search_semantic(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        identity: TaskEmbeddingIdentity,
        query_vector: &[f32],
        query: TaskLexicalQuery,
        limits: TaskStoreReadLimits,
        max_query_bytes: NonZeroUsize,
    ) -> Result<TaskLexicalSearchRead, TaskStoreError> {
        native::check_read(original, admission)?;
        let query_vector = match &query.cursor {
            Some(cursor) => cursor
                .query_vector(&identity)?
                .ok_or(TaskStoreError::InvalidCursor)?,
            None => query_vector,
        };
        if query.mode == TaskSearchMode::Fts
            || query.text.trim().is_empty()
            || query.text.len() > max_query_bytes.get()
            || query_vector.len() != identity.dimensions.get() as usize
            || query_vector.iter().any(|v| !v.is_finite())
        {
            return Err(TaskStoreError::Invalid("Task semantic query"));
        }
        let Some(mut capture) =
            native::read_pin_with_parent_check(original, admission, limits).await?
        else {
            if query.cursor.is_some() {
                return Err(TaskStoreError::InvalidCursor);
            }
            return Ok(TaskLexicalSearchRead::NoCorpus);
        };
        let binding = super::lexical_search::bind_pin(
            &mut capture,
            original,
            admission,
            &query,
            Some(SemanticBinding {
                identity: identity.clone(),
                query_vector: query_vector.to_vec(),
            }),
        )
        .await?;
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, TaskStoreError>(())
        };
        let summaries: HashMap<_, _> =
            read::head_summaries(&capture.pin, original, limits, &before)
                .await?
                .into_iter()
                .map(|s| (s.id, s))
                .collect();
        if summaries.is_empty() {
            if query.cursor.is_some() {
                return Err(TaskStoreError::InvalidCursor);
            }
            return Ok(TaskLexicalSearchRead::PublishedEmpty {
                generation: binding.generation,
            });
        }
        let mut current = HashMap::new();
        visit_documents(
            &capture.pin,
            original,
            &identity,
            limits,
            None,
            &before,
            |mut d| {
                d.text = String::new();
                current.insert(d.source_key.clone(), d);
                Ok(true)
            },
        )
        .await?;
        let mut selected: HashMap<TaskId, TaskLexicalHit> = HashMap::new();
        visit_embeddings(
            &capture.pin,
            original,
            limits,
            &before,
            true,
            None,
            |e, vector| {
                original.check()?;
                let Some(d) = current.get(&e.document.source_key).filter(|d| {
                    d.identity == e.document.identity && d.fingerprint == e.document.fingerprint
                }) else {
                    return Ok(());
                };
                let Some(summary) = summaries
                    .get(&d.task_id)
                    .filter(|s| query.filter.matches(s))
                else {
                    return Ok(());
                };
                let (vectors, row) =
                    vector.ok_or(TaskStoreError::Invalid("Task embedding vector column"))?;
                let distance = graphit_data::vector_distance::cosine_distances(
                    &vectors.slice(row, 1),
                    query_vector,
                )?;
                let score =
                    1. - distance[0].ok_or(TaskStoreError::Invalid("Task semantic null score"))?;
                if !score.is_finite() {
                    return Err(TaskStoreError::Invalid("Task semantic nonfinite score"));
                }
                let hit = TaskLexicalHit {
                    task: summary.clone(),
                    score,
                    location: d.location,
                };
                if selected.get(&d.task_id).is_none_or(|old| {
                    score > old.score
                        || (score == old.score
                            && super::lexical_search::prefer(d.location, old.location))
                }) {
                    selected.insert(d.task_id, hit);
                }
                Ok(())
            },
        )
        .await?;
        let mut hits: Vec<_> = selected.into_values().collect();
        if query.mode == TaskSearchMode::Hybrid {
            hits.sort_unstable_by(|a, b| {
                b.score
                    .total_cmp(&a.score)
                    .then_with(|| a.task.id.get().cmp(&b.task.id.get()))
            });
            let expanded = super::lexical_search::expand(&query.text, max_query_bytes.get())?;
            let mut lexical = super::lexical_search::collect_hits(
                &capture.pin,
                original,
                &summaries,
                &query,
                limits,
                expanded,
                &before,
            )
            .await?
            .ok_or(TaskStoreError::IncompleteCorpus)?;
            lexical.sort_unstable_by(|a, b| {
                b.score
                    .total_cmp(&a.score)
                    .then_with(|| a.task.id.get().cmp(&b.task.id.get()))
            });
            let mut fused: HashMap<TaskId, TaskLexicalHit> = HashMap::new();
            for ranked in [hits, lexical] {
                for (rank, mut hit) in ranked.into_iter().enumerate() {
                    let weight = 1. / (60. + rank as f32 + 1.);
                    if let Some(old) = fused.get_mut(&hit.task.id) {
                        old.score += weight;
                    } else {
                        hit.score = weight;
                        fused.insert(hit.task.id, hit);
                    }
                }
            }
            hits = fused.into_values().collect();
        }
        super::lexical_search::page(hits, &query, binding, before)
    }
}

#[cfg(all(test, target_os = "linux"))]
mod stream_boundary_tests {
    use super::super::{CreateCheck, CreateSession, CreateTask, test_fixture};
    use super::*;
    use crate::reference_projection::AuthoredReferencesField;
    use graphit_auth::{ProjectReadModule, ProjectRecordAuthority};
    use graphit_core::file_lock::Cancellation;

    #[tokio::test]
    async fn task_embedding_streams_recheck_authority_and_namespace_after_cpu_callbacks() {
        for namespace in [false, true] {
            let cancellation = Cancellation::default();
            let request = test_fixture::request(&cancellation);
            let original = request.snapshot();
            let fixture = test_fixture::Fixture::new(&request, "main");
            let issuer = fixture.issuer(&request);
            let (_registered, reference, _registration_run) = fixture.issue(&issuer, &request);
            let secret =
                std::fs::read_to_string(fixture.installation.owner_root().join("daemon/mcp.key"))
                    .unwrap();
            let caller = issuer.authenticate_runtime(&secret, &request).unwrap();
            let admin = issuer.bootstrap_admin(&caller).unwrap();
            let parent = issuer.admin_parent(&caller, &admin).unwrap();
            let scope = fixture.scope();
            let run = issuer
                .start_run(&parent, &scope, std::time::Duration::from_secs(3600))
                .unwrap();
            let bearer = issuer
                .issue(
                    &parent,
                    &run,
                    &reference,
                    &[
                        graphit_auth::Permission::Read(graphit_core::module_settings::Module::Task),
                        graphit_auth::Permission::WriteTask,
                        graphit_auth::Permission::WriteOwnTask,
                        graphit_auth::Permission::CoordinateTask,
                    ],
                    &scope,
                )
                .unwrap();
            let grant = issuer
                .verify(
                    bearer.expose_for_transport(),
                    &reference,
                    fixture.scope(),
                    &request,
                )
                .unwrap();
            let authority = ProjectRecordAuthority::new(&issuer);
            let read = authority
                .admit_read(&grant, &request, ProjectReadModule::Task)
                .unwrap();
            let write = authority
                .admit_write(&grant, &request, ProjectWriteOperation::Task)
                .unwrap();
            let session = NativeTaskStore::create_session(
                original,
                &write,
                CreateSession {
                    idempotency_key: "stream-boundary".into(),
                    title: "Session".into(),
                    description: "Boundary validation".into(),
                    strategy: "Read actual records".into(),
                },
                AuthoredReferencesField::Omitted,
                test_fixture::write_limits(),
            )
            .await
            .unwrap();
            NativeTaskStore::create_task(
                original,
                &write,
                CreateTask {
                    commit: None,
                    idempotency_key: "stream-boundary".into(),
                    session_id: session.id,
                    parent_id: None,
                    depends_on: vec![],
                    title: "Task".into(),
                    description: "Public document".into(),
                    task_type: "feature".into(),
                    priority: 2,
                    schedule: None,
                    checks: vec![
                        CreateCheck {
                            kind: crate::task_spec::CheckKind::Acceptance,
                            text: "Read the actual persisted document".into(),
                        },
                        CreateCheck {
                            kind: crate::task_spec::CheckKind::Test,
                            text: "Reject revoked authorization or a replaced namespace".into(),
                        },
                    ],
                },
                AuthoredReferencesField::Omitted,
                test_fixture::write_limits(),
            )
            .await
            .unwrap();
            let identity = TaskEmbeddingIdentity::new(
                [42; 32],
                NonZeroU32::new(2).unwrap(),
                TASK_DOCUMENT_RECIPE.into(),
            )
            .unwrap();
            if namespace {
                let TaskEmbeddingCorpusRead::Published(mut corpus) =
                    NativeTaskStore::capture_embeddings(
                        original,
                        &read,
                        identity.clone(),
                        test_fixture::read_limits(),
                    )
                    .await
                    .unwrap()
                else {
                    panic!("published")
                };
                let document = corpus
                    .next_documents(original, &read, NonZeroUsize::MIN)
                    .await
                    .unwrap()
                    .unwrap()
                    .pop()
                    .unwrap();
                NativeTaskStore::attach_embeddings(
                    original,
                    &write,
                    vec![TaskEmbeddingAttachment {
                        document,
                        vector: vec![1., 0.],
                    }],
                    test_fixture::write_limits(),
                )
                .await
                .unwrap();
            }
            let capture =
                native::read_pin_with_parent_check(original, &read, test_fixture::read_limits())
                    .await
                    .unwrap()
                    .unwrap();
            let before = || {
                native::check_read(original, &read)?;
                capture.guard.revalidate(original)?;
                Ok::<_, TaskStoreError>(())
            };
            let mut visited = false;
            let result = if namespace {
                visit_embeddings(
                    &capture.pin,
                    original,
                    test_fixture::read_limits(),
                    before,
                    true,
                    None,
                    |_, vector| {
                        assert!(vector.is_some());
                        let root = fixture.project.runtime_root();
                        std::fs::rename(root, root.with_extension("retired")).unwrap();
                        std::fs::create_dir(root).unwrap();
                        visited = true;
                        Ok(())
                    },
                )
                .await
            } else {
                visit_documents(
                    &capture.pin,
                    original,
                    &identity,
                    test_fixture::read_limits(),
                    None,
                    before,
                    |_| {
                        issuer.revoke(&parent, &bearer).unwrap();
                        visited = true;
                        Ok(true)
                    },
                )
                .await
            };
            assert!(
                visited,
                "the actual source/vector SDK stream reached its CPU callback"
            );
            assert!(
                matches!(
                    result,
                    Err(TaskStoreError::Auth(_)) | Err(TaskStoreError::Request(_))
                ),
                "boundary revocation must fail before delivery: {result:?}"
            );
        }
    }
}
