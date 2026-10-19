//! Direct native reranking using the caller's real Auth lease and HTTP pool.
use crate::{
    cohere_rerank::CohereReranker,
    context::InferenceContext,
    jina_rerank::JinaReranker,
    native_rerank::{GuardedRerankError, NativeRerankError, NativeRerankLimits},
    rerank_wire::credential_headers,
    voyage_rerank::VoyageReranker,
};
use graphit_auth::{
    ServiceBindingView, ServiceCredentialLease, ServicePurpose, ServiceUseError,
    profile::DirectProtocol,
};
use graphit_http::client::{BoundedClient, NetworkEffect};
use std::{fmt, io};

pub enum DirectNativeRerankError {
    UnsupportedBinding,
    Context(io::Error),
    Service {
        source: ServiceUseError,
        effect: NetworkEffect,
    },
    Rerank {
        source: NativeRerankError,
        effect: NetworkEffect,
    },
}
impl DirectNativeRerankError {
    pub fn effect(&self) -> NetworkEffect {
        match self {
            Self::UnsupportedBinding | Self::Context(_) => NetworkEffect::NotSent,
            Self::Service { effect, .. } | Self::Rerank { effect, .. } => *effect,
        }
    }
}
impl fmt::Debug for DirectNativeRerankError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            Self::UnsupportedBinding => "UnsupportedBinding",
            Self::Context(_) => "Context",
            Self::Service { .. } => "Service",
            Self::Rerank { .. } => "Rerank",
        };
        f.debug_struct(kind)
            .field("effect", &self.effect())
            .finish_non_exhaustive()
    }
}
impl fmt::Display for DirectNativeRerankError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("direct native reranking failed")
    }
}
impl std::error::Error for DirectNativeRerankError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::UnsupportedBinding => None,
            Self::Context(source) => Some(source),
            Self::Service { source, .. } => Some(source),
            Self::Rerank { source, .. } => Some(source),
        }
    }
}
impl From<GuardedRerankError<ServiceUseError>> for DirectNativeRerankError {
    fn from(error: GuardedRerankError<ServiceUseError>) -> Self {
        match error {
            GuardedRerankError::Rerank { source, effect } => Self::Rerank { source, effect },
            GuardedRerankError::Admission { source, effect } => Self::Service { source, effect },
        }
    }
}
fn not_sent_service(source: ServiceUseError) -> DirectNativeRerankError {
    DirectNativeRerankError::Service {
        source,
        effect: NetworkEffect::NotSent,
    }
}
fn not_sent_rerank(source: NativeRerankError) -> DirectNativeRerankError {
    DirectNativeRerankError::Rerank {
        source,
        effect: NetworkEffect::NotSent,
    }
}

/// Use only literal native-rerank bindings; selection and authentication stay with Auth.
/// Credentials, cancellation and deadline are borrowed from the lease for this future.
pub async fn score(
    lease: &ServiceCredentialLease<'_>,
    query: &str,
    documents: &[&str],
    client: &BoundedClient,
    limits: NativeRerankLimits,
) -> Result<Vec<f64>, DirectNativeRerankError> {
    let ServiceBindingView::Direct {
        purpose: ServicePurpose::Rerank,
        protocol,
        endpoint,
        model,
        dimensions: 0,
        ..
    } = lease.binding().map_err(not_sent_service)?
    else {
        return Err(DirectNativeRerankError::UnsupportedBinding);
    };
    if !matches!(
        protocol,
        DirectProtocol::Cohere
            | DirectProtocol::CohereV2
            | DirectProtocol::Voyage
            | DirectProtocol::VoyageV1
            | DirectProtocol::Jina
            | DirectProtocol::JinaV1
    ) {
        return Err(DirectNativeRerankError::UnsupportedBinding);
    }
    let original = lease.original().map_err(not_sent_service)?;
    let deadline = original
        .snapshot()
        .required_deadline()
        .map_err(DirectNativeRerankError::Context)?;
    let context = InferenceContext::new(original.cancellation(), deadline);
    if documents.is_empty() {
        limits
            .validate_input(query, documents, context)
            .map_err(not_sent_rerank)?;
        let admitted = lease.check().map_err(not_sent_service);
        context
            .check()
            .map_err(|error| not_sent_rerank(error.into()))?;
        admitted?;
        return Ok(Vec::new());
    }
    let key = lease
        .credential()
        .map_err(not_sent_service)?
        .ok_or_else(|| not_sent_service(ServiceUseError::MissingCredential))?;
    let headers = credential_headers(key, limits, context).map_err(not_sent_rerank)?;
    let mut gate = || lease.check();
    let result = match protocol {
        DirectProtocol::Cohere | DirectProtocol::CohereV2 => {
            CohereReranker::new(client, endpoint, model, limits)
                .map_err(not_sent_rerank)?
                .score_guarded(query, documents, &headers, context, &mut gate)
                .await
        }
        DirectProtocol::Voyage | DirectProtocol::VoyageV1 => {
            VoyageReranker::new(client, endpoint, model, limits)
                .map_err(not_sent_rerank)?
                .score_guarded(query, documents, &headers, context, &mut gate)
                .await
        }
        DirectProtocol::Jina | DirectProtocol::JinaV1 => {
            JinaReranker::new(client, endpoint, model, limits)
                .map_err(not_sent_rerank)?
                .score_guarded(query, documents, &headers, context, &mut gate)
                .await
        }
        _ => return Err(DirectNativeRerankError::UnsupportedBinding),
    };
    // A successful nonempty result completed at least one actual HTTP batch.
    let effect = result
        .as_ref()
        .map_or_else(|error| error.effect(), |_| NetworkEffect::MayHaveApplied);
    context
        .check()
        .map_err(|error| DirectNativeRerankError::Rerank {
            source: error.into(),
            effect,
        })?;
    let admitted = lease
        .check()
        .map_err(|source| DirectNativeRerankError::Service { source, effect });
    context
        .check()
        .map_err(|error| DirectNativeRerankError::Rerank {
            source: error.into(),
            effect,
        })?;
    match result {
        Ok(scores) => {
            admitted?;
            Ok(scores)
        }
        Err(error) => Err(error.into()),
    }
}
