//! Google text retrieval embeddings with explicit family and captured endpoints.
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
use http::{HeaderMap, Method, Uri};
use secrecy::{ExposeSecret, SecretString, zeroize::Zeroizing};
use serde::Serialize;
use std::io::{self, Write};

/// The caller selects the family; a model name never supplies implicit capabilities.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GoogleFamily {
    Gemini1,
    Gemini2,
}
impl GoogleFamily {
    /// Semantic input overhead before JSON encoding, for a caller's literal byte budget.
    pub fn max_input_prefix_bytes(self) -> usize {
        match self {
            Self::Gemini1 => 0,
            Self::Gemini2 => GoogleInput::Query
                .prefix()
                .len()
                .max(GoogleInput::Document.prefix().len()),
        }
    }
}

/// Asymmetric text retrieval mode for each call.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GoogleInput {
    Document,
    Query,
}
impl GoogleInput {
    fn task_type(self) -> &'static str {
        match self {
            Self::Document => "RETRIEVAL_DOCUMENT",
            Self::Query => "RETRIEVAL_QUERY",
        }
    }
    fn prefix(self) -> &'static str {
        match self {
            Self::Document => "title: none | text: ",
            Self::Query => "task: search result | query: ",
        }
    }
}
/// Captured operation endpoints for the same Google model resource.
pub struct GoogleEndpoints<'a> {
    pub single: &'a str,
    pub batch: &'a str,
}

pub struct GoogleEmbeddings<'a> {
    client: &'a BoundedClient,
    single_endpoint: &'a str,
    batch_endpoint: &'a str,
    model_resource: &'a str,
    dimensions: usize,
    request_dimensions: Option<usize>,
    family: GoogleFamily,
    limits: EmbeddingLimits,
}
impl<'a> GoogleEmbeddings<'a> {
    pub fn new(
        client: &'a BoundedClient,
        endpoints: GoogleEndpoints<'a>,
        model_resource: &'a str,
        dimensions: usize,
        request_dimensions: Option<usize>,
        family: GoogleFamily,
        limits: EmbeddingLimits,
    ) -> Result<Self, EmbeddingError> {
        let GoogleEndpoints {
            single: single_endpoint,
            batch: batch_endpoint,
        } = endpoints;
        if dimensions == 0
            || request_dimensions.is_some_and(|width| width == 0 || width != dimensions)
        {
            return Err(EmbeddingError::InvalidRequest);
        }
        if dimensions > limits.max_dimensions
            || [single_endpoint, batch_endpoint, model_resource]
                .iter()
                .any(|s| s.len() > limits.request_json_bytes)
        {
            return Err(EmbeddingError::Limit);
        }
        let name = model_resource
            .strip_prefix("models/")
            .ok_or(EmbeddingError::InvalidRequest)?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            || !coherent_endpoint(single_endpoint, model_resource, ":embedContent")
            || !coherent_endpoint(batch_endpoint, model_resource, ":batchEmbedContents")
        {
            return Err(EmbeddingError::InvalidRequest);
        }
        Ok(Self {
            client,
            single_endpoint,
            batch_endpoint,
            model_resource,
            dimensions,
            request_dimensions,
            family,
            limits,
        })
    }
    pub async fn embed(
        &self,
        texts: &[&str],
        input: GoogleInput,
        key: &SecretString,
        context: InferenceContext<'_>,
    ) -> Result<EmbeddingBatch<'a>, EmbeddingError> {
        let mut gate = || Ok::<(), std::convert::Infallible>(());
        self.limits
            .validate_input(texts, self.dimensions, context)?;
        let headers = if texts.is_empty() {
            HeaderMap::new()
        } else {
            crate::embedding::credential_headers(key.expose_secret(), true, self.limits)?
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
        input: GoogleInput,
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
            let prefix = if self.family == GoogleFamily::Gemini2 {
                input.prefix()
            } else {
                ""
            };
            // Include every prefix before allocating or opening any socket.
            let mut bytes = 0usize;
            for text in texts {
                context.check()?;
                bytes = bytes
                    .checked_add(text.len())
                    .and_then(|n| n.checked_add(prefix.len()))
                    .ok_or(EmbeddingError::Limit)?;
                if bytes > self.limits.max_input_bytes {
                    return Err(EmbeddingError::Limit.into());
                }
            }
            if texts.is_empty() {
                check_embedding_admission(gate, context)?;
                return Ok(());
            }
            let headers = crate::embedding::configured_headers(headers, self.limits, context)?;
            for chunk in texts.chunks(self.limits.max_batch) {
                check_embedding_admission(gate, context)?;
                self.encode(chunk, input, prefix, context)?;
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
            for chunk in texts.chunks(self.limits.max_batch) {
                check_embedding_admission(gate, context)?;
                let body = self.encode(chunk, input, prefix, context)?;
                let single = chunk.len() == 1;
                let response = self
                    .client
                    .execute_guarded(
                        NetworkRequest {
                            method: Method::POST,
                            url: if single {
                                self.single_endpoint
                            } else {
                                self.batch_endpoint
                            },
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
                let decoded = decode(
                    response.body(),
                    chunk.len(),
                    self.dimensions,
                    single,
                    context,
                );
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
                self.model_resource,
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
        input: GoogleInput,
        prefix: &str,
        context: InferenceContext<'_>,
    ) -> Result<Zeroizing<Vec<u8>>, EmbeddingError> {
        let mut prepared = Vec::new();
        prepared
            .try_reserve_exact(texts.len())
            .map_err(|_| EmbeddingError::Allocation)?;
        for text in texts {
            context.check()?;
            let len = text
                .len()
                .checked_add(prefix.len())
                .ok_or(EmbeddingError::Limit)?;
            let mut value = Zeroizing::new(String::new());
            value
                .try_reserve_exact(len)
                .map_err(|_| EmbeddingError::Allocation)?;
            value.push_str(prefix);
            value.push_str(text);
            prepared.push(value);
        }
        let mut requests = Vec::new();
        requests
            .try_reserve_exact(prepared.len())
            .map_err(|_| EmbeddingError::Allocation)?;
        for text in &prepared {
            context.check()?;
            requests.push(Request {
                model: self.model_resource,
                content: Content {
                    parts: [Part { text }],
                },
                config: Config {
                    output_dimensionality: self.request_dimensions,
                    task_type: (self.family == GoogleFamily::Gemini1).then(|| input.task_type()),
                },
            });
        }
        let mut writer = BoundedWriter {
            bytes: Zeroizing::new(Vec::new()),
            cap: self.limits.request_json_bytes,
            context,
            failure: None,
        };
        let result = if requests.len() == 1 {
            let request = &requests[0];
            serde_json::to_writer(
                &mut writer,
                &SingleRequest {
                    content: &request.content,
                    config: &request.config,
                },
            )
        } else {
            serde_json::to_writer(
                &mut writer,
                &BatchRequest {
                    requests: &requests,
                },
            )
        };
        if result.is_err() {
            return Err(writer.failure.unwrap_or(EmbeddingError::Codec));
        }
        context.check()?;
        Ok(writer.bytes)
    }
}
fn coherent_endpoint(endpoint: &str, resource: &str, operation: &str) -> bool {
    let Ok(uri) = endpoint.parse::<Uri>() else {
        return false;
    };
    matches!(uri.scheme_str(), Some("http" | "https"))
        && uri.authority().is_some()
        && uri.query().is_none()
        && uri
            .path()
            .strip_suffix(operation)
            .and_then(|p| p.strip_suffix(resource))
            .is_some_and(|p| p.ends_with('/'))
}
#[derive(Serialize)]
struct Part<'a> {
    text: &'a str,
}
#[derive(Serialize)]
struct Content<'a> {
    parts: [Part<'a>; 1],
}
#[derive(Serialize)]
struct Config {
    #[serde(rename = "outputDimensionality")]
    #[serde(skip_serializing_if = "Option::is_none")]
    output_dimensionality: Option<usize>,
    #[serde(rename = "taskType", skip_serializing_if = "Option::is_none")]
    task_type: Option<&'static str>,
}
#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    content: Content<'a>,
    #[serde(rename = "embedContentConfig")]
    config: Config,
}
#[derive(Serialize)]
struct SingleRequest<'a> {
    content: &'a Content<'a>,
    #[serde(rename = "embedContentConfig")]
    config: &'a Config,
}
#[derive(Serialize)]
struct BatchRequest<'a> {
    requests: &'a [Request<'a>],
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
    single: bool,
    context: InferenceContext<'_>,
) -> Result<crate::embedding_wire::Decoded, EmbeddingError> {
    crate::embedding_wire::decode(
        body,
        count,
        width,
        if single {
            crate::embedding_wire::Format::GoogleSingle
        } else {
            crate::embedding_wire::Format::GoogleBatch
        },
        context,
    )
}
