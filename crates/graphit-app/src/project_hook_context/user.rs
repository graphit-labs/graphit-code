//! Source retains USER authority and reads; formatting consumes only its transient data.
use graphit_records::{memory_store::Memory, user_memory_store::UserMemoryListRead};

pub(crate) enum UserContext<'a> {
    Unavailable,
    Captured(&'a UserMemoryListRead),
}

pub(super) fn projection(read: &UserMemoryListRead) -> (&'static str, &[Memory]) {
    match read {
        UserMemoryListRead::NoCorpus => ("no_corpus", &[]),
        UserMemoryListRead::Published { memories, .. } => (
            if memories.is_empty() {
                "empty"
            } else {
                "loaded"
            },
            memories,
        ),
    }
}
