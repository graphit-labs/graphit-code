//! Closed Knowledge synchronization-history DTOs shared by actual Root and Role readers.
use graphit_data::{
    knowledge_index::KnowledgeIndexError,
    knowledge_sync_history::{
        KnowledgeSyncHistoryError, KnowledgeSyncHistoryQuery, KnowledgeSyncHistoryRead,
    },
};
use graphit_hub::native::ContentLimits;
use serde::Deserialize;
use serde_json::{Value, json};
use std::num::NonZeroUsize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Input {
    pub limit: NonZeroUsize,
    #[serde(default, deserialize_with = "present")]
    pub cursor: Option<String>,
}
fn present<'de, T: Deserialize<'de>, D: serde::Deserializer<'de>>(
    input: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(input).map(Some)
}
impl Input {
    pub(crate) fn validate(&self, content: ContentLimits) -> Result<(), Value> {
        if self.limit.get() > content.max_collection_entries
            || self
                .cursor
                .as_ref()
                .is_some_and(|c| c.is_empty() || c.len() > content.max_input_bytes)
        {
            return Err(json!({"code":"invalid_arguments"}));
        }
        Ok(())
    }
    pub(crate) fn native(self) -> KnowledgeSyncHistoryQuery {
        KnowledgeSyncHistoryQuery {
            limit: self.limit,
            cursor: self.cursor,
        }
    }
}
pub(crate) fn view(read: KnowledgeSyncHistoryRead) -> Value {
    match read {
        KnowledgeSyncHistoryRead::NoCorpus => json!({"kind":"no_corpus"}),
        KnowledgeSyncHistoryRead::Published {
            generation,
            entries,
            next_cursor,
        } => json!({
            "kind":"published",
            "generation":{"version":generation.version,"physical_branch":generation.physical_branch,"inherited_target":generation.inherited_target},
            "entries":entries,"next_cursor":next_cursor,
        }),
    }
}
pub(crate) fn failure(cause: KnowledgeSyncHistoryError) -> Value {
    match cause {
        KnowledgeSyncHistoryError::Data(KnowledgeIndexError::Auth(_)) => {
            json!({"code":"authentication_or_permission_denied"})
        }
        KnowledgeSyncHistoryError::Data(KnowledgeIndexError::Request(cause)) => {
            json!({"code":"request_failed","io_kind":format!("{:?}",cause.kind())})
        }
        other => json!({"code":other.code()}),
    }
}
