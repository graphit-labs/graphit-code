use super::*;
use crate::{
    memory_store::{
        embeddings::{SourceOwner, capture as shared_capture},
        lexical_search::HistoryView,
    },
    user_memory_store::{native, store},
};
use graphit_auth::UserReadAdmission;
use graphit_core::file_io::SnapshotContext;
use std::{collections::HashMap, path::PathBuf, sync::Arc};

pub enum UserMemoryEmbeddingCorpusRead {
    NoCorpus,
    Published(UserMemoryEmbeddingCorpus),
}
pub struct UserMemoryEmbeddingCorpus {
    pub(in crate::user_memory_store) state: Box<CorpusState>,
}
pub(in crate::user_memory_store) struct CorpusState {
    pub original: Arc<SnapshotContext>,
    pub read: native::ReadPin,
    pub root: PathBuf,
    pub generation: UserMemoryGeneration,
    pub identity: MemoryEmbeddingIdentity,
    pub limits: MemoryStoreReadLimits,
    pub ids: Vec<MemoryId>,
    pub emitted: HashMap<(MemoryId, u64), [u8; 32]>,
    next_id: usize,
    next_revision: u64,
}
impl UserMemoryEmbeddingCorpus {
    /// Whether the captured corpus contains no logical Memory IDs.
    pub fn is_empty(&self) -> bool {
        self.state.ids.is_empty()
    }
    pub fn generation(&self) -> &UserMemoryGeneration {
        &self.state.generation
    }
    pub fn identity(&self) -> &MemoryEmbeddingIdentity {
        &self.state.identity
    }
    pub(in crate::user_memory_store) fn check_read(
        &self,
        original: &Arc<SnapshotContext>,
        admission: &UserReadAdmission<'_>,
    ) -> Result<(), MemoryStoreError> {
        if !Arc::ptr_eq(original, &self.state.original) {
            return Err(MemoryStoreError::DifferentRequest);
        }
        native::check_read(original, admission)?;
        let scope = admission.user()?.live;
        if native::owner(scope)? != self.state.generation.owner
            || scope.runtime_root() != self.state.root
        {
            return Err(MemoryStoreError::WrongAdmission);
        }
        self.state.read.guard.revalidate(original)?;
        Ok(())
    }
    pub(in crate::user_memory_store) async fn renew_read(
        &mut self,
        original: &Arc<SnapshotContext>,
        admission: &UserReadAdmission<'_>,
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
        admission: &UserReadAdmission<'_>,
    ) -> Result<Option<Vec<MemoryEmbeddingDocument>>, MemoryStoreError> {
        self.renew_read(original, admission).await?;
        let mut documents = Vec::new();
        let mut next_id = self.state.next_id;
        let mut next_revision = self.state.next_revision;
        let chunk_rows = self.state.limits.batch_rows.get() as usize;
        while next_id < self.state.ids.len() && documents.len() < chunk_rows {
            let id = self.state.ids[next_id];
            let before = || self.check_read(original, admission);
            let chain = store::chain(
                &self.state.read.pin,
                original,
                &self.state.generation.owner,
                id,
                self.state.limits,
                &before,
            )
            .await?
            .ok_or(MemoryStoreError::Invalid("orphan Memory embedding"))?;
            let view = HistoryView::User(&chain);
            let covered = shared_capture::validated_chain_embeddings(
                &crate::memory_store::embeddings::query::EmbeddingScan {
                    pin: &self.state.read.pin,
                    original,
                    owner: SourceOwner::User(&self.state.generation.owner),
                    limits: self.state.limits,
                },
                id,
                view,
                Some(&self.state.identity),
                &before,
            )
            .await?;
            let mut last_revision = next_revision;
            let mut complete = true;
            for memory in view.sources() {
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
            before()?;
            next_revision = last_revision;
            if complete {
                next_id += 1;
                next_revision = 0;
            }
        }
        let result = (!documents.is_empty()).then_some(documents);
        self.check_read(original, admission)?;
        self.state.next_id = next_id;
        self.state.next_revision = next_revision;
        if let Some(documents) = &result {
            for document in documents {
                self.state.emitted.insert(
                    (document.memory_id(), document.revision()),
                    *document.fingerprint(),
                );
            }
        }
        Ok(result)
    }
}
impl NativeUserMemoryStore {
    pub async fn capture_embeddings(
        original: &Arc<SnapshotContext>,
        admission: &UserReadAdmission<'_>,
        identity: MemoryEmbeddingIdentity,
        limits: MemoryStoreReadLimits,
    ) -> Result<UserMemoryEmbeddingCorpusRead, MemoryStoreError> {
        let Some(read) = native::read_pin(original, admission, limits.native).await? else {
            native::check_read(original, admission)?;
            return Ok(UserMemoryEmbeddingCorpusRead::NoCorpus);
        };
        let before = || {
            native::check_read(original, admission)?;
            read.guard.revalidate(original)?;
            Ok(())
        };
        let ids = shared_capture::discover_ids(
            &read.pin,
            original,
            SourceOwner::User(&read.owner),
            limits,
            &before,
        )
        .await?;
        let generation = native::generation(&read.pin, &read.owner);
        let root = admission.user()?.live.runtime_root().to_owned();
        before()?;
        let corpus = UserMemoryEmbeddingCorpus {
            state: Box::new(CorpusState {
                original: Arc::clone(original),
                read,
                root,
                generation,
                identity,
                limits,
                ids,
                emitted: HashMap::new(),
                next_id: 0,
                next_revision: 0,
            }),
        };
        corpus.check_read(original, admission)?;
        Ok(UserMemoryEmbeddingCorpusRead::Published(corpus))
    }
}
