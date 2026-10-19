//! Authorized Knowledge corpus inference and NN through existing producers.
use crate::embedding_service::EmbeddingMetadata;
use graphit_auth::{
    AuthError, ProjectReadAdmission, ProjectReadModule, ProjectWriteAdmission,
    ProjectWriteOperation, ServiceUseError,
};
use graphit_core::{file_io::SnapshotContext, runtime_key::RuntimeKeyError};
use graphit_daemon::model_service::{EmbedInput, EmbedOperation, ModelServiceError, RpcLimits};
use graphit_data::{knowledge_embeddings::*, knowledge_index::*};
use graphit_knowledge::embedding_projection::{
    KnowledgeEmbeddingProjectionError, prepare_knowledge_embedding_document,
    prepare_knowledge_embedding_projection,
};
use std::{
    error::Error,
    fmt,
    future::Future,
    io,
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, Instant},
};

/// Observed vector space and existing batch policy, independent of the provider.
/// The callback retains its own real service authority for each finite operation.
pub struct KnowledgeSemanticModel<'a> {
    pub metadata: &'a EmbeddingMetadata,
    pub batch_limits: RpcLimits,
    pub rpc_timeout: Duration,
}
pub struct KnowledgeSemanticIndexLimits {
    pub read: KnowledgeIndexReadLimits,
    pub write: KnowledgeIndexWriteLimits,
}
pub struct KnowledgeSemanticQueryOptions {
    pub top_k: NonZeroUsize,
    pub metric: KnowledgeEmbeddingMetric,
}
#[derive(Debug)]
pub enum KnowledgeSemanticIndexResult {
    NoCorpus,
    Published(KnowledgeEmbeddingPublication),
}
#[derive(Debug)]
pub enum KnowledgeSemanticQueryResult {
    NoCorpus,
    Published(KnowledgeSemanticQuery),
}
#[derive(Debug)]
pub enum KnowledgeSemanticError {
    Request(io::Error),
    Auth(AuthError),
    Service(ServiceUseError),
    Runtime(RuntimeKeyError),
    Projection(KnowledgeEmbeddingProjectionError),
    Models(ModelServiceError),
    Inference(Box<dyn Error + Send + Sync>),
    InvalidInferenceOutput,
    InvalidInferencePolicy,
    Search(Box<graphit_data::knowledge_search::KnowledgeSearchError>),
    Data(Box<KnowledgeEmbeddingError>),
    Publication {
        outcome: Box<Result<KnowledgeEmbeddingPublication, KnowledgeEmbeddingError>>,
        cause: Box<KnowledgeSemanticError>,
    },
    Imported(Box<graphit_knowledge::imported_knowledge::ImportedKnowledgeSemanticError>),
    InvalidAdmission,
    ForeignRequest,
    MissingEmbeddingDimensions,
}
impl fmt::Display for KnowledgeSemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Knowledge semantic operation failed")
    }
}
impl Error for KnowledgeSemanticError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(e) => Some(e),
            Self::Auth(e) => Some(e),
            Self::Service(e) => Some(e),
            Self::Runtime(e) => Some(e),
            Self::Projection(e) => Some(e),
            Self::Models(e) => Some(e),
            Self::Inference(e) => Some(e.as_ref()),
            Self::Search(e) => Some(e.as_ref()),
            Self::Data(e) => Some(e.as_ref()),
            Self::Publication { cause, .. } => Some(cause.as_ref()),
            Self::Imported(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}
macro_rules! error_from {
    ($cause:ty, $variant:ident) => {
        impl From<$cause> for KnowledgeSemanticError {
            fn from(cause: $cause) -> Self {
                Self::$variant(cause)
            }
        }
    };
}
error_from!(io::Error, Request);
error_from!(AuthError, Auth);
error_from!(ServiceUseError, Service);
error_from!(RuntimeKeyError, Runtime);
error_from!(KnowledgeEmbeddingProjectionError, Projection);
error_from!(ModelServiceError, Models);
impl From<crate::embedding_service::EmbeddingServiceError> for KnowledgeSemanticError {
    fn from(error: crate::embedding_service::EmbeddingServiceError) -> Self {
        Self::Inference(Box::new(error))
    }
}
impl From<KnowledgeEmbeddingError> for KnowledgeSemanticError {
    fn from(cause: KnowledgeEmbeddingError) -> Self {
        Self::Data(Box::new(cause))
    }
}

impl From<graphit_knowledge::imported_knowledge::ImportedKnowledgeSemanticError>
    for KnowledgeSemanticError
{
    fn from(error: graphit_knowledge::imported_knowledge::ImportedKnowledgeSemanticError) -> Self {
        Self::Imported(Box::new(error))
    }
}
impl From<graphit_knowledge::imported_knowledge::ImportedKnowledgeReadError>
    for KnowledgeSemanticError
{
    fn from(error: graphit_knowledge::imported_knowledge::ImportedKnowledgeReadError) -> Self {
        graphit_knowledge::imported_knowledge::ImportedKnowledgeSemanticError::Reader(error).into()
    }
}

fn check_read(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
) -> Result<(), KnowledgeSemanticError> {
    original.check()?;
    if read.module() != ProjectReadModule::Knowledge {
        return Err(KnowledgeSemanticError::InvalidAdmission);
    }
    read.recheck()?;
    if !Arc::ptr_eq(original, read.request()?.snapshot()) {
        return Err(KnowledgeSemanticError::ForeignRequest);
    }
    Ok(())
}
fn check_write(
    original: &Arc<SnapshotContext>,
    write: &ProjectWriteAdmission<'_>,
) -> Result<(), KnowledgeSemanticError> {
    original.check()?;
    if write.operation() != ProjectWriteOperation::Knowledge {
        return Err(KnowledgeSemanticError::InvalidAdmission);
    }
    write.recheck()?;
    if !Arc::ptr_eq(original, write.request()?.snapshot()) {
        return Err(KnowledgeSemanticError::ForeignRequest);
    }
    Ok(())
}

/// Infer bounded input batches, then publish once after the source cursor reaches EOF.
pub async fn index_knowledge_semantics<E, F, C>(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    model: &KnowledgeSemanticModel<'_>,
    mode: KnowledgeEmbeddingWriteMode,
    limits: KnowledgeSemanticIndexLimits,
    infer: C,
) -> Result<KnowledgeSemanticIndexResult, KnowledgeSemanticError>
where
    E: Error + Send + Sync + 'static,
    F: Future<Output = Result<Vec<Vec<f32>>, E>>,
    C: FnMut(EmbedOperation) -> F,
{
    index_knowledge_semantics_inner(
        original, read, write, model, mode, limits, infer, None, None,
    )
    .await
}

/// Index the exact documental generation already observed by the caller.
/// A later head is a conflict, never an implicit adoption.
#[allow(clippy::too_many_arguments)]
pub async fn index_knowledge_semantics_at_generation<E, F, C>(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    model: &KnowledgeSemanticModel<'_>,
    mode: KnowledgeEmbeddingWriteMode,
    limits: KnowledgeSemanticIndexLimits,
    infer: C,
    expected: &KnowledgeGeneration,
) -> Result<KnowledgeSemanticIndexResult, KnowledgeSemanticError>
where
    E: Error + Send + Sync + 'static,
    F: Future<Output = Result<Vec<Vec<f32>>, E>>,
    C: FnMut(EmbedOperation) -> F,
{
    index_knowledge_semantics_inner(
        original,
        read,
        write,
        model,
        mode,
        limits,
        infer,
        Some(expected),
        None,
    )
    .await
}

/// Infer and confirm a finite selection of committed documents. Every call
/// completes its own selected Source/input streams; unrelated FILEs can keep
/// publishing while this batch is calculated.
#[allow(clippy::too_many_arguments)]
pub async fn index_knowledge_semantics_selection<E, F, C>(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    model: &KnowledgeSemanticModel<'_>,
    limits: KnowledgeSemanticIndexLimits,
    infer: C,
    selected: &std::collections::BTreeSet<String>,
) -> Result<KnowledgeSemanticIndexResult, KnowledgeSemanticError>
where
    E: Error + Send + Sync + 'static,
    F: Future<Output = Result<Vec<Vec<f32>>, E>>,
    C: FnMut(EmbedOperation) -> F,
{
    index_knowledge_semantics_inner(
        original,
        read,
        write,
        model,
        KnowledgeEmbeddingWriteMode::Attach,
        limits,
        infer,
        None,
        Some(selected),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn index_knowledge_semantics_inner<E, F, C>(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    model: &KnowledgeSemanticModel<'_>,
    mode: KnowledgeEmbeddingWriteMode,
    limits: KnowledgeSemanticIndexLimits,
    mut infer: C,
    expected: Option<&KnowledgeGeneration>,
    selected: Option<&std::collections::BTreeSet<String>>,
) -> Result<KnowledgeSemanticIndexResult, KnowledgeSemanticError>
where
    E: Error + Send + Sync + 'static,
    F: Future<Output = Result<Vec<Vec<f32>>, E>>,
    C: FnMut(EmbedOperation) -> F,
{
    check_read(original, read)?;
    check_write(original, write)?;
    let captured = match selected {
        Some(slugs) => {
            capture_knowledge_embedding_selection(original, read, slugs, limits.read).await
        }
        None => capture_knowledge_embedding_corpus(original, read, limits.read).await,
    }?;
    let corpus = match captured {
        KnowledgeEmbeddingRead::NoCorpus if expected.is_some() => {
            return Err(KnowledgeEmbeddingError::Conflict.into());
        }
        KnowledgeEmbeddingRead::NoCorpus => return Ok(KnowledgeSemanticIndexResult::NoCorpus),
        KnowledgeEmbeddingRead::Published(corpus) => corpus,
    };
    if expected.is_some_and(|generation| generation != corpus.source_generation()) {
        return Err(KnowledgeEmbeddingError::Conflict.into());
    }
    let dimensions = model.metadata.dimensions;
    validate_model(model)?;
    let identity = prepare_knowledge_embedding_projection(
        original,
        &corpus,
        &[],
        model.metadata.vector_id,
        dimensions,
    )?
    .identity()
    .clone();
    let mode = if selected.is_some() && corpus.requires_reindex(&identity)? {
        KnowledgeEmbeddingWriteMode::ReplaceComplete
    } else {
        mode
    };
    let rpc = model.batch_limits;
    let count_limit = rpc
        .max_texts
        .min(rpc.max_output_values / identity.dimensions().get() as usize);
    if count_limit == 0 {
        return Err(KnowledgeSemanticError::InvalidInferencePolicy);
    }
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let input = Box::pin(
        graphit_data::lance_module_store::RecordBatchStreamAdapter::new(
            native_knowledge_embedding_schema(&identity),
            futures_util::stream::unfold(receiver, |mut receiver| async move {
                receiver.recv().await.map(|batch| (batch, receiver))
            }),
        ),
    );
    // Keep the generic inference state out of the publisher/join stack frames.
    // Both branches still run in this admitted operation and share its guards.
    let produce = Box::pin(async {
        let result: Result<(), KnowledgeSemanticError> = async {
            let mut sent_any = false;
            while let Some(documents) = corpus.next_documents(original, read).await? {
                check_read(original, read)?;
                check_write(original, write)?;
                let mut texts = Vec::new();
                let mut inputs = Vec::new();
                let mut reusable = Vec::new();
                let mut bytes = 0usize;
                for document in &documents {
                    check_read(original, read)?;
                    check_write(original, write)?;
                    let projected = prepare_knowledge_embedding_document(
                        original, &corpus, document, &identity,
                    )?;
                    if !projected.is_pending() {
                        if matches!(mode, KnowledgeEmbeddingWriteMode::ReplaceComplete) {
                            reusable
                                .push((projected.slug().to_owned(), projected.input_fingerprint()));
                        }
                        continue;
                    }
                    let additional = projected.text().len();
                    if !texts.is_empty()
                        && (texts.len() >= count_limit
                            || bytes
                                .checked_add(additional)
                                .is_none_or(|value| value > rpc.max_input_bytes))
                    {
                        if !send_document_batch(
                            original,
                            read,
                            write,
                            model,
                            &corpus,
                            &identity,
                            std::mem::take(&mut inputs),
                            std::mem::take(&mut texts),
                            &mut infer,
                            &sender,
                            limits.write,
                        )
                        .await?
                        {
                            return Ok(());
                        }
                        sent_any = true;
                        bytes = 0;
                    }
                    inputs.push((projected.slug().to_owned(), projected.input_fingerprint()));
                    texts.push(projected.into_text());
                    // Individually oversized text remains intact for Models' typed Limit.
                    bytes = bytes
                        .checked_add(additional)
                        .ok_or(KnowledgeEmbeddingProjectionError::ConstructionOverflow)?;
                    if texts.len() >= count_limit || bytes >= rpc.max_input_bytes {
                        if !send_document_batch(
                            original,
                            read,
                            write,
                            model,
                            &corpus,
                            &identity,
                            std::mem::take(&mut inputs),
                            std::mem::take(&mut texts),
                            &mut infer,
                            &sender,
                            limits.write,
                        )
                        .await?
                        {
                            return Ok(());
                        }
                        sent_any = true;
                        bytes = 0;
                    }
                }
                if !texts.is_empty() {
                    if !send_document_batch(
                        original,
                        read,
                        write,
                        model,
                        &corpus,
                        &identity,
                        inputs,
                        texts,
                        &mut infer,
                        &sender,
                        limits.write,
                    )
                    .await?
                    {
                        return Ok(());
                    }
                    sent_any = true;
                }
                if !reusable.is_empty() {
                    let permit =
                        match semantic_guarded(sender.reserve(), original, read, Some(write))
                            .await?
                        {
                            Ok(permit) => permit,
                            Err(_) => return Ok(()),
                        };
                    let inputs: Vec<_> = reusable
                        .iter()
                        .map(|(slug, fingerprint)| KnowledgeEmbeddingInput {
                            slug,
                            input_fingerprint: *fingerprint,
                        })
                        .collect();
                    let batch = semantic_guarded(
                        corpus.read_reusable_embedding_batch(original, read, &identity, &inputs),
                        original,
                        read,
                        Some(write),
                    )
                    .await??;
                    permit.send(Ok(batch));
                    sent_any = true;
                }
                // SDK-local bodies and text/vector batches drop before the next read.
            }
            check_read(original, read)?;
            check_write(original, write)?;
            if !sent_any {
                let empty = encode_native_knowledge_embedding_batch(
                    original,
                    write,
                    &identity,
                    &[],
                    limits.write,
                )?;
                let _ =
                    semantic_guarded(sender.send(Ok(empty)), original, read, Some(write)).await?;
            }
            Ok(())
        }
        .await;
        let cause = result.err();
        if cause.is_some() {
            let _ = sender
                .send(Err(
                    graphit_data::lance_module_store::DataFusionError::Execution(
                        "Knowledge inference stream failed in its admitted producer".into(),
                    ),
                ))
                .await;
        }
        drop(sender);
        cause
    });
    let publish = Box::pin(publish_knowledge_embeddings_stream(
        original,
        write,
        &corpus,
        &identity,
        input,
        mode,
        limits.write,
    ));
    let (cause, publication) = tokio::join!(produce, publish);
    if let Some(cause) = cause {
        let retain_outcome = matches!(
            &publication,
            Ok(KnowledgeEmbeddingPublication::Committed { .. })
                | Err(KnowledgeEmbeddingError::CorpusChanged { .. }
                    | KnowledgeEmbeddingError::IndexFailed { .. }
                    | KnowledgeEmbeddingError::CommittedButRecheckFailed { .. }
                    | KnowledgeEmbeddingError::Delta(_)
                    | KnowledgeEmbeddingError::Branch(_),)
        );
        if retain_outcome {
            return Err(KnowledgeSemanticError::Publication {
                outcome: Box::new(publication),
                cause: Box::new(cause),
            });
        }
        return Err(cause);
    }
    Ok(KnowledgeSemanticIndexResult::Published(publication?))
}

#[allow(clippy::too_many_arguments)]
async fn send_document_batch<E, F, C>(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    model: &KnowledgeSemanticModel<'_>,
    corpus: &KnowledgeEmbeddingCapture,
    identity: &KnowledgeEmbeddingIdentity,
    inputs: Vec<(String, [u8; 32])>,
    texts: Vec<String>,
    infer: &mut C,
    sender: &tokio::sync::mpsc::Sender<
        Result<arrow_array::RecordBatch, graphit_data::lance_module_store::DataFusionError>,
    >,
    limits: KnowledgeIndexWriteLimits,
) -> Result<bool, KnowledgeSemanticError>
where
    E: Error + Send + Sync + 'static,
    F: Future<Output = Result<Vec<Vec<f32>>, E>>,
    C: FnMut(EmbedOperation) -> F,
{
    // Reserve before RPC. A failed SDK consumer ends this producer without a
    // fabricated I/O error; the actual publisher failure owns the outcome.
    let permit = match semantic_guarded(sender.reserve(), original, read, Some(write)).await? {
        Ok(permit) => permit,
        Err(_) => return Ok(false),
    };
    let observed = semantic_guarded(
        Box::pin(infer_document_batch(original, model, inputs, texts, infer)),
        original,
        read,
        Some(write),
    )
    .await;
    check_read(original, read)?;
    check_write(original, write)?;
    corpus.recheck(original, read)?;
    let observed = observed??;
    let rows: Vec<_> = observed
        .iter()
        .map(|(slug, fingerprint, vector)| KnowledgeEmbeddingRow {
            input: KnowledgeEmbeddingInput {
                slug,
                input_fingerprint: *fingerprint,
            },
            vector,
        })
        .collect();
    let batch = encode_native_knowledge_embedding_batch(original, write, identity, &rows, limits)?;
    check_read(original, read)?;
    check_write(original, write)?;
    permit.send(Ok(batch));
    Ok(true)
}

fn check_admissions(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: Option<&ProjectWriteAdmission<'_>>,
) -> Result<(), KnowledgeSemanticError> {
    check_read(original, read)?;
    if let Some(write) = write {
        check_write(original, write)?;
    }
    Ok(())
}
async fn semantic_guarded<T>(
    future: impl Future<Output = T>,
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: Option<&ProjectWriteAdmission<'_>>,
) -> Result<T, KnowledgeSemanticError> {
    check_admissions(original, read, write)?;
    tokio::pin!(future);
    let mut timer = tokio::time::interval(std::time::Duration::from_millis(25));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            value = &mut future => {
                check_admissions(original, read, write)?;
                return Ok(value);
            }
            _ = original.cancellation().cancelled() => {
                check_admissions(original, read, write)?;
            }
            _ = timer.tick() => {
                check_admissions(original, read, write)?;
            }
        }
    }
}

fn validate_model(model: &KnowledgeSemanticModel<'_>) -> Result<(), KnowledgeSemanticError> {
    if model.metadata.dimensions == 0 {
        return Err(KnowledgeSemanticError::MissingEmbeddingDimensions);
    }
    if model.rpc_timeout.is_zero()
        || model.batch_limits.max_texts == 0
        || model.batch_limits.max_input_bytes == 0
        || model.batch_limits.max_output_values == 0
    {
        return Err(KnowledgeSemanticError::InvalidInferencePolicy);
    }
    Ok(())
}
fn operation_deadline(
    original: &SnapshotContext,
    model: &KnowledgeSemanticModel<'_>,
) -> Result<Instant, KnowledgeSemanticError> {
    original.check()?;
    validate_model(model)?;
    let deadline = Instant::now()
        .checked_add(model.rpc_timeout)
        .ok_or(KnowledgeSemanticError::InvalidInferencePolicy)?;
    Ok(original
        .deadline()
        .map_or(deadline, |outer| deadline.min(outer)))
}
fn inference_error<E: Error + Send + Sync + 'static>(error: E) -> KnowledgeSemanticError {
    let error: Box<dyn Error + Send + Sync> = Box::new(error);
    match error.downcast::<ModelServiceError>() {
        Ok(error) => KnowledgeSemanticError::Models(*error),
        Err(error) => KnowledgeSemanticError::Inference(error),
    }
}
async fn infer_vectors<E, F, C>(
    original: &Arc<SnapshotContext>,
    model: &KnowledgeSemanticModel<'_>,
    input: EmbedInput,
    texts: Vec<String>,
    infer: C,
) -> Result<Vec<Vec<f32>>, KnowledgeSemanticError>
where
    E: Error + Send + Sync + 'static,
    F: Future<Output = Result<Vec<Vec<f32>>, E>>,
    C: FnOnce(EmbedOperation) -> F,
{
    let count = texts.len();
    let result = infer(EmbedOperation {
        input,
        texts,
        deadline: operation_deadline(original, model)?,
    })
    .await;
    original.check()?;
    let output = result.map_err(inference_error)?;
    let width = usize::try_from(model.metadata.dimensions)
        .map_err(|_| KnowledgeSemanticError::MissingEmbeddingDimensions)?;
    if output.len() != count
        || output
            .iter()
            .any(|vector| vector.len() != width || vector.iter().any(|value| !value.is_finite()))
    {
        return Err(KnowledgeSemanticError::InvalidInferenceOutput);
    }
    Ok(output)
}
async fn infer_document_batch<E, F, C>(
    original: &Arc<SnapshotContext>,
    model: &KnowledgeSemanticModel<'_>,
    inputs: Vec<(String, [u8; 32])>,
    texts: Vec<String>,
    infer: &mut C,
) -> Result<Vec<(String, [u8; 32], Vec<f32>)>, KnowledgeSemanticError>
where
    E: Error + Send + Sync + 'static,
    F: Future<Output = Result<Vec<Vec<f32>>, E>>,
    C: FnMut(EmbedOperation) -> F,
{
    if inputs.len() != texts.len() {
        return Err(KnowledgeSemanticError::InvalidInferenceOutput);
    }
    let output = infer_vectors(original, model, EmbedInput::Document, texts, infer).await?;
    Ok(inputs
        .into_iter()
        .zip(output)
        .map(|((slug, fingerprint), vector)| (slug, fingerprint, vector))
        .collect())
}

/// Query literal text through Models and retain Data's distinct query outcomes.
pub async fn query_knowledge_semantics<E, F, C>(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    model: &KnowledgeSemanticModel<'_>,
    text: &str,
    options: KnowledgeSemanticQueryOptions,
    limits: KnowledgeIndexReadLimits,
    infer: C,
) -> Result<KnowledgeSemanticQueryResult, KnowledgeSemanticError>
where
    E: Error + Send + Sync + 'static,
    F: Future<Output = Result<Vec<Vec<f32>>, E>>,
    C: FnOnce(EmbedOperation) -> F,
{
    check_read(original, read)?;
    let corpus = match capture_knowledge_embedding_corpus(original, read, limits).await? {
        KnowledgeEmbeddingRead::NoCorpus => return Ok(KnowledgeSemanticQueryResult::NoCorpus),
        KnowledgeEmbeddingRead::Published(corpus) => corpus,
    };
    let dimensions = model.metadata.dimensions;
    validate_model(model)?;
    let identity = prepare_knowledge_embedding_projection(
        original,
        &corpus,
        &[],
        model.metadata.vector_id,
        dimensions,
    )?
    .identity()
    .clone();
    let mut observed_inputs = Vec::new();
    while let Some(documents) = corpus.next_documents(original, read).await? {
        check_read(original, read)?;
        let projection = prepare_knowledge_embedding_projection(
            original,
            &corpus,
            &documents,
            model.metadata.vector_id,
            dimensions,
        )?;
        observed_inputs.extend(
            projection
                .current_inputs()
                .map(|input| (input.slug.to_owned(), input.input_fingerprint)),
        );
        check_read(original, read)?;
    }
    let output = semantic_guarded(
        infer_vectors(
            original,
            model,
            EmbedInput::Query,
            vec![text.to_owned()],
            infer,
        ),
        original,
        read,
        None,
    )
    .await;
    check_read(original, read)?;
    corpus.recheck(original, read)?;
    let output = output??;
    let vector = &output[0];
    let inputs: Vec<_> = observed_inputs
        .iter()
        .map(|(slug, fingerprint)| KnowledgeEmbeddingInput {
            slug,
            input_fingerprint: *fingerprint,
        })
        .collect();
    check_read(original, read)?;
    let result = query_knowledge_embeddings(
        original,
        read,
        &corpus,
        &identity,
        &inputs,
        KnowledgeEmbeddingQuery {
            vector,
            top_k: options.top_k,
            metric: options.metric,
        },
    )
    .await?;
    Ok(KnowledgeSemanticQueryResult::Published(result))
}

/// Browser/Source query parameters for the existing documentary search bridge.
pub(crate) struct KnowledgeSemanticSearchOptions<'a> {
    pub page: graphit_data::knowledge_search::KnowledgeLexicalQuery<'a>,
    pub mode: graphit_data::knowledge_search::KnowledgeVectorSearchMode,
    pub metric: KnowledgeEmbeddingMetric,
    pub limits: graphit_data::knowledge_search::KnowledgeSearchLimits,
    pub preview: graphit_knowledge::preview::MarkdownPreviewLimits,
}

pub(crate) struct KnowledgeSearchExecution {
    pub(crate) page: graphit_data::knowledge_search::KnowledgeVectorSearchPage,
    pub(crate) corpus: Box<KnowledgeEmbeddingCapture>,
    pub(crate) selected_slugs: Vec<String>,
}

/// Keep the same validated Source pin alive until selected Ask pages are obtained.
pub(crate) async fn query_knowledge_search_capture<E, F, C>(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    model: &KnowledgeSemanticModel<'_>,
    options: KnowledgeSemanticSearchOptions<'_>,
    infer: C,
    select_pages: bool,
) -> Result<Option<KnowledgeSearchExecution>, KnowledgeSemanticError>
where
    E: Error + Send + Sync + 'static,
    F: Future<Output = Result<Vec<Vec<f32>>, E>>,
    C: FnOnce(EmbedOperation) -> F,
{
    check_read(original, read)?;
    let corpus =
        match capture_knowledge_embedding_corpus(original, read, options.limits.read).await? {
            KnowledgeEmbeddingRead::NoCorpus => return Ok(None),
            KnowledgeEmbeddingRead::Published(corpus) => corpus,
        };
    let dimensions = model.metadata.dimensions;
    validate_model(model)?;
    let identity = prepare_knowledge_embedding_projection(
        original,
        &corpus,
        &[],
        model.metadata.vector_id,
        dimensions,
    )?
    .identity()
    .clone();
    let mut observed_inputs = Vec::new();
    while let Some(documents) = corpus.next_documents(original, read).await? {
        check_read(original, read)?;
        let projection = prepare_knowledge_embedding_projection(
            original,
            &corpus,
            &documents,
            model.metadata.vector_id,
            dimensions,
        )?;
        observed_inputs.extend(
            projection
                .current_inputs()
                .map(|input| (input.slug.to_owned(), input.input_fingerprint)),
        );
    }
    let inputs: Vec<_> = observed_inputs
        .iter()
        .map(|(slug, fingerprint)| KnowledgeEmbeddingInput {
            slug,
            input_fingerprint: *fingerprint,
        })
        .collect();
    check_read(original, read)?;
    let readiness =
        knowledge_embedding_search_readiness(original, read, &corpus, &identity, &inputs)?;
    let output = if readiness == KnowledgeEmbeddingSearchReadiness::Ready {
        let result = semantic_guarded(
            Box::pin(infer_vectors(
                original,
                model,
                EmbedInput::Query,
                vec![options.page.text.to_owned()],
                infer,
            )),
            original,
            read,
            None,
        )
        .await;
        check_read(original, read)?;
        corpus.recheck(original, read)?;
        Some(result??)
    } else {
        None
    };
    check_read(original, read)?;
    let mut page = graphit_knowledge::search::query_native_knowledge_vector_search(
        original,
        read,
        &corpus,
        &identity,
        &inputs,
        graphit_data::knowledge_search::KnowledgeVectorSearchQuery {
            page: options.page,
            mode: options.mode,
            vector: output.as_ref().map(|output| output[0].as_slice()),
            metric: options.metric,
        },
        options.limits,
        options.preview,
    )
    .await
    .map_err(|error| KnowledgeSemanticError::Search(Box::new(error)))?;
    let mut selection = Vec::new();
    if select_pages {
        selection.extend(
            std::mem::take(&mut page.hits)
                .into_iter()
                .map(|hit| hit.source.slug),
        );
        while let Some(cursor) = page.next_cursor.take() {
            let mut query = options.page;
            query.cursor = Some(&cursor);
            let next = graphit_knowledge::search::query_native_knowledge_vector_search(
                original,
                read,
                &corpus,
                &identity,
                &inputs,
                graphit_data::knowledge_search::KnowledgeVectorSearchQuery {
                    page: query,
                    mode: options.mode,
                    vector: output.as_ref().map(|output| output[0].as_slice()),
                    metric: options.metric,
                },
                options.limits,
                options.preview,
            )
            .await
            .map_err(|error| KnowledgeSemanticError::Search(Box::new(error)))?;
            selection.extend(next.hits.into_iter().map(|hit| hit.source.slug));
            page.next_cursor = next.next_cursor;
        }
    }
    check_read(original, read)?;
    Ok(Some(KnowledgeSearchExecution {
        page,
        corpus,
        selected_slugs: selection,
    }))
}

#[cfg(test)]
mod callback_tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;

    fn metadata() -> EmbeddingMetadata {
        EmbeddingMetadata::remote([7; 32], 2, "fixture-model".into(), "weights-r1".into()).unwrap()
    }

    #[tokio::test]
    async fn document_callback_preserves_full_inputs_and_rejects_incomplete_or_nonfinite_output() {
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(10),
            Cancellation::default(),
        ));
        let metadata = metadata();
        let model = KnowledgeSemanticModel {
            metadata: &metadata,
            batch_limits: RpcLimits::new(65536, 2, 32768, 4).unwrap(),
            rpc_timeout: Duration::from_secs(30),
        };
        let text = "# Literal α\r\n\r\n".repeat(1500);
        let inputs = vec![
            ("first".to_owned(), [1; 32]),
            ("second".to_owned(), [2; 32]),
        ];
        let result = infer_document_batch(
            &original,
            &model,
            inputs.clone(),
            vec![text.clone(), "second".into()],
            &mut |operation: EmbedOperation| {
                assert!(matches!(operation.input, EmbedInput::Document));
                assert_eq!(operation.texts, [text.clone(), "second".into()]);
                assert_eq!(operation.deadline, original.required_deadline().unwrap());
                std::future::ready(Ok::<_, io::Error>(vec![vec![1., 0.], vec![0., 1.]]))
            },
        )
        .await
        .unwrap();
        assert_eq!(
            result,
            [
                ("first".into(), [1; 32], vec![1., 0.]),
                ("second".into(), [2; 32], vec![0., 1.]),
            ]
        );
        for vectors in [
            vec![vec![1., 0.]],
            vec![vec![1.], vec![0., 1.]],
            vec![vec![f32::NAN, 0.], vec![0., 1.]],
            vec![vec![1., 0.], vec![f32::INFINITY, 1.]],
        ] {
            let error = infer_document_batch(
                &original,
                &model,
                inputs.clone(),
                vec!["a".into(), "b".into()],
                &mut |_| std::future::ready(Ok::<_, io::Error>(vectors.clone())),
            )
            .await
            .unwrap_err();
            assert!(matches!(
                error,
                KnowledgeSemanticError::InvalidInferenceOutput
            ));
        }
    }

    #[tokio::test]
    async fn continuous_outer_gets_finite_operation_and_cancellation_keeps_original_cause() {
        let cancel = Cancellation::default();
        let original = Arc::new(SnapshotContext::service(cancel.clone()));
        let metadata = metadata();
        let model = KnowledgeSemanticModel {
            metadata: &metadata,
            batch_limits: RpcLimits::new(65536, 2, 32768, 4).unwrap(),
            rpc_timeout: Duration::from_secs(10),
        };
        let started = Instant::now();
        let error = infer_vectors(
            &original,
            &model,
            EmbedInput::Query,
            vec!["literal".into()],
            |operation: EmbedOperation| {
                assert!(operation.deadline >= started + model.rpc_timeout);
                assert!(operation.deadline <= Instant::now() + model.rpc_timeout);
                cancel.cancel();
                std::future::ready(Err::<Vec<Vec<f32>>, _>(io::Error::from(
                    io::ErrorKind::ConnectionRefused,
                )))
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(error, KnowledgeSemanticError::Request(ref cause)
            if cause.kind() == io::ErrorKind::Interrupted));
        assert!(original.deadline().is_none());
    }
}
