//! Direct text embeddings using the caller's real Auth lease and HTTP pool.
use crate::{
    cohere_embedding::{CohereEmbeddings, CohereInput},
    context::InferenceContext,
    embedding::{EmbeddingError, EmbeddingLimits, GuardedEmbeddingError},
    google_embedding::{GoogleEmbeddings, GoogleEndpoints, GoogleFamily, GoogleInput},
    openai::OpenAiEmbeddings,
    voyage_embedding::{VoyageEmbeddings, VoyageInput},
};
use graphit_auth::{
    ServiceBindingView, ServiceCredentialLease, ServicePurpose, ServiceUseError,
    profile::DirectProtocol,
};
use graphit_http::client::{BoundedClient, NetworkEffect};
use std::{fmt, io};

pub enum DirectEmbeddingError {
    UnsupportedBinding,
    Context(io::Error),
    Service {
        source: ServiceUseError,
        effect: NetworkEffect,
    },
    Embedding {
        source: EmbeddingError,
        effect: NetworkEffect,
    },
}
impl DirectEmbeddingError {
    pub fn effect(&self) -> NetworkEffect {
        match self {
            Self::UnsupportedBinding | Self::Context(_) => NetworkEffect::NotSent,
            Self::Service { effect, .. } | Self::Embedding { effect, .. } => *effect,
        }
    }
}
impl fmt::Debug for DirectEmbeddingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            Self::UnsupportedBinding => "UnsupportedBinding",
            Self::Context(_) => "Context",
            Self::Service { .. } => "Service",
            Self::Embedding { .. } => "Embedding",
        };
        f.debug_struct(kind)
            .field("effect", &self.effect())
            .finish_non_exhaustive()
    }
}
impl fmt::Display for DirectEmbeddingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("direct embedding failed")
    }
}
impl std::error::Error for DirectEmbeddingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::UnsupportedBinding => None,
            Self::Context(source) => Some(source),
            Self::Service { source, .. } => Some(source),
            Self::Embedding { source, .. } => Some(source),
        }
    }
}
impl From<GuardedEmbeddingError<ServiceUseError>> for DirectEmbeddingError {
    fn from(error: GuardedEmbeddingError<ServiceUseError>) -> Self {
        match error {
            GuardedEmbeddingError::Embedding { source, effect, .. } => {
                Self::Embedding { source, effect }
            }
            GuardedEmbeddingError::Admission { source, effect } => Self::Service { source, effect },
        }
    }
}
fn not_sent_service(source: ServiceUseError) -> DirectEmbeddingError {
    DirectEmbeddingError::Service {
        source,
        effect: NetworkEffect::NotSent,
    }
}
fn not_sent_embedding(source: EmbeddingError) -> DirectEmbeddingError {
    DirectEmbeddingError::Embedding {
        source,
        effect: NetworkEffect::NotSent,
    }
}

/// Retrieval mode is explicit and mapped to the selected provider's native input type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectEmbeddingInput {
    Query,
    Document,
}

/// Selection and authentication stay with Auth; all borrowed data lives only for this future.
pub async fn embed(
    lease: &ServiceCredentialLease<'_>,
    texts: &[&str],
    input: DirectEmbeddingInput,
    client: &BoundedClient,
    limits: EmbeddingLimits,
) -> Result<Vec<Vec<f32>>, DirectEmbeddingError> {
    let ServiceBindingView::Direct {
        purpose: ServicePurpose::Embedding,
        protocol,
        endpoint,
        model,
        dimensions,
        request_dimensions,
        batch_endpoint,
        ..
    } = lease.binding().map_err(not_sent_service)?
    else {
        return Err(DirectEmbeddingError::UnsupportedBinding);
    };
    if dimensions == 0
        || !matches!(
            protocol,
            DirectProtocol::Cohere
                | DirectProtocol::Voyage
                | DirectProtocol::OpenAi
                | DirectProtocol::OpenAiCompatible
                | DirectProtocol::GoogleGemini1
                | DirectProtocol::GoogleGemini2
        )
    {
        return Err(DirectEmbeddingError::UnsupportedBinding);
    }
    let dimensions =
        usize::try_from(dimensions).map_err(|_| not_sent_embedding(EmbeddingError::Limit))?;
    let original = lease.original().map_err(not_sent_service)?;
    let deadline = original
        .snapshot()
        .required_deadline()
        .map_err(DirectEmbeddingError::Context)?;
    let context = InferenceContext::new(original.cancellation(), deadline);
    if texts.is_empty() {
        limits
            .validate_input(texts, dimensions, context)
            .map_err(not_sent_embedding)?;
        let admitted = lease.check().map_err(not_sent_service);
        context
            .check()
            .map_err(|error| not_sent_embedding(error.into()))?;
        admitted?;
        return Ok(Vec::new());
    }
    let key = lease
        .credential()
        .map_err(not_sent_service)?
        .ok_or_else(|| not_sent_service(ServiceUseError::MissingCredential))?;
    let headers = crate::embedding::credential_headers(
        key,
        matches!(
            protocol,
            DirectProtocol::GoogleGemini1 | DirectProtocol::GoogleGemini2
        ),
        limits,
    )
    .map_err(not_sent_embedding)?;
    let mut gate = || lease.check();
    let result = match protocol {
        DirectProtocol::Cohere => {
            let input = match input {
                DirectEmbeddingInput::Query => CohereInput::Query,
                DirectEmbeddingInput::Document => CohereInput::Document,
            };
            CohereEmbeddings::new(client, endpoint, model, dimensions, limits)
                .map_err(not_sent_embedding)?
                .embed_guarded(texts, input, &headers, context, &mut gate)
                .await
        }
        DirectProtocol::Voyage => {
            let input = match input {
                DirectEmbeddingInput::Query => VoyageInput::Query,
                DirectEmbeddingInput::Document => VoyageInput::Document,
            };
            VoyageEmbeddings::new(client, endpoint, model, dimensions, limits)
                .map_err(not_sent_embedding)?
                .embed_guarded(texts, input, &headers, context, &mut gate)
                .await
        }
        DirectProtocol::OpenAi | DirectProtocol::OpenAiCompatible => {
            let request_dimensions = request_dimensions
                .map(usize::try_from)
                .transpose()
                .map_err(|_| not_sent_embedding(EmbeddingError::Limit))?;
            if let Some(width) = request_dimensions {
                let native = match (protocol, model) {
                    (DirectProtocol::OpenAi, "text-embedding-3-small") => 1536,
                    (DirectProtocol::OpenAi, "text-embedding-3-large") => 3072,
                    _ => return Err(not_sent_embedding(EmbeddingError::InvalidRequest)),
                };
                if width == 0 || width >= native || width != dimensions {
                    return Err(not_sent_embedding(EmbeddingError::InvalidRequest));
                }
            }
            OpenAiEmbeddings::new(
                client,
                endpoint,
                model,
                dimensions,
                request_dimensions,
                limits,
            )
            .map_err(not_sent_embedding)?
            .embed_guarded(texts, &headers, context, &mut gate)
            .await
        }
        DirectProtocol::GoogleGemini1 | DirectProtocol::GoogleGemini2 => {
            let family = if protocol == DirectProtocol::GoogleGemini1 {
                GoogleFamily::Gemini1
            } else {
                GoogleFamily::Gemini2
            };
            let input = match input {
                DirectEmbeddingInput::Query => GoogleInput::Query,
                DirectEmbeddingInput::Document => GoogleInput::Document,
            };
            let batch_endpoint = batch_endpoint.ok_or(DirectEmbeddingError::UnsupportedBinding)?;
            GoogleEmbeddings::new(
                client,
                GoogleEndpoints {
                    single: endpoint,
                    batch: batch_endpoint,
                },
                model,
                dimensions,
                Some(dimensions),
                family,
                limits,
            )
            .map_err(not_sent_embedding)?
            .embed_guarded(texts, input, &headers, context, &mut gate)
            .await
        }
        _ => return Err(DirectEmbeddingError::UnsupportedBinding),
    };
    let effect = result
        .as_ref()
        .map_or_else(|error| error.effect(), |_| NetworkEffect::MayHaveApplied);
    context
        .check()
        .map_err(|error| DirectEmbeddingError::Embedding {
            source: error.into(),
            effect,
        })?;
    let admitted = lease
        .check()
        .map_err(|source| DirectEmbeddingError::Service { source, effect });
    context
        .check()
        .map_err(|error| DirectEmbeddingError::Embedding {
            source: error.into(),
            effect,
        })?;
    match result {
        Ok(batch) => {
            admitted?;
            Ok(batch.into_vectors())
        }
        Err(error) => Err(error.into()),
    }
}
