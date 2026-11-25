//! Graphit reranking through the existing bounded native rerank pipeline.
use crate::{
    context::InferenceContext,
    native_rerank::{
        GuardedRerankError, NativeRerankError, NativeRerankLimits, RerankFailure, check_admission,
    },
    rerank_wire::{
        CallAdmission, Credentials, Envelope, WireBatch, encode_request, request_scores,
    },
};
use graphit_auth::broker::BrokerRerankService;
use graphit_http::client::{BoundedClient, NetworkEffect};
use http::HeaderMap;
use secrecy::zeroize::Zeroizing;
use serde::Serialize;

pub struct GraphitReranker<'a> {
    client: &'a BoundedClient,
    descriptor: &'a BrokerRerankService,
    limits: NativeRerankLimits,
}
impl<'a> GraphitReranker<'a> {
    pub fn new(
        client: &'a BoundedClient,
        descriptor: &'a BrokerRerankService,
        limits: NativeRerankLimits,
    ) -> Result<Self, NativeRerankError> {
        let endpoint = &descriptor.endpoint;
        if endpoint.trim().is_empty()
            || descriptor.max_documents == 0
            || [
                &descriptor.identity.service_revision,
                &descriptor.identity.model,
                &descriptor.identity.model_revision,
                &descriptor.identity.recipe,
            ]
            .into_iter()
            .any(|value| value.is_empty())
        {
            return Err(NativeRerankError::Policy);
        }
        if endpoint.len() > limits.max_request_json {
            return Err(NativeRerankError::Limit);
        }
        Ok(Self {
            client,
            descriptor,
            limits,
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
        let mut effect = NetworkEffect::NotSent;
        let result: Result<Vec<f64>, RerankFailure<E>> = async {
            check_admission(gate, context)?;
            self.limits.validate_input(query, documents, context)?;
            if documents.is_empty() {
                check_admission(gate, context)?;
                return Ok(Vec::new());
            }
            for chunk in documents.chunks(self.limits.max_batch.min(self.descriptor.max_documents))
            {
                check_admission(gate, context)?;
                self.encode(query, chunk, context)?;
            }
            let mut scores = Vec::new();
            scores
                .try_reserve_exact(documents.len())
                .map_err(|_| NativeRerankError::Allocation)?;
            for chunk in documents.chunks(self.limits.max_batch.min(self.descriptor.max_documents))
            {
                check_admission(gate, context)?;
                let body = self.encode(query, chunk, context)?;
                let chunk_scores = request_scores(
                    self.client,
                    &self.descriptor.endpoint,
                    WireBatch {
                        body,
                        expected_count: chunk.len(),
                        envelope: Envelope::Graphit(&self.descriptor.identity),
                    },
                    Credentials::Configured(headers),
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
                query,
                documents,
                top_n: documents.len(),
            },
            self.limits,
            context,
        )
    }
}
#[derive(Serialize)]
struct Request<'a> {
    query: &'a str,
    documents: &'a [&'a str],
    top_n: usize,
}
