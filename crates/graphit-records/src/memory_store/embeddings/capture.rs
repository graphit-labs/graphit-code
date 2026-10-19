use super::super::{
    MemoryGeneration, MemoryId, MemoryStoreError, MemoryStoreReadLimits, NativeMemoryStore,
    history, native, rows,
};
use super::{
    MemoryEmbeddingDocument, MemoryEmbeddingIdentity, SourceOwner, metadata,
    prepare_memory_embedding_document, validate_source,
};
use crate::memory_store::lexical_search::HistoryView;
use arrow_array::{Array, Float32Array, ListArray};
use futures::TryStreamExt;
use graphit_auth::ProjectReadAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_data::{lance_branch_store::BranchSnapshot, lance_module_store::StoreError};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
};

pub enum MemoryEmbeddingCorpusRead {
    NoCorpus,
    Published(MemoryEmbeddingCorpus),
}
/// Captured read state with immutable source/configuration binding and a private document cursor.
pub struct MemoryEmbeddingCorpus {
    pub(in crate::memory_store) state: Box<CorpusState>,
}
pub(in crate::memory_store) struct CorpusState {
    pub original: Arc<SnapshotContext>,
    pub read: native::ReadPin,
    pub generation: MemoryGeneration,
    pub identity: MemoryEmbeddingIdentity,
    pub limits: MemoryStoreReadLimits,
    pub ids: Vec<MemoryId>,
    pub emitted: HashMap<(MemoryId, u64), [u8; 32]>,
    next_id: usize,
    next_revision: u64,
}
impl MemoryEmbeddingCorpus {
    pub fn generation(&self) -> &MemoryGeneration {
        &self.state.generation
    }
    pub fn identity(&self) -> &MemoryEmbeddingIdentity {
        &self.state.identity
    }

    /// Whether this captured snapshot contains no Memory IDs.
    /// Independent of whether any revisions still need embeddings.
    pub fn is_empty(&self) -> bool {
        self.state.ids.is_empty()
    }

    pub(in crate::memory_store) fn check_read(
        &self,
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
    ) -> Result<(), MemoryStoreError> {
        if !Arc::ptr_eq(original, &self.state.original) {
            return Err(MemoryStoreError::DifferentRequest);
        }
        original.check()?;
        if admission.module() != graphit_auth::ProjectReadModule::Memory {
            return Err(MemoryStoreError::WrongAdmission);
        }
        let (project, request) = admission.project_and_request()?;
        if !Arc::ptr_eq(original, request.snapshot()) {
            return Err(MemoryStoreError::DifferentRequest);
        }
        if native::project_id(project.scope.live)? != self.state.generation.project_id
            || project.version.version() != self.state.generation.project_version
        {
            return Err(MemoryStoreError::Invalid(
                "Memory embedding capture binding",
            ));
        }
        self.state.read.guard.revalidate(original)?;
        Ok(())
    }

    pub(in crate::memory_store) async fn renew_read(
        &mut self,
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
    ) -> Result<(), MemoryStoreError> {
        self.check_read(original, admission)?;
        if self.state.read.guard.transport_expired() {
            let storage = admission
                .record_storage()
                .await?
                .ok_or(MemoryStoreError::Invalid(
                    "Memory embedding storage changed during capture",
                ))?;
            let renewed = crate::storage::renew_snapshot(
                original,
                &self.state.read.pin,
                &self.state.read.guard,
                storage,
            )
            .await;
            self.check_read(original, admission)?;
            let (pin, guard) = renewed?;
            self.state.read.pin = pin;
            self.state.read.guard = guard;
        }
        self.check_read(original, admission)
    }

    pub async fn next_documents(
        &mut self,
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
    ) -> Result<Option<Vec<MemoryEmbeddingDocument>>, MemoryStoreError> {
        self.renew_read(original, admission).await?;
        let mut documents = Vec::new();
        let mut next_id = self.state.next_id;
        let mut next_revision = self.state.next_revision;
        let chunk_rows = self.state.limits.batch_rows.get() as usize;
        while next_id < self.state.ids.len() && documents.len() < chunk_rows {
            let id = self.state.ids[next_id];
            let before = || self.check_read(original, admission);
            let chain = history::chain(
                &self.state.read.pin,
                original,
                id,
                self.state.generation.project_id,
                self.state.limits,
                before,
            )
            .await?
            .ok_or(MemoryStoreError::Invalid("orphan Memory embedding"))?;
            let covered = validated_chain_embeddings(
                &super::query::EmbeddingScan {
                    pin: &self.state.read.pin,
                    original,
                    owner: SourceOwner::Project(self.state.generation.project_id),
                    limits: self.state.limits,
                },
                id,
                HistoryView::Project(&chain),
                Some(&self.state.identity),
                before,
            )
            .await?;
            let mut last_revision = next_revision;
            let mut complete = true;
            for memory in std::iter::once(&chain.creation.memory).chain(
                chain
                    .revisions
                    .iter()
                    .filter(|event| !event.operation.deleted())
                    .map(|event| &event.after),
            ) {
                original.check()?;
                if memory.revision <= next_revision {
                    continue;
                }
                if covered.revisions.contains(&memory.revision) {
                    last_revision = memory.revision;
                    continue;
                }
                if documents.len() == chunk_rows {
                    complete = false;
                    break;
                }
                let mut document = prepare_memory_embedding_document(memory, &self.state.identity)?;
                document.reusable_vector = covered
                    .vectors
                    .get(document.fingerprint())
                    .map(|vector| vector.values().to_vec());
                last_revision = document.revision();
                documents.push(document);
            }
            self.check_read(original, admission)?;
            next_revision = last_revision;
            if complete {
                next_id += 1;
                next_revision = 0;
            }
        }
        self.check_read(original, admission)?;
        self.state.next_id = next_id;
        self.state.next_revision = next_revision;
        for document in &documents {
            self.state.emitted.insert(
                (document.memory_id(), document.revision()),
                *document.fingerprint(),
            );
        }
        if documents.is_empty() {
            Ok(None)
        } else {
            Ok(Some(documents))
        }
    }
}

impl NativeMemoryStore {
    pub async fn capture_embeddings(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        identity: MemoryEmbeddingIdentity,
        limits: MemoryStoreReadLimits,
    ) -> Result<MemoryEmbeddingCorpusRead, MemoryStoreError> {
        let Some(read) = native::read_pin(original, admission, limits.native).await? else {
            native::check_read(original, admission)?;
            return Ok(MemoryEmbeddingCorpusRead::NoCorpus);
        };
        let project = admission.project()?;
        let project_id = native::project_id(project.scope.live)?;
        let before = || {
            native::check_read(original, admission)?;
            read.guard.revalidate(original)?;
            Ok::<_, MemoryStoreError>(())
        };
        let ids = discover_ids(
            &read.pin,
            original,
            SourceOwner::Project(project_id),
            limits,
            before,
        )
        .await?;
        before()?;
        let generation = native::generation(&read.pin, project_id, project.version.version());
        Ok(MemoryEmbeddingCorpusRead::Published(
            MemoryEmbeddingCorpus {
                state: Box::new(CorpusState {
                    original: Arc::clone(original),
                    read,
                    generation,
                    identity,
                    limits,
                    ids,
                    emitted: HashMap::new(),
                    next_id: 0,
                    next_revision: 0,
                }),
            },
        ))
    }
}

pub(crate) async fn discover_ids(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    owner: SourceOwner<'_>,
    limits: MemoryStoreReadLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Vec<MemoryId>, MemoryStoreError> {
    before()?;
    let mut scanner = pin.scan();
    scanner
        .batch_size(limits.batch_rows.get() as usize)
        .fragment_readahead(1)
        .batch_readahead(1);
    let mut stream = crate::storage::read(original, scanner.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    before()?;
    let mut ids = BTreeSet::new();
    let mut keys = HashSet::new();
    loop {
        before()?;
        let batch = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = batch else {
            break;
        };
        rows::validate_schema(&batch, owner.schema().as_ref())?;
        for row in 0..batch.num_rows() {
            original.check()?;

            if !keys.insert(rows::text(&batch, 0, row)?.to_owned()) {
                return Err(MemoryStoreError::Invalid("duplicate Memory row key"));
            }
            owner.decode(&batch, row, original)?;
            ids.insert(rows::text(&batch, 2, row)?.parse()?);
        }
    }
    before()?;
    Ok(ids.into_iter().collect())
}

pub(crate) struct ChainEmbeddings {
    pub(crate) revisions: HashSet<u64>,
    pub(crate) vectors: HashMap<[u8; 32], Float32Array>,
}

pub(crate) async fn validated_chain_embeddings(
    scan: &super::query::EmbeddingScan<'_>,
    id: MemoryId,
    chain: HistoryView<'_>,
    identity: Option<&MemoryEmbeddingIdentity>,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<ChainEmbeddings, MemoryStoreError> {
    before()?;
    let mut scanner = scan.pin.scan();
    scanner
        .filter(&format!("record_id = {}", native::literal(&id.to_string())))
        .map_err(StoreError::from)?
        .batch_size(scan.limits.batch_rows.get() as usize)
        .fragment_readahead(1)
        .batch_readahead(1);
    let mut stream = crate::storage::read(scan.original, scanner.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    before()?;
    let mut keys = HashSet::new();
    let mut covered = HashSet::new();
    let mut vectors = HashMap::new();
    loop {
        before()?;
        let batch = crate::storage::read(scan.original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = batch else {
            break;
        };
        rows::validate_schema(&batch, scan.owner.schema().as_ref())?;
        for row in 0..batch.num_rows() {
            scan.original.check()?;

            if !keys.insert(rows::text(&batch, 0, row)?.to_owned()) {
                return Err(MemoryStoreError::Invalid("duplicate Memory row key"));
            }
            if !scan.owner.decode(&batch, row, scan.original)?
                && rows::text(&batch, 1, row)? == "embedding"
            {
                let meta = metadata(&batch, row, scan.owner.layout())?;
                validate_source(&meta, chain)?;
                if identity == Some(&meta.identity) {
                    covered.insert(meta.revision);
                    if let std::collections::hash_map::Entry::Vacant(entry) =
                        vectors.entry(meta.fingerprint)
                    {
                        let lists = batch
                            .column(scan.owner.column(23))
                            .as_any()
                            .downcast_ref::<ListArray>()
                            .ok_or(MemoryStoreError::Invalid("Memory embedding list"))?;
                        let values = lists.value(row);
                        let values = values
                            .as_any()
                            .downcast_ref::<Float32Array>()
                            .ok_or(MemoryStoreError::Invalid("Memory embedding values"))?;
                        entry.insert(values.clone());
                    }
                }
            }
        }
    }
    before()?;
    Ok(ChainEmbeddings {
        revisions: covered,
        vectors,
    })
}
