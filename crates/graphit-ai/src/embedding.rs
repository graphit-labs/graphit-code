//! Explicit embedding budgets and results; no profile or credential discovery.
use crate::context::{ContextError, InferenceContext};
use graphit_http::client::{NetworkEffect, NetworkError};
use http::StatusCode;
use http::{HeaderMap, HeaderValue, header};
use secrecy::zeroize::Zeroizing;
use serde_json::value::RawValue;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmbeddingError {
    Context(ContextError),
    Network(NetworkError),
    Status(StatusCode),
    InvalidPolicy,
    InvalidRequest,
    Credential,
    Limit,
    Codec,
    Shape,
    NonFinite,
    Allocation,
}
impl fmt::Display for EmbeddingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for EmbeddingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Context(e) => Some(e),
            Self::Network(e) => Some(e),
            _ => None,
        }
    }
}
impl From<ContextError> for EmbeddingError {
    fn from(e: ContextError) -> Self {
        Self::Context(e)
    }
}
impl From<NetworkError> for EmbeddingError {
    fn from(e: NetworkError) -> Self {
        Self::Network(e)
    }
}

/// Typed admission and embedding failures retain the effect of all attempted batches.
pub enum GuardedEmbeddingError<E> {
    Embedding {
        source: EmbeddingError,
        effect: NetworkEffect,
        completed: Vec<(usize, Vec<f32>)>,
        usage: Vec<Box<RawValue>>,
    },
    Admission {
        source: E,
        effect: NetworkEffect,
    },
}
impl<E> GuardedEmbeddingError<E> {
    /// Only complete, validated and unambiguously indexed response items.
    pub fn completed(&self) -> &[(usize, Vec<f32>)] {
        match self {
            Self::Embedding { completed, .. } => completed,
            Self::Admission { .. } => &[],
        }
    }
    pub fn take_completed(&mut self) -> Vec<(usize, Vec<f32>)> {
        match self {
            Self::Embedding { completed, .. } => std::mem::take(completed),
            Self::Admission { .. } => Vec::new(),
        }
    }
    pub fn take_usage(&mut self) -> Vec<Box<RawValue>> {
        match self {
            Self::Embedding { usage, .. } => std::mem::take(usage),
            Self::Admission { .. } => Vec::new(),
        }
    }
    pub(crate) fn with_completed(
        mut self,
        completed: Vec<(usize, Vec<f32>)>,
        usage: Vec<Box<RawValue>>,
    ) -> Self {
        if let Self::Embedding {
            completed: output,
            usage: observed,
            ..
        } = &mut self
        {
            *output = completed;
            *observed = usage;
        }
        self
    }
    pub fn effect(&self) -> NetworkEffect {
        match self {
            Self::Embedding { effect, .. } | Self::Admission { effect, .. } => *effect,
        }
    }
}
impl<E> fmt::Debug for GuardedEmbeddingError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Embedding { source, effect, .. } => f
                .debug_struct("Embedding")
                .field("source", source)
                .field("effect", effect)
                .finish(),
            Self::Admission { effect, .. } => f
                .debug_struct("Admission")
                .field("effect", effect)
                .finish_non_exhaustive(),
        }
    }
}
impl<E> fmt::Display for GuardedEmbeddingError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("guarded embedding failed")
    }
}
impl<E: std::error::Error + 'static> std::error::Error for GuardedEmbeddingError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Embedding { source, .. } => Some(source),
            Self::Admission { source, .. } => Some(source),
        }
    }
}
pub(crate) enum EmbeddingFailure<E> {
    Embedding(EmbeddingError),
    Admission(E),
}
impl<E> From<EmbeddingError> for EmbeddingFailure<E> {
    fn from(error: EmbeddingError) -> Self {
        Self::Embedding(error)
    }
}
impl<E> From<ContextError> for EmbeddingFailure<E> {
    fn from(error: ContextError) -> Self {
        Self::Embedding(error.into())
    }
}
impl<E> EmbeddingFailure<E> {
    pub(crate) fn with_effect(self, effect: NetworkEffect) -> GuardedEmbeddingError<E> {
        match self {
            Self::Embedding(source) => GuardedEmbeddingError::Embedding {
                source,
                effect,
                completed: Vec::new(),
                usage: Vec::new(),
            },
            Self::Admission(source) => GuardedEmbeddingError::Admission { source, effect },
        }
    }
}
pub(crate) fn check_embedding_admission<G, E>(
    gate: &mut G,
    context: InferenceContext<'_>,
) -> Result<(), EmbeddingFailure<E>>
where
    G: FnMut() -> Result<(), E>,
{
    context.check()?;
    let admitted = gate().map_err(EmbeddingFailure::Admission);
    context.check()?;
    admitted
}

#[derive(Clone, Copy, Debug)]
pub struct EmbeddingLimits {
    pub(crate) max_texts: usize,
    pub(crate) max_input_bytes: usize,
    pub(crate) max_dimensions: usize,
    pub(crate) max_vector_values: usize,
    pub(crate) max_batch: usize,
    pub(crate) request_json_bytes: usize,
    pub(crate) response_json_bytes: usize,
}
impl EmbeddingLimits {
    pub fn new(
        max_texts: usize,
        max_input_bytes: usize,
        max_dimensions: usize,
        max_vector_values: usize,
        max_batch: usize,
        request_json_bytes: usize,
        response_json_bytes: usize,
    ) -> Result<Self, EmbeddingError> {
        for value in [
            max_texts,
            max_input_bytes,
            max_dimensions,
            max_vector_values,
            max_batch,
            request_json_bytes,
            response_json_bytes,
        ] {
            if value == 0 || value > isize::MAX as usize {
                return Err(EmbeddingError::InvalidPolicy);
            }
        }
        if max_texts > isize::MAX as usize / std::mem::size_of::<Vec<f32>>()
            || max_dimensions > isize::MAX as usize / std::mem::size_of::<f32>()
            || max_vector_values > isize::MAX as usize / std::mem::size_of::<f32>()
        {
            return Err(EmbeddingError::InvalidPolicy);
        }
        Ok(Self {
            max_texts,
            max_input_bytes,
            max_dimensions,
            max_vector_values,
            max_batch: max_batch.min(2048),
            request_json_bytes,
            response_json_bytes,
        })
    }
    pub(crate) fn validate_input(
        self,
        texts: &[&str],
        dimensions: usize,
        context: InferenceContext<'_>,
    ) -> Result<(), EmbeddingError> {
        context.check()?;
        if dimensions == 0 {
            return Err(EmbeddingError::InvalidRequest);
        }
        if dimensions > self.max_dimensions
            || texts.len() > self.max_texts
            || texts
                .len()
                .checked_mul(dimensions)
                .is_none_or(|n| n > self.max_vector_values)
        {
            return Err(EmbeddingError::Limit);
        }
        let mut bytes = 0usize;
        for text in texts {
            context.check()?;
            bytes = bytes.checked_add(text.len()).ok_or(EmbeddingError::Limit)?;
            if bytes > self.max_input_bytes {
                return Err(EmbeddingError::Limit);
            }
        }
        Ok(())
    }
}

/// Payload access is explicit; diagnostic and serialization traits are absent.
pub struct EmbeddingBatch<'a> {
    model: &'a str,
    dimensions: usize,
    vectors: Vec<Vec<f32>>,
    usage: Vec<Box<RawValue>>,
}
impl<'a> EmbeddingBatch<'a> {
    pub(crate) fn new(model: &'a str, dimensions: usize, vectors: Vec<Vec<f32>>) -> Self {
        Self {
            model,
            dimensions,
            vectors,
            usage: Vec::new(),
        }
    }
    pub fn model(&self) -> &str {
        self.model
    }
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }
    pub fn vectors(&self) -> &[Vec<f32>] {
        &self.vectors
    }
    pub fn into_vectors(self) -> Vec<Vec<f32>> {
        self.vectors
    }
    /// Observed usage objects, in the order of completed provider responses.
    pub fn usage(&self) -> &[Box<RawValue>] {
        &self.usage
    }
    pub fn take_usage(&mut self) -> Vec<Box<RawValue>> {
        std::mem::take(&mut self.usage)
    }
    pub(crate) fn with_usage(mut self, usage: Vec<Box<RawValue>>) -> Self {
        self.usage = usage;
        self
    }
}

/// Preserve the configured authentication headers; protocol content type is fixed.
pub(crate) fn configured_headers(
    headers: &HeaderMap,
    limits: EmbeddingLimits,
    context: InferenceContext<'_>,
) -> Result<HeaderMap, EmbeddingError> {
    context.check()?;
    if headers.contains_key(header::CONTENT_TYPE) {
        return Err(EmbeddingError::Credential);
    }
    let mut size = 0usize;
    for (name, value) in headers {
        context.check()?;
        size = size
            .checked_add(name.as_str().len())
            .and_then(|n| n.checked_add(value.as_bytes().len()))
            .ok_or(EmbeddingError::Limit)?;
        if size > limits.request_json_bytes {
            return Err(EmbeddingError::Limit);
        }
    }
    let mut result =
        HeaderMap::try_with_capacity(headers.len().checked_add(1).ok_or(EmbeddingError::Limit)?)
            .map_err(|_| EmbeddingError::Allocation)?;
    for (name, value) in headers {
        let mut value = value.clone();
        value.set_sensitive(true);
        result.append(name, value);
    }
    result.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    context.check()?;
    Ok(result)
}

/// The active Direct consumer and convenience calls retain their provider defaults.
pub(crate) fn credential_headers(
    key: &str,
    google: bool,
    limits: EmbeddingLimits,
) -> Result<HeaderMap, EmbeddingError> {
    if key.trim().is_empty() {
        return Err(EmbeddingError::Credential);
    }
    let prefix = if google { "" } else { "Bearer " };
    let size = prefix
        .len()
        .checked_add(key.len())
        .filter(|n| *n <= limits.request_json_bytes)
        .ok_or(EmbeddingError::Limit)?;
    let mut bytes = Zeroizing::new(Vec::new());
    bytes
        .try_reserve_exact(size)
        .map_err(|_| EmbeddingError::Allocation)?;
    bytes.extend_from_slice(prefix.as_bytes());
    bytes.extend_from_slice(key.as_bytes());
    let mut value = HeaderValue::from_bytes(&bytes).map_err(|_| EmbeddingError::Credential)?;
    value.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(
        if google {
            header::HeaderName::from_static("x-goog-api-key")
        } else {
            header::AUTHORIZATION
        },
        value,
    );
    Ok(headers)
}
