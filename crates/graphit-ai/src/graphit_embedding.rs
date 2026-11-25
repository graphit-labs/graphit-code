//! Graphit embeddings v1 through the existing guarded HTTP and vector decoder.
use crate::{
    context::InferenceContext,
    embedding::{
        EmbeddingBatch, EmbeddingError, EmbeddingFailure, EmbeddingLimits, GuardedEmbeddingError,
        check_embedding_admission, configured_headers,
    },
    embedding_wire::{self, Decoded, Format},
};
use graphit_auth::broker::BrokerEmbeddingService;
use graphit_http::client::{
    BoundedClient, GuardedNetworkError, NetworkEffect, NetworkRequest, NetworkRequestContext,
};
use http::{HeaderMap, Method};
use secrecy::zeroize::Zeroizing;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, DeserializeSeed, IgnoredAny, SeqAccess, Visitor},
};
use serde_json::value::RawValue;
use std::{
    borrow::Cow,
    collections::BTreeMap,
    fmt,
    io::{self, Write},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphitEmbeddingInput {
    Query,
    Document,
}

/// The descriptor is checked configuration data; the caller must independently
/// retain and recheck its real Auth lease through `embed_guarded`'s gate.
pub struct GraphitEmbeddings<'a> {
    client: &'a BoundedClient,
    service: &'a BrokerEmbeddingService,
    limits: EmbeddingLimits,
    width: usize,
}
impl<'a> GraphitEmbeddings<'a> {
    pub fn new(
        client: &'a BoundedClient,
        service: &'a BrokerEmbeddingService,
        limits: EmbeddingLimits,
    ) -> Result<Self, EmbeddingError> {
        let width = usize::try_from(service.dimensions).map_err(|_| EmbeddingError::Limit)?;
        if width == 0
            || service.max_batch == 0
            || service.max_input_bytes == 0
            || [
                &service.endpoint,
                &service.identity.model,
                &service.identity.model_revision,
                &service.identity.recipe,
                &service.identity.service_revision,
            ]
            .iter()
            .any(|s| s.trim().is_empty())
        {
            return Err(EmbeddingError::InvalidRequest);
        }
        if width > limits.max_dimensions
            || [
                &service.endpoint,
                &service.identity.model,
                &service.identity.model_revision,
                &service.identity.recipe,
                &service.identity.service_revision,
            ]
            .iter()
            .any(|s| s.len() > limits.request_json_bytes)
        {
            return Err(EmbeddingError::Limit);
        }
        Ok(Self {
            client,
            service,
            limits,
            width,
        })
    }

    pub async fn embed_guarded<G, E>(
        &self,
        texts: &[&str],
        input: GraphitEmbeddingInput,
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
            self.limits.validate_input(texts, self.width, context)?;
            if texts.is_empty() {
                check_embedding_admission(gate, context)?;
                return Ok(());
            }
            let headers = configured_headers(headers, self.limits, context)?;
            // Validate every planned request before the first network effect.
            let mut start = 0;
            while start < texts.len() {
                check_embedding_admission(gate, context)?;
                let end = self.batch_end(texts, start, context)?;
                self.encode(&texts[start..end], input, context)?;
                start = end;
            }
            texts
                .len()
                .checked_mul(self.limits.response_json_bytes)
                .filter(|n| *n <= isize::MAX as usize)
                .ok_or(EmbeddingError::Limit)?;
            completed
                .try_reserve_exact(texts.len())
                .map_err(|_| EmbeddingError::Allocation)?;
            usage
                .try_reserve_exact(texts.len())
                .map_err(|_| EmbeddingError::Allocation)?;
            start = 0;
            while start < texts.len() {
                check_embedding_admission(gate, context)?;
                let end = self.batch_end(texts, start, context)?;
                let body = self.encode(&texts[start..end], input, context)?;
                let response = self
                    .client
                    .execute_guarded(
                        NetworkRequest {
                            method: Method::POST,
                            url: &self.service.endpoint,
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
                let decoded = decode_response(
                    response.body(),
                    response.headers(),
                    self.service,
                    end - start,
                    self.width,
                    context,
                );
                let admitted = check_embedding_admission(gate, context);
                context.check()?;
                let decoded = decoded?;
                admitted?;
                completed.extend(
                    decoded
                        .entries
                        .into_iter()
                        .map(|(index, vector)| (start + index, vector)),
                );
                if let Some(observed) = decoded.usage {
                    usage.push(observed);
                }
                if let Some(error) = decoded.failure {
                    return Err(error.into());
                }
                start = end;
            }
            check_embedding_admission(gate, context)
        }
        .await;
        match result {
            Ok(()) => Ok(EmbeddingBatch::new(
                &self.service.identity.model,
                self.width,
                completed.into_iter().map(|(_, vector)| vector).collect(),
            )
            .with_usage(usage)),
            Err(error) => Err(error.with_effect(effect).with_completed(completed, usage)),
        }
    }

    fn batch_end(
        &self,
        texts: &[&str],
        start: usize,
        context: InferenceContext<'_>,
    ) -> Result<usize, EmbeddingError> {
        let count = self.limits.max_batch.min(self.service.max_batch);
        let mut bytes = 0usize;
        let mut end = start;
        while end < texts.len() && end - start < count {
            context.check()?;
            let next = bytes
                .checked_add(texts[end].len())
                .ok_or(EmbeddingError::Limit)?;
            if next > self.service.max_input_bytes {
                break;
            }
            bytes = next;
            end += 1;
        }
        if end == start {
            return Err(EmbeddingError::Limit);
        }
        Ok(end)
    }
    fn encode(
        &self,
        input: &[&str],
        input_type: GraphitEmbeddingInput,
        context: InferenceContext<'_>,
    ) -> Result<Zeroizing<Vec<u8>>, EmbeddingError> {
        let mut writer = BoundedWriter {
            bytes: Zeroizing::new(Vec::new()),
            cap: self.limits.request_json_bytes,
            context,
            failure: None,
        };
        if serde_json::to_writer(&mut writer, &Request { input, input_type }).is_err() {
            return Err(writer.failure.unwrap_or(EmbeddingError::Codec));
        }
        context.check()?;
        Ok(writer.bytes)
    }
}

#[derive(Serialize)]
struct Request<'a> {
    input: &'a [&'a str],
    input_type: GraphitEmbeddingInput,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResponseIdentity<'a> {
    #[serde(borrow)]
    revision: Cow<'a, str>,
    #[serde(borrow)]
    model: Cow<'a, str>,
    #[serde(borrow)]
    model_revision: Cow<'a, str>,
    #[serde(borrow)]
    recipe: Cow<'a, str>,
    dimensions: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response<'a> {
    #[serde(borrow)]
    object: Cow<'a, str>,
    #[serde(borrow)]
    data: &'a RawValue,
    #[serde(borrow)]
    graphit: ResponseIdentity<'a>,
    #[serde(default, borrow, deserialize_with = "present_raw")]
    usage: Option<&'a RawValue>,
}
fn present_raw<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<&'de RawValue>, D::Error> {
    <&RawValue>::deserialize(deserializer).map(Some)
}
fn exact_header(headers: &HeaderMap, name: &'static str, expected: &str) -> bool {
    let mut values = headers.get_all(name).iter();
    matches!((values.next(), values.next()), (Some(value), None) if value.as_bytes() == expected.as_bytes())
}
fn valid_usage(raw: &RawValue) -> bool {
    type Object = BTreeMap<String, IgnoredAny>;
    serde_json::from_str::<Object>(raw.get()).is_ok()
        || serde_json::from_str::<Vec<Object>>(raw.get()).is_ok()
}
fn decode_response(
    body: &[u8],
    headers: &HeaderMap,
    service: &BrokerEmbeddingService,
    count: usize,
    width: usize,
    context: InferenceContext<'_>,
) -> Result<Decoded, EmbeddingError> {
    context.check()?;
    let response = serde_json::from_slice::<Response<'_>>(body);
    context.check()?;
    let response = response.map_err(|_| EmbeddingError::Codec)?;
    let identity = &service.identity;
    if response.object != "list"
        || response.graphit.revision.as_ref() != identity.service_revision
        || response.graphit.model.as_ref() != identity.model
        || response.graphit.model_revision.as_ref() != identity.model_revision
        || response.graphit.recipe.as_ref() != identity.recipe
        || response.graphit.dimensions != service.dimensions
        || !exact_header(
            headers,
            "x-graphit-embedding-revision",
            &identity.service_revision,
        )
        || !exact_header(
            headers,
            "x-graphit-embedding-dimensions",
            &service.dimensions.to_string(),
        )
        || response.usage.is_some_and(|raw| !valid_usage(raw))
    {
        return Err(EmbeddingError::Shape);
    }
    // Validate the protocol discriminator with bounded Serde iteration. The
    // existing indexed decoder still owns indices, vectors, finite values and EOF.
    let mut data = serde_json::Deserializer::from_str(response.data.get());
    let objects = ObjectRows { count, context }.deserialize(&mut data);
    context.check()?;
    objects.map_err(|_| EmbeddingError::Shape)?;
    data.end().map_err(|_| EmbeddingError::Codec)?;
    context.check()?;
    embedding_wire::decode(body, count, width, Format::Indexed, context)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObjectRow<'a> {
    #[serde(borrow)]
    object: Cow<'a, str>,
    index: usize,
    #[serde(borrow)]
    embedding: &'a RawValue,
}
struct ObjectRows<'a> {
    count: usize,
    context: InferenceContext<'a>,
}
impl<'de> DeserializeSeed<'de> for ObjectRows<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for ObjectRows<'_> {
    type Value = ();
    fn expecting(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str("bounded Graphit embedding objects")
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut rows: S) -> Result<(), S::Error> {
        let mut count = 0usize;
        while let Some(row) = rows.next_element::<ObjectRow<'_>>()? {
            self.context
                .check()
                .map_err(|_| de::Error::custom("embedding operation ended"))?;
            if count >= self.count || row.object != "embedding" || row.index >= self.count {
                return Err(de::Error::custom("Graphit embedding object shape"));
            }
            let _vector = row.embedding;
            count += 1;
        }
        if count != self.count {
            return Err(de::Error::custom("Graphit embedding object count"));
        }
        Ok(())
    }
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
        result.map_err(|error| {
            self.failure = Some(error);
            io::Error::other("Graphit embedding encoding failed")
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
use crate::rerank_http as http_fixture;

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_auth::broker::BrokerModelIdentity;
    use graphit_core::file_lock::Cancellation;
    use graphit_http::client::{ClientPolicy, Limits};
    use serde_json::{Value, json};
    use std::time::{Duration, Instant};

    fn service() -> BrokerEmbeddingService {
        BrokerEmbeddingService {
            endpoint: "http://127.0.0.1:9000/embedding".into(),
            identity: BrokerModelIdentity {
                model: "model".into(),
                model_revision: "upstream-r1".into(),
                recipe: "retrieval-v1".into(),
                service_revision: "service-v1".into(),
            },
            dimensions: 2,
            max_batch: 2,
            max_input_bytes: 5,
        }
    }
    fn body() -> Value {
        json!({"object":"list","data":[
        {"object":"embedding","index":1,"embedding":[0.3,0.4]},
        {"object":"embedding","index":0,"embedding":[0.1,0.2]}],
        "graphit":{"revision":"service-v1","model":"model","model_revision":"upstream-r1","recipe":"retrieval-v1","dimensions":2}})
    }
    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-graphit-embedding-revision",
            "service-v1".parse().unwrap(),
        );
        headers.insert("x-graphit-embedding-dimensions", "2".parse().unwrap());
        headers
    }
    fn context(cancel: &Cancellation) -> InferenceContext<'_> {
        InferenceContext::new(cancel, Instant::now() + Duration::from_secs(5))
    }
    fn decode(
        value: &Value,
        headers: &HeaderMap,
        context: InferenceContext<'_>,
    ) -> Result<Decoded, EmbeddingError> {
        decode_response(
            &serde_json::to_vec(value).unwrap(),
            headers,
            &service(),
            2,
            2,
            context,
        )
    }
    fn limits() -> EmbeddingLimits {
        EmbeddingLimits::new(16, 4096, 4, 64, 8, 8192, 8192).unwrap()
    }
    fn client(origin: &str) -> BoundedClient {
        BoundedClient::new(
            ClientPolicy {
                origin,
                allow_loopback_http: true,
                proxy: None,
                additional_roots: &[],
            },
            Limits {
                request_body_bytes: 8192,
                response_body_bytes: 8192,
                url_bytes: 2048,
                header_bytes: 4096,
                header_count: 32,
                max_in_flight: 1,
                max_idle_per_host: 1,
                idle_timeout: Duration::from_secs(1),
                max_certificates: 1,
                max_certificate_bytes: 4096,
            },
        )
        .unwrap()
    }
    fn response_headers() -> Vec<(String, String)> {
        vec![
            ("x-graphit-embedding-revision".into(), "service-v1".into()),
            ("x-graphit-embedding-dimensions".into(), "2".into()),
        ]
    }
    #[tokio::test]
    async fn actual_http_chunks_preserve_input_duplicates_order_and_observed_usage() {
        let mut first = body();
        first["usage"] = json!({"tokens":5});
        let peer = http_fixture::fixture_with_headers(
            vec![http_fixture::reply(first), http_fixture::reply(body())],
            vec![response_headers(), response_headers()],
        )
        .await;
        let client = client(&peer.origin);
        let mut service = service();
        service.endpoint = format!("{}/embedding", peer.origin);
        service.max_input_bytes = 100;
        let model = GraphitEmbeddings::new(&client, &service, limits()).unwrap();
        let cancel = Cancellation::default();
        let mut checks = 0;
        let output = model
            .embed_guarded(
                &["dup", "dup", "third", "last"],
                GraphitEmbeddingInput::Document,
                &HeaderMap::new(),
                context(&cancel),
                &mut || {
                    checks += 1;
                    Ok::<(), ()>(())
                },
            )
            .await
            .unwrap();
        assert_eq!(
            output.vectors(),
            &[
                vec![0.1, 0.2],
                vec![0.3, 0.4],
                vec![0.1, 0.2],
                vec![0.3, 0.4]
            ]
        );
        assert_eq!(output.usage().len(), 1);
        assert!(checks > 2);
        peer.observed.notified().await;
        let requests = peer.server.await.unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0].body,
            json!({"input":["dup","dup"],"input_type":"document"})
        );
        assert_eq!(
            requests[1].body,
            json!({"input":["third","last"],"input_type":"document"})
        );
        assert!(requests[0].head.starts_with("POST /embedding "));
    }
    #[tokio::test]
    async fn actual_http_metadata_change_retains_only_previously_confirmed_items() {
        let mut changed = body();
        changed["graphit"]["model_revision"] = json!("new-upstream");
        let peer = http_fixture::fixture_with_headers(
            vec![http_fixture::reply(body()), http_fixture::reply(changed)],
            vec![response_headers(), response_headers()],
        )
        .await;
        let client = client(&peer.origin);
        let mut service = service();
        service.endpoint = format!("{}/embedding", peer.origin);
        service.max_input_bytes = 100;
        let model = GraphitEmbeddings::new(&client, &service, limits()).unwrap();
        let cancel = Cancellation::default();
        let error = model
            .embed_guarded(
                &["one", "two", "three", "four"],
                GraphitEmbeddingInput::Query,
                &HeaderMap::new(),
                context(&cancel),
                &mut || Ok::<(), ()>(()),
            )
            .await
            .err()
            .unwrap();
        assert_eq!(error.effect(), NetworkEffect::MayHaveApplied);
        assert_eq!(
            error.completed(),
            &[(0, vec![0.1, 0.2]), (1, vec![0.3, 0.4])]
        );
        assert!(matches!(
            error,
            GuardedEmbeddingError::Embedding {
                source: EmbeddingError::Shape,
                ..
            }
        ));
        assert_eq!(peer.server.await.unwrap().len(), 2);
    }
    #[test]
    fn encoding_and_service_limits_preserve_utf8_without_openai_fields() {
        let client = client("http://127.0.0.1:9000");
        let service = service();
        let model = GraphitEmbeddings::new(&client, &service, limits()).unwrap();
        let cancel = Cancellation::default();
        let texts = ["é", "é", "aaa"];
        assert_eq!(model.batch_end(&texts, 0, context(&cancel)).unwrap(), 2);
        assert_eq!(model.batch_end(&texts, 2, context(&cancel)).unwrap(), 3);
        let body = model
            .encode(&texts[..2], GraphitEmbeddingInput::Query, context(&cancel))
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            json!({"input":["é","é"],"input_type":"query"})
        );
        assert!(model.batch_end(&["123456"], 0, context(&cancel)).is_err());
    }
    #[test]
    fn identity_order_and_usage_are_preserved_without_inventing_cache_usage() {
        let cancel = Cancellation::default();
        let plain = decode(&body(), &headers(), context(&cancel)).unwrap();
        assert_eq!(
            plain.entries,
            vec![(0, vec![0.1, 0.2]), (1, vec![0.3, 0.4])]
        );
        assert!(plain.failure.is_none() && plain.usage.is_none());
        let mut observed = body();
        observed["usage"] = json!([{"tokens":5},{"tokens":8}]);
        let usage = decode(&observed, &headers(), context(&cancel))
            .unwrap()
            .usage
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(usage.get()).unwrap(),
            observed["usage"]
        );
        observed["usage"] = Value::Null;
        assert!(decode(&observed, &headers(), context(&cancel)).is_err());
    }
    #[test]
    fn identity_headers_objects_and_eof_are_checked_before_vectors() {
        let cancel = Cancellation::default();
        for field in ["revision", "model", "model_revision", "recipe"] {
            let mut changed = body();
            changed["graphit"][field] = json!("changed");
            assert!(
                decode(&changed, &headers(), context(&cancel)).is_err(),
                "{field}"
            );
        }
        let mut changed = body();
        changed["graphit"]["dimensions"] = json!(3);
        assert!(decode(&changed, &headers(), context(&cancel)).is_err());
        let mut missing = headers();
        missing.remove("x-graphit-embedding-revision");
        assert!(decode(&body(), &missing, context(&cancel)).is_err());
        let mut duplicate = headers();
        duplicate.append("x-graphit-embedding-dimensions", "2".parse().unwrap());
        assert!(decode(&body(), &duplicate, context(&cancel)).is_err());
        let mut changed = body();
        changed["data"][0]["object"] = json!("other");
        assert!(decode(&changed, &headers(), context(&cancel)).is_err());
        let mut extra = serde_json::to_vec(&body()).unwrap();
        extra.extend_from_slice(b" {}");
        assert!(decode_response(&extra, &headers(), &service(), 2, 2, context(&cancel)).is_err());
    }
}
