use super::*;
use crate::{
    memory_store::{
        embeddings::{
            SourceOwner, capture as shared_capture, publish as shared_publish,
            query::EmbeddingScan, source_at,
        },
        lexical_search::HistoryView,
    },
    user_memory_store::{native, store},
};
use graphit_auth::UserWriteAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_data::lance_branch_store::{BranchError, DeltaResolution, PreparedDelta};
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};

pub(super) async fn commit(
    prepared: PreparedDelta,
    owner: &UserMemoryOwner,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<UserMemoryEmbeddingAttachOutcome, MemoryStoreError> {
    let recovery_key = prepared.recovery_key().clone();
    before()?;
    let pin = prepared.commit().await?;
    let publication = UserMemoryEmbeddingPublication {
        generation: native::generation(&pin, owner),
        recovery_key,
    };
    before().map_err(|cause| MemoryStoreError::UserEmbeddingConfirmed {
        publication: Box::new(publication.clone()),
        cause: Box::new(cause),
    })?;
    Ok(UserMemoryEmbeddingAttachOutcome::Published(publication))
}
impl NativeUserMemoryStore {
    pub async fn attach_embeddings(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        capture: &UserMemoryEmbeddingCorpus,
        batch: Vec<MemoryEmbeddingAttachment>,
        limits: MemoryStoreWriteLimits,
    ) -> Result<UserMemoryEmbeddingAttachOutcome, MemoryStoreError> {
        native::check_write(original, admission)?;
        if !Arc::ptr_eq(original, &capture.state.original) {
            return Err(MemoryStoreError::DifferentRequest);
        }
        let scope = admission.user()?.live;
        let owner = native::owner(scope)?;
        if owner != capture.state.generation.owner || scope.runtime_root() != capture.state.root {
            return Err(MemoryStoreError::WrongAdmission);
        }
        capture.state.read.guard.revalidate(original)?;
        if batch.is_empty() {
            return Err(MemoryStoreError::Invalid(
                "empty Memory embedding attachment",
            ));
        }
        let mut bytes = 0;
        let mut unique = HashSet::new();
        let mut grouped: BTreeMap<MemoryId, Vec<MemoryEmbeddingAttachment>> = BTreeMap::new();
        for attachment in batch {
            native::check_write(original, admission)?;
            capture.state.read.guard.revalidate(original)?;
            let key = (attachment.memory_id, attachment.revision.get());
            if !unique.insert(key) {
                return Err(MemoryStoreError::Invalid(
                    "duplicate Memory embedding attachment",
                ));
            }
            if capture.state.emitted.get(&key) != Some(&attachment.input_fingerprint) {
                return Err(MemoryStoreError::Invalid(
                    "unemitted Memory embedding attachment",
                ));
            }
            if attachment.vector.len() != capture.identity().dimensions().get() as usize
                || attachment.vector.iter().any(|v| !v.is_finite())
            {
                return Err(MemoryStoreError::Invalid("Memory embedding vector"));
            }
            charge(
                &mut bytes,
                attachment
                    .vector
                    .len()
                    .checked_mul(4)
                    .and_then(|n| n.checked_add(56))
                    .ok_or(MemoryStoreError::Limit("embedding input bytes"))?,
                limits.max_input_bytes.get(),
                "embedding input bytes",
            )?;
            grouped
                .entry(attachment.memory_id)
                .or_default()
                .push(attachment);
        }
        let writer = native::acquire(original, admission, limits, false)
            .await?
            .ok_or(MemoryStoreError::IncompleteCorpus)?;
        if writer.guard.corpus_key(writer.pin.corpus_uri())
            != capture
                .state
                .read
                .guard
                .corpus_key(capture.state.read.pin.corpus_uri())
        {
            return Err(MemoryStoreError::Invalid(
                "Memory embedding storage changed during capture",
            ));
        }
        let before = || {
            writer.check(original, admission)?;
            capture.state.read.guard.revalidate(original)?;
            Ok(())
        };
        before()?;
        shared_capture::discover_ids(
            &writer.pin,
            original,
            SourceOwner::User(&owner),
            limits.read,
            &before,
        )
        .await?;
        let scan = EmbeddingScan {
            pin: &writer.pin,
            original,
            owner: SourceOwner::User(&owner),
            limits: limits.read,
        };
        let mut updates = Vec::new();
        for (id, attachments) in grouped {
            before()?;
            let chain = store::chain(&writer.pin, original, &owner, id, limits.read, &before)
                .await?
                .ok_or(MemoryStoreError::Invalid("orphan Memory embedding"))?;
            let view = HistoryView::User(&chain);
            let existing = shared_publish::existing_rows(
                &scan,
                view,
                &attachments,
                capture.identity(),
                &before,
            )
            .await?;
            for attachment in attachments {
                before()?;
                let source = source_at(view, attachment.revision.get())
                    .ok_or(MemoryStoreError::Invalid("orphan Memory embedding"))?;
                let document = prepare_memory_embedding_document(source, capture.identity())?;
                if document.fingerprint() != &attachment.input_fingerprint {
                    return Err(MemoryStoreError::Invalid(
                        "Memory embedding source fingerprint",
                    ));
                }
                if !existing.contains(&attachment.revision.get()) {
                    updates.push(encode_user_memory_embedding_row(
                        &owner,
                        &document,
                        &attachment.vector,
                        limits,
                    )?);
                }
            }
        }
        before()?;
        if updates.is_empty() {
            let result = UserMemoryEmbeddingAttachOutcome::Existing {
                generation: native::generation(&writer.pin, &owner),
            };
            before()?;
            return Ok(result);
        }
        let prepared = writer
            .pin
            .prepare_merge(updates, vec!["row_key".into()])
            .await?;
        commit(prepared, &owner, before).await
    }
    pub async fn resolve_embeddings(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        key: &DeltaRecoveryKey,
        limits: MemoryStoreWriteLimits,
    ) -> Result<DeltaResolution, MemoryStoreError> {
        native::check_write(original, admission)?;
        let corpus_uri = match admission.record_storage().await? {
            Some(storage) => storage.uri,
            None => native::uri(admission.user()?.live)?,
        };
        if key.corpus_uri() != corpus_uri || key.physical_branch().is_some() {
            return Err(BranchError::RecoveryBinding.into());
        }
        let Some(writer) = native::acquire(original, admission, limits, false).await? else {
            native::check_write(original, admission)?;
            return Ok(DeltaResolution::Unresolved);
        };
        writer.check(original, admission)?;
        let observed = writer.pin.resolve(key).await?;
        writer.check(original, admission)?;
        Ok(observed)
    }
}

#[cfg(test)]
#[path = "../../../tests/user_memory_embeddings/publication.rs"]
mod publication_tests;
