//! Native Voyage reranking on the caller's existing bounded HTTP client.
use crate::{
    context::InferenceContext,
    native_rerank::{
        GuardedRerankError, NativeRerankError, NativeRerankLimits, RerankFailure, check_admission,
    },
    rerank_wire::{
        CallAdmission, Credentials, Envelope, WireBatch, encode_request, request_scores,
    },
};
use graphit_http::client::{BoundedClient, NetworkEffect};
use http::HeaderMap;
use secrecy::{ExposeSecret, SecretString, zeroize::Zeroizing};
use serde::Serialize;

pub struct VoyageReranker<'a> {
    client: &'a BoundedClient,
    endpoint: &'a str,
    model: &'a str,
    limits: NativeRerankLimits,
}
impl<'a> VoyageReranker<'a> {
    pub fn new(
        client: &'a BoundedClient,
        endpoint: &'a str,
        model: &'a str,
        limits: NativeRerankLimits,
    ) -> Result<Self, NativeRerankError> {
        if endpoint.trim().is_empty() || model.trim().is_empty() {
            return Err(NativeRerankError::Policy);
        }
        if endpoint.len() > limits.max_request_json || model.len() > limits.max_request_json {
            return Err(NativeRerankError::Limit);
        }
        Ok(Self {
            client,
            endpoint,
            model,
            limits,
        })
    }
    pub async fn score(
        &self,
        query: &str,
        documents: &[&str],
        key: &SecretString,
        context: InferenceContext<'_>,
    ) -> Result<Vec<f64>, NativeRerankError> {
        let mut gate = || Ok::<(), std::convert::Infallible>(());
        self.score_with_credentials(
            query,
            documents,
            Credentials::Bearer(key.expose_secret()),
            context,
            &mut gate,
        )
        .await
        .map_err(|error| match error {
            GuardedRerankError::Rerank { source, .. } => source,
            GuardedRerankError::Admission { source, .. } => match source {},
        })
    }

    /// Borrow configured headers and admission for this finite operation.
    pub async fn score_guarded<G, E>(
        &self,
        query: &str,
        documents: &[&str],
        headers: &HeaderMap,
        context: InferenceContext<'_>,
        gate: &mut G,
    ) -> Result<Vec<f64>, GuardedRerankError<E>>
    where
        G: FnMut() -> Result<(), E>,
    {
        self.score_with_credentials(
            query,
            documents,
            Credentials::Configured(headers),
            context,
            gate,
        )
        .await
    }

    async fn score_with_credentials<G, E>(
        &self,
        query: &str,
        documents: &[&str],
        credentials: Credentials<'_>,
        context: InferenceContext<'_>,
        gate: &mut G,
    ) -> Result<Vec<f64>, GuardedRerankError<E>>
    where
        G: FnMut() -> Result<(), E>,
    {
        let mut effect = NetworkEffect::NotSent;
        let result: Result<Vec<f64>, RerankFailure<E>> = async {
            check_admission(gate, context)?;
            self.limits.validate_input(query, documents, context)?;
            if documents.is_empty() {
                check_admission(gate, context)?;
                return Ok(Vec::new());
            }
            for chunk in documents.chunks(self.limits.max_batch.min(1000)) {
                check_admission(gate, context)?;
                self.encode(query, chunk, context)?;
            }
            let mut scores = Vec::new();
            scores
                .try_reserve_exact(documents.len())
                .map_err(|_| NativeRerankError::Allocation)?;
            for chunk in documents.chunks(self.limits.max_batch.min(1000)) {
                check_admission(gate, context)?;
                let body = self.encode(query, chunk, context)?;
                let chunk_scores = request_scores(
                    self.client,
                    self.endpoint,
                    WireBatch {
                        body,
                        expected_count: chunk.len(),
                        envelope: Envelope::Data,
                    },
                    credentials,
                    self.limits,
                    context,
                    CallAdmission {
                        gate,
                        effect: &mut effect,
                    },
                )
                .await?;
                check_admission(gate, context)?;
                let end = scores
                    .len()
                    .checked_add(chunk_scores.len())
                    .ok_or(NativeRerankError::Limit)?;
                if end > documents.len() {
                    return Err(NativeRerankError::Shape.into());
                }
                scores.extend(chunk_scores);
            }
            check_admission(gate, context)?;
            if scores.len() != documents.len() {
                return Err(NativeRerankError::Shape.into());
            }
            Ok(scores)
        }
        .await;
        result.map_err(|error| error.with_effect(effect))
    }
    fn encode(
        &self,
        query: &str,
        documents: &[&str],
        context: InferenceContext<'_>,
    ) -> Result<Zeroizing<Vec<u8>>, NativeRerankError> {
        encode_request(
            &Request {
                model: self.model,
                query,
                documents,
                top_k: documents.len(),
                return_documents: false,
            },
            self.limits,
            context,
        )
    }
}
#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    query: &'a str,
    documents: &'a [&'a str],
    top_k: usize,
    return_documents: bool,
}
