use super::{MemoryEmbeddingCause, MemoryEmbeddingModel};
use crate::model_service::{EmbedInput, EmbedOperation, RpcLimits};
use graphit_auth::{UserMemoryWriteOperation, UserReadAdmission, UserWriteAdmission};
use graphit_records::{
    memory_store::{
        MemoryEmbeddingAttachment, MemoryHybridQuery, MemoryHybridSearchOptions,
        MemorySemanticSearchOptions, MemoryStoreError, MemoryStoreReadLimits,
        MemoryStoreWriteLimits,
    },
    user_memory_store::{
        NativeUserMemoryStore, UserMemoryEmbeddingAttachOutcome, UserMemoryEmbeddingCorpusRead,
        UserMemoryGeneration, UserMemoryHybridSearchRead, UserMemorySemanticSearchRead,
    },
};
use std::{error::Error, fmt, future::Future, num::NonZeroU64, sync::Arc};

/// Captured source generation and the latest independently observed attachment.
#[derive(Debug)]
pub enum UserMemoryEmbeddingIndexRead {
    NoCorpus,
    Complete {
        generation: UserMemoryGeneration,
        last_attachment: Option<Box<UserMemoryEmbeddingAttachOutcome>>,
    },
}

/// Preserve the producer cause, including USER confirmed publications/recovery keys.
pub struct UserMemoryEmbeddingFailure<E> {
    pub cause: MemoryEmbeddingCause<E>,
    pub last_attachment: Option<Box<UserMemoryEmbeddingAttachOutcome>>,
}
impl<E: fmt::Debug> fmt::Debug for UserMemoryEmbeddingFailure<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UserMemoryEmbeddingFailure")
            .field("cause", &self.cause)
            .field("has_previous_attachment", &self.last_attachment.is_some())
            .finish()
    }
}
impl<E> fmt::Display for UserMemoryEmbeddingFailure<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("USER Memory embedding consumer failed")
    }
}
impl<E: Error + 'static> Error for UserMemoryEmbeddingFailure<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match &self.cause {
            MemoryEmbeddingCause::Service(error) => error,
            MemoryEmbeddingCause::Store(error) => error.as_ref(),
        })
    }
}
impl<E> UserMemoryEmbeddingFailure<E> {
    fn service(error: E) -> Self {
        Self {
            cause: MemoryEmbeddingCause::Service(error),
            last_attachment: None,
        }
    }
}
impl<E> From<MemoryStoreError> for UserMemoryEmbeddingFailure<E> {
    fn from(error: MemoryStoreError) -> Self {
        Self {
            cause: MemoryEmbeddingCause::Store(Box::new(error)),
            last_attachment: None,
        }
    }
}

impl MemoryEmbeddingModel<'_> {
    fn check_user_read(&self, read: &UserReadAdmission<'_>) -> Result<(), MemoryStoreError> {
        self.original.check().map_err(MemoryStoreError::Request)?;
        if !Arc::ptr_eq(self.original, read.request()?.snapshot()) {
            return Err(MemoryStoreError::DifferentRequest);
        }
        Ok(())
    }

    fn check_user_write(
        &self,
        read: &UserReadAdmission<'_>,
        write: &UserWriteAdmission<'_>,
    ) -> Result<(), MemoryStoreError> {
        self.check_user_read(read)?;
        if write.operation()? != UserMemoryWriteOperation::Memory {
            return Err(MemoryStoreError::WrongAdmission);
        }
        if !Arc::ptr_eq(self.original, write.request()?.snapshot()) {
            return Err(MemoryStoreError::DifferentRequest);
        }
        let reader = read.user()?.live;
        let writer = write.user()?.live;
        if reader.owner() != writer.owner() || reader.runtime_root() != writer.runtime_root() {
            return Err(MemoryStoreError::WrongAdmission);
        }
        Ok(())
    }

    /// USER authority is independent of the model lease. Each chunk commits separately.
    pub async fn index_user<E, F, Fut>(
        &self,
        read: &UserReadAdmission<'_>,
        write: &UserWriteAdmission<'_>,
        read_limits: MemoryStoreReadLimits,
        write_limits: MemoryStoreWriteLimits,
        batch_limits: RpcLimits,
        mut embed: F,
    ) -> Result<UserMemoryEmbeddingIndexRead, UserMemoryEmbeddingFailure<E>>
    where
        F: FnMut(EmbedOperation) -> Fut,
        Fut: Future<Output = Result<Vec<Vec<f32>>, E>>,
    {
        self.check_user_write(read, write)?;
        let UserMemoryEmbeddingCorpusRead::Published(mut corpus) =
            NativeUserMemoryStore::capture_embeddings(
                self.original,
                read,
                self.identity.clone(),
                read_limits,
            )
            .await?
        else {
            self.check_user_write(read, write)?;
            return Ok(UserMemoryEmbeddingIndexRead::NoCorpus);
        };
        let generation = corpus.generation().clone();
        let mut last_attachment = None;
        let result: Result<(), UserMemoryEmbeddingFailure<E>> = async {
            while let Some(documents) = corpus.next_documents(self.original, read).await? {
                self.check_user_write(read, write)?;
                let width = self.identity.dimensions().get() as usize;
                let count_limit = batch_limits
                    .max_texts
                    .min(batch_limits.max_output_values / width);
                let mut documents = documents.into_iter().peekable();
                while documents.peek().is_some() {
                    self.check_user_write(read, write)?;
                    let mut metadata = Vec::new();
                    let mut texts = Vec::new();
                    let mut reused = Vec::new();
                    let mut input_bytes = 0usize;
                    let vector_bytes = width
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
                                    && input_bytes.checked_add(document.text().len()).is_some_and(
                                        |bytes| bytes <= batch_limits.max_input_bytes,
                                    )))
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
                    if metadata.is_empty() {
                        return Err(
                            MemoryStoreError::Limit("Memory embedding batch capacity").into()
                        );
                    }
                    self.check_user_write(read, write)?;
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
                        .map_err(UserMemoryEmbeddingFailure::service)?
                    };
                    self.check_user_write(read, write)?;
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
                        NativeUserMemoryStore::attach_embeddings(
                            self.original,
                            write,
                            &corpus,
                            attachments,
                            write_limits,
                        )
                        .await?,
                    ));
                    self.check_user_write(read, write)?;
                }
            }
            self.check_user_write(read, write)?;
            Ok(())
        }
        .await;
        if let Err(mut error) = result {
            error.last_attachment = last_attachment;
            return Err(error);
        }
        Ok(UserMemoryEmbeddingIndexRead::Complete {
            generation,
            last_attachment,
        })
    }

    pub async fn query_user_hybrid<E, F, Fut>(
        &self,
        read: &UserReadAdmission<'_>,
        text: String,
        options: MemoryHybridSearchOptions,
        limits: MemoryStoreReadLimits,
        embed: F,
    ) -> Result<UserMemoryHybridSearchRead, UserMemoryEmbeddingFailure<E>>
    where
        F: FnMut(EmbedOperation) -> Fut,
        Fut: Future<Output = Result<Vec<Vec<f32>>, E>>,
    {
        self.check_user_read(read)?;
        let query = MemoryHybridQuery::new(&text, options, limits)?;
        let UserMemoryEmbeddingCorpusRead::Published(mut corpus) =
            NativeUserMemoryStore::capture_embeddings(
                self.original,
                read,
                self.identity.clone(),
                limits,
            )
            .await?
        else {
            self.check_user_read(read)?;
            return Ok(UserMemoryHybridSearchRead::NoCorpus);
        };
        if corpus.is_empty() {
            self.check_user_read(read)?;
            return Ok(UserMemoryHybridSearchRead::PublishedEmpty {
                generation: corpus.generation().clone(),
            });
        }
        self.check_user_read(read)?;
        let vector =
            self.query_vector(text, embed)
                .await
                .map_err(|failure| UserMemoryEmbeddingFailure {
                    cause: failure.cause,
                    last_attachment: None,
                })?;
        self.check_user_read(read)?;
        let result = corpus
            .search_hybrid(self.original, read, query, &vector)
            .await?;
        self.check_user_read(read)?;
        Ok(result)
    }

    pub async fn query_user<E, F, Fut>(
        &self,
        read: &UserReadAdmission<'_>,
        text: String,
        options: MemorySemanticSearchOptions,
        limits: MemoryStoreReadLimits,
        embed: F,
    ) -> Result<UserMemorySemanticSearchRead, UserMemoryEmbeddingFailure<E>>
    where
        F: FnMut(EmbedOperation) -> Fut,
        Fut: Future<Output = Result<Vec<Vec<f32>>, E>>,
    {
        self.check_user_read(read)?;
        let UserMemoryEmbeddingCorpusRead::Published(mut corpus) =
            NativeUserMemoryStore::capture_embeddings(
                self.original,
                read,
                self.identity.clone(),
                limits,
            )
            .await?
        else {
            self.check_user_read(read)?;
            return Ok(UserMemorySemanticSearchRead::NoCorpus);
        };
        if corpus.is_empty() {
            self.check_user_read(read)?;
            return Ok(UserMemorySemanticSearchRead::PublishedEmpty {
                generation: corpus.generation().clone(),
            });
        }
        self.check_user_read(read)?;
        let vector =
            self.query_vector(text, embed)
                .await
                .map_err(|failure| UserMemoryEmbeddingFailure {
                    cause: failure.cause,
                    last_attachment: None,
                })?;
        self.check_user_read(read)?;
        let result = corpus
            .search_semantic(self.original, read, &vector, options)
            .await?;
        self.check_user_read(read)?;
        Ok(result)
    }
}
