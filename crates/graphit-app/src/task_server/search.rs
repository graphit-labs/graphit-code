//! Task query inference uses the configured provider; corpus authority stays with the caller.
use super::*;
use graphit_auth::{ProjectReadAdmission, ServiceSelection};
use graphit_daemon::model_service::{EmbedInput, EmbedOperation};
use graphit_records::task_store::{
    NativeTaskStore, TASK_DOCUMENT_RECIPE, TaskEmbeddingIdentity, TaskLexicalQuery,
    TaskLexicalSearchRead, TaskSearchMode,
};
use std::num::{NonZeroU32, NonZeroUsize};

pub(crate) async fn run(
    original: &RequestContext<'_>,
    admission: &ProjectReadAdmission<'_>,
    input: TaskLexicalQuery,
    policy: &TaskPolicy,
    max_query_bytes: NonZeroUsize,
    resources: Option<&crate::foreground_memory_jobs::MemoryJobResources>,
    service: Option<&mut ServiceSelection<'_>>,
) -> Result<TaskLexicalSearchRead, Failure> {
    if input.mode == TaskSearchMode::Fts {
        return NativeTaskStore::search_lexical(
            original.snapshot(),
            admission,
            input,
            policy.read,
            max_query_bytes,
        )
        .await
        .map_err(Failure::store);
    }
    if input.text.trim().is_empty() || input.text.len() > max_query_bytes.get() {
        return Err(Failure::new("invalid_arguments"));
    }
    admission.recheck().map_err(Failure::auth)?;
    let resources = resources.ok_or_else(|| Failure::new("model_unavailable"))?;
    let service = service.ok_or_else(|| Failure::new("embedding_service_unavailable"))?;
    let metadata = crate::embedding_service::metadata(service, &resources.execution())
        .await
        .map_err(model_failure)?;
    let dimensions = u32::try_from(metadata.dimensions)
        .ok()
        .and_then(NonZeroU32::new)
        .ok_or_else(|| Failure::new("invalid_embedding_response"))?;
    let identity =
        TaskEmbeddingIdentity::new(metadata.vector_id, dimensions, TASK_DOCUMENT_RECIPE.into())
            .map_err(Failure::store)?;
    let cached = input
        .cursor
        .as_ref()
        .map(|cursor| cursor.query_vector(&identity))
        .transpose()
        .map_err(Failure::store)?
        .flatten();
    let vector = if let Some(vector) = cached {
        vector.to_vec()
    } else {
        let mut output = resources
            .embed(
                service,
                &metadata,
                EmbedOperation {
                    input: EmbedInput::Query,
                    texts: vec![input.text.clone()],
                    deadline: original
                        .snapshot()
                        .required_deadline()
                        .map_err(|_| Failure::new("request_cancelled_or_expired"))?,
                },
            )
            .await
            .map_err(model_failure)?;
        if output.vectors.len() != 1 {
            return Err(Failure::new("invalid_embedding_response"));
        }
        output.vectors.pop().expect("one validated vector")
    };
    let result = NativeTaskStore::search_semantic(
        original.snapshot(),
        admission,
        identity,
        &vector,
        input,
        policy.read,
        max_query_bytes,
    )
    .await
    .map_err(Failure::store)?;
    service
        .mode()
        .map_err(|_| Failure::new("authentication_or_permission_denied"))?;
    admission.recheck().map_err(Failure::auth)?;
    Ok(result)
}

fn model_failure(error: crate::embedding_service::EmbeddingServiceError) -> Failure {
    use graphit_daemon::model_service::ModelErrorKind;
    match error.kind() {
        ModelErrorKind::Authentication => Failure::new("authentication_or_permission_denied"),
        ModelErrorKind::Cancelled => Failure::new("cancelled"),
        ModelErrorKind::Deadline => Failure::new("deadline"),
        ModelErrorKind::Output | ModelErrorKind::ConfigChanged => {
            Failure::new("invalid_embedding_response")
        }
        _ => Failure::new("embedding_service_failure"),
    }
}
