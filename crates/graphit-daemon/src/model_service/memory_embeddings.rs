//! Provider-neutral Memory orchestration over real admissions and Records producers.
use super::{EmbedInput, EmbedOperation, RpcLimits};
use graphit_auth::{
    ProjectReadAdmission, ProjectReadModule, ProjectWriteAdmission, ProjectWriteOperation,
};
use graphit_core::file_io::SnapshotContext;
use graphit_records::memory_store::{
    MemoryEmbeddingAttachOutcome, MemoryEmbeddingAttachment, MemoryEmbeddingCorpusRead,
    MemoryEmbeddingIdentity, MemoryGeneration, MemoryHybridQuery, MemoryHybridSearchOptions,
    MemoryHybridSearchRead, MemorySemanticSearchOptions, MemorySemanticSearchRead,
    MemoryStoreError, MemoryStoreReadLimits, MemoryStoreWriteLimits, NativeMemoryStore,
};
use std::{
    error::Error,
    fmt,
    future::Future,
    num::NonZeroU64,
    sync::Arc,
    time::{Duration, Instant},
};
mod user;
pub use user::{UserMemoryEmbeddingFailure, UserMemoryEmbeddingIndexRead};

#[derive(Debug)]
pub enum MemoryEmbeddingIndexRead {
    NoCorpus,
    Complete {
        generation: MemoryGeneration,
        last_attachment: Option<Box<MemoryEmbeddingAttachOutcome>>,
    },
}
pub struct MemoryEmbeddingFailure<E> {
    pub cause: MemoryEmbeddingCause<E>,
    pub last_attachment: Option<Box<MemoryEmbeddingAttachOutcome>>,
}
#[derive(Debug)]
pub enum MemoryEmbeddingCause<E> {
    Service(E),
    Store(Box<MemoryStoreError>),
}
impl<E: fmt::Debug> fmt::Debug for MemoryEmbeddingFailure<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryEmbeddingFailure")
            .field("cause", &self.cause)
            .field("has_previous_attachment", &self.last_attachment.is_some())
            .finish()
    }
}
impl<E> fmt::Display for MemoryEmbeddingFailure<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Memory embedding consumer failed")
    }
}
impl<E: Error + 'static> Error for MemoryEmbeddingFailure<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match &self.cause {
            MemoryEmbeddingCause::Service(e) => e,
            MemoryEmbeddingCause::Store(e) => e.as_ref(),
        })
    }
}
impl<E> MemoryEmbeddingFailure<E> {
    fn service(error: E) -> Self {
        Self {
            cause: MemoryEmbeddingCause::Service(error),
            last_attachment: None,
        }
    }
}
impl<E> From<MemoryStoreError> for MemoryEmbeddingFailure<E> {
    fn from(error: MemoryStoreError) -> Self {
        Self {
            cause: MemoryEmbeddingCause::Store(Box::new(error)),
            last_attachment: None,
        }
    }
}

/// Retain the caller's Original and exact vector-space identity, not a model
/// worker. The callback must execute that identity through an actual admitted
/// provider, with a finite subrequest for each operation. Corpus authority stays
/// bound to the original read/write admissions across those callbacks.
pub struct MemoryEmbeddingModel<'a> {
    original: &'a Arc<SnapshotContext>,
    identity: MemoryEmbeddingIdentity,
    rpc_timeout: Duration,
}
impl<'a> MemoryEmbeddingModel<'a> {
    pub fn new(
        original: &'a Arc<SnapshotContext>,
        identity: MemoryEmbeddingIdentity,
        rpc_timeout: Duration,
    ) -> Result<Self, MemoryStoreError> {
        original.check().map_err(MemoryStoreError::Request)?;
        if rpc_timeout.is_zero() {
            return Err(MemoryStoreError::Invalid(
                "zero embedding operation timeout",
            ));
        }
        Ok(Self {
            original,
            identity,
            rpc_timeout,
        })
    }
    fn operation_deadline(&self) -> Result<Instant, MemoryStoreError> {
        self.original.check().map_err(MemoryStoreError::Request)?;
        let deadline =
            Instant::now()
                .checked_add(self.rpc_timeout)
                .ok_or(MemoryStoreError::Invalid(
                    "embedding operation deadline overflow",
                ))?;
        Ok(self
            .original
            .deadline()
            .map_or(deadline, |original| deadline.min(original)))
    }
    fn validate_vectors(&self, vectors: &[Vec<f32>], count: usize) -> Result<(), MemoryStoreError> {
        let width = self.identity.dimensions().get() as usize;
        if vectors.len() != count
            || vectors.iter().any(|vector| {
                vector.len() != width || vector.iter().any(|value| !value.is_finite())
            })
        {
            return Err(MemoryStoreError::Invalid("Memory embedding output"));
        }
        Ok(())
    }
    pub fn identity(&self) -> &MemoryEmbeddingIdentity {
        &self.identity
    }

    /// Each inference subrequest is an independent SDK attachment; failures retain earlier publications.
    pub async fn index<E, F, Fut>(
        &self,
        read: &ProjectReadAdmission<'_>,
        write: &ProjectWriteAdmission<'_>,
        read_limits: MemoryStoreReadLimits,
        write_limits: MemoryStoreWriteLimits,
        batch_limits: RpcLimits,
        mut embed: F,
    ) -> Result<MemoryEmbeddingIndexRead, MemoryEmbeddingFailure<E>>
    where
        F: FnMut(EmbedOperation) -> Fut,
        Fut: Future<Output = Result<Vec<Vec<f32>>, E>>,
    {
        self.original.check().map_err(MemoryStoreError::Request)?;
        if write.operation() != ProjectWriteOperation::Memory {
            return Err(MemoryStoreError::WrongAdmission.into());
        }
        if !Arc::ptr_eq(
            self.original,
            write.request().map_err(MemoryStoreError::Auth)?.snapshot(),
        ) {
            return Err(MemoryStoreError::DifferentRequest.into());
        }
        write.recheck().map_err(MemoryStoreError::Auth)?;
        let MemoryEmbeddingCorpusRead::Published(mut corpus) =
            NativeMemoryStore::capture_embeddings(
                self.original,
                read,
                self.identity.clone(),
                read_limits,
            )
            .await?
        else {
            return Ok(MemoryEmbeddingIndexRead::NoCorpus);
        };
        let generation = corpus.generation().clone();
        let mut last_attachment = None;
        let result: Result<(), MemoryEmbeddingFailure<E>> = async {
            while let Some(documents) = corpus.next_documents(self.original, read).await? {
                let limits = batch_limits;
                let dimensions = usize::try_from(self.identity.dimensions().get())
                    .map_err(|_| MemoryStoreError::Invalid("Memory embedding dimensions"))?;
                let count_limit = limits.max_texts.min(limits.max_output_values / dimensions);
                let mut documents = documents.into_iter().peekable();
                while documents.peek().is_some() {
                    self.original.check().map_err(MemoryStoreError::Request)?;
                    let mut metadata = Vec::new();
                    let mut texts = Vec::new();
                    let mut reused = Vec::new();
                    let mut input_bytes = 0usize;
                    let vector_bytes = dimensions
                        .checked_mul(4)
                        .and_then(|n| n.checked_add(56))
                        .ok_or(MemoryStoreError::Limit("embedding input bytes"))?;
                    while let Some(mut document) = documents.next_if(|document| {
                        let fits_attachment = metadata
                            .len()
                            .checked_add(1)
                            .and_then(|n| n.checked_mul(vector_bytes))
                            .is_some_and(|bytes| bytes <= write_limits.max_input_bytes.get());
                        fits_attachment
                            && (document.reusable_vector().is_some()
                                || (texts.len() < count_limit
                                    && input_bytes
                                        .checked_add(document.text().len())
                                        .is_some_and(|bytes| bytes <= limits.max_input_bytes)))
                    }) {
                        self.original.check().map_err(MemoryStoreError::Request)?;
                        if document.reusable_vector().is_none() {
                            input_bytes = input_bytes
                                .checked_add(document.text().len())
                                .ok_or(MemoryStoreError::Limit("Memory embedding input bytes"))?;
                        }
                        metadata.push((
                            document.memory_id(),
                            NonZeroU64::new(document.revision())
                                .ok_or(MemoryStoreError::Invalid("Memory embedding revision"))?,
                            *document.fingerprint(),
                        ));
                        let vector = document.take_reusable_vector();
                        if vector.is_none() {
                            texts.push(document.into_text());
                        }
                        reused.push(vector);
                    }
                    // A valid prefix is published first; the next iteration rejects an
                    // indivisible document that cannot fit one bounded inference operation.
                    if metadata.is_empty() {
                        return Err(
                            MemoryStoreError::Limit("Memory embedding batch capacity").into()
                        );
                    }
                    self.original.check().map_err(MemoryStoreError::Request)?;
                    read.recheck().map_err(MemoryStoreError::Auth)?;
                    write.recheck().map_err(MemoryStoreError::Auth)?;
                    let inferred_count = texts.len();
                    let vectors = if texts.is_empty() {
                        Vec::new()
                    } else {
                        embed(EmbedOperation {
                            input: EmbedInput::Document,
                            texts,
                            deadline: self.operation_deadline()?,
                        })
                        .await
                        .map_err(MemoryEmbeddingFailure::service)?
                    };
                    self.original.check().map_err(MemoryStoreError::Request)?;
                    read.recheck().map_err(MemoryStoreError::Auth)?;
                    write.recheck().map_err(MemoryStoreError::Auth)?;
                    self.validate_vectors(&vectors, inferred_count)?;
                    let mut inferred = vectors.into_iter();
                    let vectors = reused
                        .into_iter()
                        .map(|vector| {
                            vector
                                .or_else(|| inferred.next())
                                .ok_or(MemoryStoreError::Invalid("Memory embedding output"))
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let attachments = metadata
                        .into_iter()
                        .zip(vectors)
                        .map(|((memory_id, revision, input_fingerprint), vector)| {
                            MemoryEmbeddingAttachment {
                                memory_id,
                                revision,
                                input_fingerprint,
                                vector,
                            }
                        })
                        .collect();
                    last_attachment = Some(Box::new(
                        NativeMemoryStore::attach_embeddings(
                            self.original,
                            write,
                            &corpus,
                            attachments,
                            write_limits,
                        )
                        .await?,
                    ));
                }
            }
            Ok(())
        }
        .await;
        if let Err(mut error) = result {
            error.last_attachment = last_attachment;
            return Err(error);
        }
        Ok(MemoryEmbeddingIndexRead::Complete {
            generation,
            last_attachment,
        })
    }

    pub async fn query<E, F, Fut>(
        &self,
        read: &ProjectReadAdmission<'_>,
        text: String,
        options: MemorySemanticSearchOptions,
        limits: MemoryStoreReadLimits,
        embed: F,
    ) -> Result<MemorySemanticSearchRead, MemoryEmbeddingFailure<E>>
    where
        F: FnMut(EmbedOperation) -> Fut,
        Fut: Future<Output = Result<Vec<Vec<f32>>, E>>,
    {
        let MemoryEmbeddingCorpusRead::Published(mut corpus) =
            NativeMemoryStore::capture_embeddings(
                self.original,
                read,
                self.identity.clone(),
                limits,
            )
            .await?
        else {
            return Ok(MemorySemanticSearchRead::NoCorpus);
        };
        if corpus.is_empty() {
            return Ok(MemorySemanticSearchRead::PublishedEmpty {
                generation: corpus.generation().clone(),
            });
        }
        let vector = self.query_vector(text, embed).await?;
        Ok(corpus
            .search_semantic(self.original, read, &vector, options)
            .await?)
    }
    pub async fn query_hybrid<E, F, Fut>(
        &self,
        read: &ProjectReadAdmission<'_>,
        text: String,
        options: MemoryHybridSearchOptions,
        limits: MemoryStoreReadLimits,
        embed: F,
    ) -> Result<MemoryHybridSearchRead, MemoryEmbeddingFailure<E>>
    where
        F: FnMut(EmbedOperation) -> Fut,
        Fut: Future<Output = Result<Vec<Vec<f32>>, E>>,
    {
        self.original.check().map_err(MemoryStoreError::Request)?;
        if read.module() != ProjectReadModule::Memory {
            return Err(MemoryStoreError::WrongAdmission.into());
        }
        if !Arc::ptr_eq(
            self.original,
            read.request().map_err(MemoryStoreError::from)?.snapshot(),
        ) {
            return Err(MemoryStoreError::DifferentRequest.into());
        }
        read.recheck().map_err(MemoryStoreError::from)?;
        let query = MemoryHybridQuery::new(&text, options, limits)?;
        let MemoryEmbeddingCorpusRead::Published(mut corpus) =
            NativeMemoryStore::capture_embeddings(
                self.original,
                read,
                self.identity.clone(),
                limits,
            )
            .await?
        else {
            return Ok(MemoryHybridSearchRead::NoCorpus);
        };
        if corpus.is_empty() {
            return Ok(MemoryHybridSearchRead::PublishedEmpty {
                generation: corpus.generation().clone(),
            });
        }
        let vector = self.query_vector(text, embed).await?;
        Ok(corpus
            .search_hybrid(self.original, read, query, &vector)
            .await?)
    }

    async fn query_vector<E, F, Fut>(
        &self,
        text: String,
        mut embed: F,
    ) -> Result<Vec<f32>, MemoryEmbeddingFailure<E>>
    where
        F: FnMut(EmbedOperation) -> Fut,
        Fut: Future<Output = Result<Vec<Vec<f32>>, E>>,
    {
        self.original.check().map_err(MemoryStoreError::Request)?;
        let mut vectors = embed(EmbedOperation {
            input: EmbedInput::Query,
            texts: vec![text],
            deadline: self.operation_deadline()?,
        })
        .await
        .map_err(MemoryEmbeddingFailure::service)?;
        self.original.check().map_err(MemoryStoreError::Request)?;
        self.validate_vectors(&vectors, 1)?;
        vectors
            .pop()
            .ok_or_else(|| MemoryStoreError::Invalid("Memory query vector absent").into())
    }
}
