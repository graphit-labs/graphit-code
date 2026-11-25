//! Voyage text embeddings through the caller's existing bounded HTTP client.
use crate::{
    context::InferenceContext,
    embedding::{
        EmbeddingBatch, EmbeddingError, EmbeddingFailure, EmbeddingLimits, GuardedEmbeddingError,
        check_embedding_admission,
    },
};
use graphit_http::client::{
    BoundedClient, GuardedNetworkError, NetworkEffect, NetworkRequest, NetworkRequestContext,
};
use http::{HeaderMap, Method};
use secrecy::{ExposeSecret, SecretString, zeroize::Zeroizing};
use serde::Serialize;
use std::io::{self, Write};

/// Retrieval mode is supplied explicitly for each call.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VoyageInput {
    Document,
    Query,
}
impl VoyageInput {
    fn wire(self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::Query => "query",
        }
    }
}

pub struct VoyageEmbeddings<'a> {
    client: &'a BoundedClient,
    endpoint: &'a str,
    model: &'a str,
    dimensions: usize,
    limits: EmbeddingLimits,
}
impl<'a> VoyageEmbeddings<'a> {
    pub fn new(
        client: &'a BoundedClient,
        endpoint: &'a str,
        model: &'a str,
        dimensions: usize,
        limits: EmbeddingLimits,
    ) -> Result<Self, EmbeddingError> {
        if endpoint.trim().is_empty() || model.trim().is_empty() || dimensions == 0 {
            return Err(EmbeddingError::InvalidRequest);
        }
        if endpoint.len() > limits.request_json_bytes
            || model.len() > limits.request_json_bytes
            || dimensions > limits.max_dimensions
        {
            return Err(EmbeddingError::Limit);
        }
        Ok(Self {
            client,
            endpoint,
            model,
            dimensions,
            limits,
        })
    }
    pub async fn embed(
        &self,
        texts: &[&str],
        input: VoyageInput,
        key: &SecretString,
        context: InferenceContext<'_>,
    ) -> Result<EmbeddingBatch<'a>, EmbeddingError> {
        let mut gate = || Ok::<(), std::convert::Infallible>(());
        self.limits
            .validate_input(texts, self.dimensions, context)?;
        let headers = if texts.is_empty() {
            HeaderMap::new()
        } else {
            crate::embedding::credential_headers(key.expose_secret(), false, self.limits)?
        };
        self.embed_guarded(texts, input, &headers, context, &mut gate)
            .await
            .map_err(|error| match error {
                GuardedEmbeddingError::Embedding { source, .. } => source,
                GuardedEmbeddingError::Admission { source, .. } => match source {},
            })
    }

    /// Borrow admission and credential for the original finite embedding operation.
    pub async fn embed_guarded<G, E>(
        &self,
        texts: &[&str],
        input: VoyageInput,
        headers: &HeaderMap,
        context: InferenceContext<'_>,
        gate: &mut G,
    ) -> Result<EmbeddingBatch<'a>, GuardedEmbeddingError<E>>
    where
        G: FnMut() -> Result<(), E>,
    {
        let mut effect = NetworkEffect::NotSent;
        let mut completed = Vec::new();
        let mut usage = Vec::new();
        let result: Result<(), EmbeddingFailure<E>> = async {
            check_embedding_admission(gate, context)?;
            self.limits
                .validate_input(texts, self.dimensions, context)?;
            if texts.is_empty() {
                check_embedding_admission(gate, context)?;
                return Ok(());
            }
            let headers = crate::embedding::configured_headers(headers, self.limits, context)?;
            let batch_size = self.limits.max_batch.min(1000);
            // Preflight every chunk without retaining all encoded payloads.
            for chunk in texts.chunks(batch_size) {
                check_embedding_admission(gate, context)?;
                self.encode(chunk, input, context)?;
            }
            texts
                .len()
                .checked_mul(self.limits.response_json_bytes)
                .filter(|bytes| *bytes <= isize::MAX as usize)
                .ok_or(EmbeddingError::Limit)?;
            usage
                .try_reserve_exact(texts.len())
                .map_err(|_| EmbeddingError::Allocation)?;
            completed
                .try_reserve_exact(texts.len())
                .map_err(|_| EmbeddingError::Allocation)?;
            for chunk in texts.chunks(batch_size) {
                check_embedding_admission(gate, context)?;
                let body = self.encode(chunk, input, context)?;
                let response = self
                    .client
                    .execute_guarded(
                        NetworkRequest {
                            method: Method::POST,
                            url: self.endpoint,
                            headers: &headers,
                            body: &body,
                        },
                        NetworkRequestContext::new(context.deadline(), context.cancellation()),
                        gate,
                    )
                    .await;
                let response = match response {
                    Ok(response) => {
                        effect = NetworkEffect::MayHaveApplied;
                        response
                    }
                    Err(error) => {
                        if error.effect() == NetworkEffect::MayHaveApplied {
                            effect = NetworkEffect::MayHaveApplied;
                        }
                        return Err(match error {
                            GuardedNetworkError::Network { source, .. } => {
                                EmbeddingFailure::Embedding(source.into())
                            }
                            GuardedNetworkError::Admission { source, .. } => {
                                EmbeddingFailure::Admission(source)
                            }
                        });
                    }
                };
                check_embedding_admission(gate, context)?;
                if !response.status().is_success() {
                    return Err(EmbeddingError::Status(response.status()).into());
                }
                if response.body().len() > self.limits.response_json_bytes {
                    return Err(EmbeddingError::Limit.into());
                }
                let decoded = decode(response.body(), chunk.len(), self.dimensions, context);
                let admitted = check_embedding_admission(gate, context);
                context.check()?;
                let decoded = decoded?;
                admitted?;
                let offset = completed.len();
                completed.extend(
                    decoded
                        .entries
                        .into_iter()
                        .map(|(index, vector)| (offset + index, vector)),
                );
                if let Some(observed) = decoded.usage {
                    usage.push(observed);
                }
                if let Some(error) = decoded.failure {
                    return Err(error.into());
                }
            }
            check_embedding_admission(gate, context)?;
            if completed.len() != texts.len() {
                return Err(EmbeddingError::Shape.into());
            }
            Ok(())
        }
        .await;
        match result {
            Ok(()) => Ok(EmbeddingBatch::new(
                self.model,
                self.dimensions,
                completed.into_iter().map(|(_, vector)| vector).collect(),
            )
            .with_usage(usage)),
            Err(error) => Err(error.with_effect(effect).with_completed(completed, usage)),
        }
    }
    fn encode(
        &self,
        texts: &[&str],
        input: VoyageInput,
        context: InferenceContext<'_>,
    ) -> Result<Zeroizing<Vec<u8>>, EmbeddingError> {
        let mut writer = BoundedWriter {
            bytes: Zeroizing::new(Vec::new()),
            cap: self.limits.request_json_bytes,
            context,
            failure: None,
        };
        let result = serde_json::to_writer(
            &mut writer,
            &Request {
                model: self.model,
                input: texts,
                input_type: input.wire(),
                output_dtype: "float",
            },
        );
        if result.is_err() {
            return Err(writer.failure.unwrap_or(EmbeddingError::Codec));
        }
        context.check()?;
        Ok(writer.bytes)
    }
}
#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    input: &'a [&'a str],
    input_type: &'static str,
    output_dtype: &'static str,
}
struct BoundedWriter<'a> {
    bytes: Zeroizing<Vec<u8>>,
    cap: usize,
    context: InferenceContext<'a>,
    failure: Option<EmbeddingError>,
}
impl Write for BoundedWriter<'_> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let result = (|| {
            self.context.check()?;
            let required = self
                .bytes
                .len()
                .checked_add(data.len())
                .ok_or(EmbeddingError::Limit)?;
            if required > self.cap {
                return Err(EmbeddingError::Limit);
            }
            if required > self.bytes.capacity() {
                // Geometric growth avoids reallocating for each Serde fragment,
                // while the requested capacity remains within the byte budget.
                let target = self
                    .bytes
                    .capacity()
                    .max(data.len())
                    .checked_mul(2)
                    .unwrap_or(self.cap)
                    .min(self.cap)
                    .max(required);
                let additional = target - self.bytes.len();
                self.bytes
                    .try_reserve_exact(additional)
                    .map_err(|_| EmbeddingError::Allocation)?;
            }
            self.bytes.extend_from_slice(data);
            Ok(data.len())
        })();
        result.map_err(|e| {
            self.failure = Some(e);
            io::Error::other("embedding encoding failed")
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn decode(
    body: &[u8],
    count: usize,
    width: usize,
    context: InferenceContext<'_>,
) -> Result<crate::embedding_wire::Decoded, EmbeddingError> {
    crate::embedding_wire::decode(
        body,
        count,
        width,
        crate::embedding_wire::Format::Indexed,
        context,
    )
}
