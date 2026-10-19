use super::{native, rows, *};
use crate::memory_store::{
    self as domain, MemoryLexicalQuery, MemoryStoreError, MemoryStoreReadLimits,
    embeddings::{SourceOwner, query::EmbeddingScan},
    lexical_search,
};
use graphit_auth::UserReadAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_data::lance_branch_store::JsonFtsIndexState;
use std::sync::Arc;

impl NativeUserMemoryStore {
    /// Same-pin JSON FTS search with complete validation of candidate histories.
    pub async fn search_lexical(
        original: &Arc<SnapshotContext>,
        admission: &UserReadAdmission<'_>,
        query: MemoryLexicalQuery,
        limits: MemoryStoreReadLimits,
    ) -> Result<UserMemoryLexicalSearchRead, MemoryStoreError> {
        native::check_read(original, admission)?;
        if let Some(tag) = &query.filter.tag {
            domain::nonblank(tag)?;
        }
        let expanded = lexical_search::expand(&query.text, limits.max_query_bytes.get())?;
        let Some(capture) = native::read_pin(original, admission, limits.native).await? else {
            native::check_read(original, admission)?;
            return Ok(UserMemoryLexicalSearchRead::NoCorpus);
        };
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, MemoryStoreError>(())
        };
        let generation = native::generation(&capture.pin, &capture.owner);
        if !lexical_search::has_memories(
            &capture.pin,
            original,
            rows::user_memory_schema().as_ref(),
            &before,
        )
        .await?
        {
            before()?;
            return Ok(UserMemoryLexicalSearchRead::PublishedEmpty { generation });
        }
        before()?;
        let state = capture.pin.json_fts_index_state("payload").await?;
        before()?;
        if state != JsonFtsIndexState::Ready {
            return Err(MemoryStoreError::LexicalIndexUnavailable(state));
        }
        let (total_hits, hits) = lexical_search::search_candidates(
            &EmbeddingScan {
                pin: &capture.pin,
                original,
                owner: SourceOwner::User(&capture.owner),
                limits,
            },
            expanded,
            &query,
            &before,
        )
        .await?;
        before()?;
        Ok(UserMemoryLexicalSearchRead::Published {
            generation,
            total_hits,
            hits,
        })
    }
}
