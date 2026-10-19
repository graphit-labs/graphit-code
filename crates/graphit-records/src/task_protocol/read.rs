use super::{ReadRequest, ReadResponse};
use crate::task_store::{NativeTaskStore, TaskStoreError, TaskStoreReadLimits};
use graphit_auth::ProjectReadAdmission;
use graphit_core::file_io::SnapshotContext;
use std::{num::NonZeroUsize, sync::Arc};

/// Dispatches against the caller's actual admission and original request.
pub async fn execute_read(
    request: ReadRequest,
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    limits: TaskStoreReadLimits,
    max_query_bytes: NonZeroUsize,
) -> Result<ReadResponse, TaskStoreError> {
    Ok(match request {
        ReadRequest::GetTask(input) => ReadResponse::GetTask(
            Box::pin(NativeTaskStore::read_task(
                original, admission, input, limits,
            ))
            .await?
            .map(Box::new),
        ),
        ReadRequest::GetSession(input) => ReadResponse::GetSession(
            Box::pin(NativeTaskStore::read_session(
                original, admission, input, limits,
            ))
            .await?
            .map(Box::new),
        ),
        ReadRequest::Catalogue(input) => ReadResponse::Catalogue(
            Box::pin(NativeTaskStore::catalogue(
                original,
                admission,
                limits,
                input.filter,
                input.page_size,
                input.cursor.as_ref(),
            ))
            .await?,
        ),
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        ReadRequest::Search(input) => ReadResponse::Search(
            Box::pin(NativeTaskStore::search_lexical(
                original,
                admission,
                input,
                limits,
                max_query_bytes,
            ))
            .await?,
        ),
        ReadRequest::Query(input) => ReadResponse::Query(
            Box::pin(NativeTaskStore::query(original, admission, limits, input)).await?,
        ),
        ReadRequest::Schema => {
            ReadResponse::Schema(NativeTaskStore::query_schema(original, admission)?)
        }
        ReadRequest::ReadView => ReadResponse::ReadView(
            Box::pin(NativeTaskStore::read_view(original, admission, limits)).await?,
        ),
        ReadRequest::SessionHeads => ReadResponse::SessionHeads(
            Box::pin(NativeTaskStore::session_heads(original, admission, limits)).await?,
        ),
    })
}
