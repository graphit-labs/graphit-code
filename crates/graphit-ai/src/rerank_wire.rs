//! Private bounded wire pipeline used by native reranking adapters.
use crate::{
    context::InferenceContext,
    native_rerank::{
        IndexScore, NativeRerankError, NativeRerankLimits, RerankFailure, check_admission,
        validate_results,
    },
};
use graphit_auth::broker::BrokerModelIdentity;
use graphit_http::client::{
    BoundedClient, GuardedNetworkError, NetworkEffect, NetworkRequest, NetworkRequestContext,
};
use http::{HeaderMap, HeaderValue, Method, header};
use secrecy::zeroize::Zeroizing;
use serde::{
    Deserialize, Serialize,
    de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor},
};
use std::{
    cell::Cell,
    fmt,
    io::{self, Write},
    mem::size_of,
};

#[derive(Clone, Copy)]
pub(crate) enum Envelope<'a> {
    Results,
    Data,
    Graphit(&'a BrokerModelIdentity),
}

pub(crate) struct WireBatch<'a> {
    pub(crate) body: Zeroizing<Vec<u8>>,
    pub(crate) expected_count: usize,
    pub(crate) envelope: Envelope<'a>,
}

pub(crate) fn encode_request<T: Serialize>(
    request: &T,
    limits: NativeRerankLimits,
    context: InferenceContext<'_>,
) -> Result<Zeroizing<Vec<u8>>, NativeRerankError> {
    context.check()?;
    let mut writer = BoundedWriter {
        bytes: Zeroizing::new(Vec::new()),
        cap: limits.max_request_json,
        context,
        failure: None,
    };
    if serde_json::to_writer(&mut writer, request).is_err() {
        return Err(writer.failure.unwrap_or(NativeRerankError::Encoding));
    }
    context.check()?;
    Ok(writer.bytes)
}

pub(crate) struct CallAdmission<'a, G> {
    pub(crate) gate: &'a mut G,
    pub(crate) effect: &'a mut NetworkEffect,
}

pub(crate) async fn request_scores<G, E>(
    client: &BoundedClient,
    endpoint: &str,
    batch: WireBatch<'_>,
    credentials: Credentials<'_>,
    limits: NativeRerankLimits,
    context: InferenceContext<'_>,
    admission: CallAdmission<'_, G>,
) -> Result<Vec<f64>, RerankFailure<E>>
where
    G: FnMut() -> Result<(), E>,
{
    check_admission(admission.gate, context)?;
    if batch.expected_count == 0 {
        return Err(NativeRerankError::Shape.into());
    }
    if batch.expected_count > limits.max_candidates
        || batch.expected_count > limits.max_batch
        || batch
            .expected_count
            .checked_mul(size_of::<IndexScore>().max(size_of::<Option<f64>>()))
            .is_none_or(|bytes| bytes > isize::MAX as usize)
        || batch.body.len() > limits.max_request_json
    {
        return Err(NativeRerankError::Limit.into());
    }
    let headers = match credentials {
        Credentials::Configured(headers) => configured_headers(headers, limits, context)?,
        Credentials::Bearer(key) => {
            configured_headers(&credential_headers(key, limits, context)?, limits, context)?
        }
    };
    let response_result = client
        .execute_guarded(
            NetworkRequest {
                method: Method::POST,
                url: endpoint,
                headers: &headers,
                body: &batch.body,
            },
            NetworkRequestContext::new(context.deadline(), context.cancellation()),
            admission.gate,
        )
        .await;
    let response = match response_result {
        Ok(response) => {
            *admission.effect = NetworkEffect::MayHaveApplied;
            response
        }
        Err(error) => {
            if error.effect() == NetworkEffect::MayHaveApplied {
                *admission.effect = NetworkEffect::MayHaveApplied;
            }
            return Err(match error {
                GuardedNetworkError::Network { source, .. } => RerankFailure::Native(source.into()),
                GuardedNetworkError::Admission { source, .. } => RerankFailure::Admission(source),
            });
        }
    };
    check_admission(admission.gate, context)?;
    if !response.status().is_success() {
        return Err(NativeRerankError::Status(response.status()).into());
    }
    if response.body().len() > limits.max_response_json {
        return Err(NativeRerankError::Limit.into());
    }
    let decoded = decode(
        response.body(),
        batch.expected_count,
        limits,
        context,
        batch.envelope,
    );
    context.check()?;
    let admitted = (admission.gate)().map_err(RerankFailure::Admission);
    context.check()?;
    let scores = decoded?;
    admitted?;
    Ok(scores)
}
pub(crate) fn credential_headers(
    key: &str,
    limits: NativeRerankLimits,
    context: InferenceContext<'_>,
) -> Result<HeaderMap, NativeRerankError> {
    context.check()?;
    if key.trim().is_empty() {
        return Err(NativeRerankError::Policy);
    }
    let len = key.len().checked_add(7).ok_or(NativeRerankError::Limit)?;
    if len > limits.max_request_json {
        return Err(NativeRerankError::Limit);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    bytes
        .try_reserve_exact(len)
        .map_err(|_| NativeRerankError::Allocation)?;
    bytes.extend_from_slice(b"Bearer ");
    bytes.extend_from_slice(key.as_bytes());
    let mut value = HeaderValue::from_bytes(&bytes).map_err(|_| NativeRerankError::Policy)?;
    value.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, value);
    Ok(headers)
}

#[derive(Clone, Copy)]
pub(crate) enum Credentials<'a> {
    Configured(&'a HeaderMap),
    Bearer(&'a str),
}

fn configured_headers(
    headers: &HeaderMap,
    limits: NativeRerankLimits,
    context: InferenceContext<'_>,
) -> Result<HeaderMap, NativeRerankError> {
    context.check()?;
    if headers.contains_key(header::CONTENT_TYPE) {
        return Err(NativeRerankError::Policy);
    }
    let mut size = 0usize;
    for (name, value) in headers {
        context.check()?;
        size = size
            .checked_add(name.as_str().len())
            .and_then(|n| n.checked_add(value.as_bytes().len()))
            .filter(|n| *n <= limits.max_request_json)
            .ok_or(NativeRerankError::Limit)?;
    }
    let mut result = HeaderMap::try_with_capacity(
        headers
            .len()
            .checked_add(1)
            .ok_or(NativeRerankError::Limit)?,
    )
    .map_err(|_| NativeRerankError::Allocation)?;
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

struct BoundedWriter<'a> {
    bytes: Zeroizing<Vec<u8>>,
    cap: usize,
    context: InferenceContext<'a>,
    failure: Option<NativeRerankError>,
}
impl Write for BoundedWriter<'_> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let result = (|| {
            self.context.check()?;
            let required = self
                .bytes
                .len()
                .checked_add(data.len())
                .ok_or(NativeRerankError::Limit)?;
            if required > self.cap {
                return Err(NativeRerankError::Limit);
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
                    .map_err(|_| NativeRerankError::Allocation)?;
            }
            self.bytes.extend_from_slice(data);
            Ok(data.len())
        })();
        result.map_err(|e| {
            self.failure = Some(e);
            io::Error::other("rerank encoding failed")
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct DecodeState<'a> {
    count: usize,
    envelope: Envelope<'a>,
    context: InferenceContext<'a>,
    failure: &'a Cell<Option<NativeRerankError>>,
}
impl DecodeState<'_> {
    fn fail<E: de::Error>(self, error: NativeRerankError) -> E {
        self.failure.set(Some(error));
        E::custom("invalid rerank response")
    }
    fn check<E: de::Error>(self) -> Result<(), E> {
        self.context.check().map_err(|e| self.fail(e.into()))
    }
}
#[derive(Deserialize, PartialEq, Eq)]
#[serde(field_identifier, rename_all = "snake_case")]
enum Field {
    Results,
    Data,
    Index,
    RelevanceScore,
    Graphit,
    #[serde(other)]
    Other,
}
fn decode(
    body: &[u8],
    count: usize,
    limits: NativeRerankLimits,
    context: InferenceContext<'_>,
    envelope: Envelope<'_>,
) -> Result<Vec<f64>, NativeRerankError> {
    let failure = Cell::new(None);
    let state = DecodeState {
        count,
        envelope,
        context,
        failure: &failure,
    };
    let mut decoder = serde_json::Deserializer::from_slice(body);
    let rows = ResponseSeed(state)
        .deserialize(&mut decoder)
        .map_err(|_| failure.get().unwrap_or(NativeRerankError::Encoding))?;
    decoder.end().map_err(|_| NativeRerankError::Encoding)?;
    validate_results(count, &rows, limits, context)
}
struct ResponseSeed<'a>(DecodeState<'a>);
impl<'de> DeserializeSeed<'de> for ResponseSeed<'_> {
    type Value = Vec<IndexScore>;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.0.check()?;
        d.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for ResponseSeed<'_> {
    type Value = Vec<IndexScore>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("native rerank object")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut rows = None;
        let mut identity_seen = false;
        while let Some(field) = map.next_key::<Field>()? {
            self.0.check()?;
            if field
                == match self.0.envelope {
                    Envelope::Results | Envelope::Graphit(_) => Field::Results,
                    Envelope::Data => Field::Data,
                }
            {
                if rows.is_some() {
                    return Err(self.0.fail(NativeRerankError::Shape));
                }
                rows = Some(map.next_value_seed(RowsSeed(self.0))?);
            } else if field == Field::Graphit && matches!(self.0.envelope, Envelope::Graphit(_)) {
                if identity_seen {
                    return Err(self.0.fail(NativeRerankError::Shape));
                }
                map.next_value_seed(IdentitySeed(self.0))?;
                identity_seen = true;
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        self.0.check()?;
        if matches!(self.0.envelope, Envelope::Graphit(_)) && !identity_seen {
            return Err(self.0.fail(NativeRerankError::Shape));
        }
        rows.ok_or_else(|| self.0.fail(NativeRerankError::Shape))
    }
}
#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum IdentityField {
    Revision,
    Model,
    ModelRevision,
    Recipe,
    #[serde(other)]
    Other,
}
struct IdentitySeed<'a>(DecodeState<'a>);
impl<'de> DeserializeSeed<'de> for IdentitySeed<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.0.check()?;
        d.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for IdentitySeed<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the captured Graphit service and model identity")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<(), M::Error> {
        let Envelope::Graphit(expected) = self.0.envelope else {
            return Err(self.0.fail(NativeRerankError::Shape));
        };
        let mut seen = 0u8;
        while let Some(field) = map.next_key::<IdentityField>()? {
            self.0.check()?;
            let (bit, wanted) = match field {
                IdentityField::Revision => (1, expected.service_revision.as_str()),
                IdentityField::Model => (2, expected.model.as_str()),
                IdentityField::ModelRevision => (4, expected.model_revision.as_str()),
                IdentityField::Recipe => (8, expected.recipe.as_str()),
                IdentityField::Other => {
                    map.next_value::<IgnoredAny>()?;
                    continue;
                }
            };
            if seen & bit != 0 {
                return Err(self.0.fail(NativeRerankError::Shape));
            }
            let observed = map.next_value::<String>()?;
            if observed != wanted {
                return Err(self.0.fail(NativeRerankError::Shape));
            }
            seen |= bit;
        }
        self.0.check()?;
        if seen != 15 {
            return Err(self.0.fail(NativeRerankError::Shape));
        }
        Ok(())
    }
}
struct RowsSeed<'a>(DecodeState<'a>);
impl<'de> DeserializeSeed<'de> for RowsSeed<'_> {
    type Value = Vec<IndexScore>;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.0.check()?;
        d.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for RowsSeed<'_> {
    type Value = Vec<IndexScore>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("complete bounded results array")
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Self::Value, S::Error> {
        let mut rows = Vec::new();
        while let Some(row) = seq.next_element_seed(RowSeed(self.0))? {
            self.0.check()?;
            if rows.len() == self.0.count {
                return Err(self.0.fail(NativeRerankError::Shape));
            }
            if rows.len() == rows.capacity() {
                let target = rows
                    .capacity()
                    .max(1)
                    .checked_mul(2)
                    .unwrap_or(self.0.count)
                    .min(self.0.count);
                rows.try_reserve_exact(target - rows.len())
                    .map_err(|_| self.0.fail(NativeRerankError::Allocation))?;
            }
            rows.push(row);
        }
        if rows.len() != self.0.count {
            return Err(self.0.fail(NativeRerankError::Shape));
        }
        Ok(rows)
    }
}
struct RowSeed<'a>(DecodeState<'a>);
impl<'de> DeserializeSeed<'de> for RowSeed<'_> {
    type Value = IndexScore;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.0.check()?;
        d.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for RowSeed<'_> {
    type Value = IndexScore;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("required index and finite native score")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut index = None;
        let mut score = None;
        while let Some(field) = map.next_key::<Field>()? {
            self.0.check()?;
            match field {
                Field::Index => {
                    if index.is_some() {
                        return Err(self.0.fail(NativeRerankError::Shape));
                    }
                    index = Some(map.next_value::<usize>()?);
                }
                Field::RelevanceScore => {
                    if score.is_some() {
                        return Err(self.0.fail(NativeRerankError::Shape));
                    }
                    let value = map.next_value::<f64>()?;
                    if !value.is_finite() {
                        return Err(self.0.fail(NativeRerankError::Shape));
                    }
                    score = Some(value);
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        self.0.check()?;
        let index = index.ok_or_else(|| self.0.fail(NativeRerankError::Shape))?;
        if index >= self.0.count {
            return Err(self.0.fail(NativeRerankError::Shape));
        }
        Ok(IndexScore {
            index,
            relevance_score: score.ok_or_else(|| self.0.fail(NativeRerankError::Shape))?,
        })
    }
}

#[cfg(test)]
use crate::rerank_http as guarded_http;
#[cfg(test)]
mod guarded_tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use graphit_http::client::{ClientPolicy, Limits, NetworkEffect};
    use serde_json::json;
    use std::time::{Duration, Instant};

    #[derive(Debug)]
    struct Denied;
    fn policy() -> NativeRerankLimits {
        NativeRerankLimits::new(10, 1, 4096, 4096, 4096).unwrap()
    }
    fn batch(context: InferenceContext<'_>) -> WireBatch<'_> {
        WireBatch {
            body: encode_request(
                &json!({"model":"fixture-model","query":"query","documents":["document"]}),
                policy(),
                context,
            )
            .unwrap(),
            expected_count: 1,
            envelope: Envelope::Results,
        }
    }
    #[tokio::test]
    async fn a_successful_tcp_batch_preserves_effect_when_the_next_admission_denies_before_send() {
        let tcp = guarded_http::fixture(vec![guarded_http::reply(
            json!({"results":[{"index":0,"relevance_score":0.25}]}),
        )])
        .await;
        let client = BoundedClient::new(
            ClientPolicy {
                origin: &tcp.origin,
                allow_loopback_http: true,
                proxy: None,
                additional_roots: &[],
            },
            Limits {
                request_body_bytes: 4096,
                response_body_bytes: 4096,
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
        .unwrap();
        let cancellation = Cancellation::default();
        let context = InferenceContext::new(&cancellation, Instant::now() + Duration::from_secs(5));
        let mut effect = NetworkEffect::NotSent;
        let mut valid = || Ok::<(), Denied>(());
        let first = request_scores(
            &client,
            &tcp.origin,
            batch(context),
            Credentials::Bearer("private-fixture-key"),
            policy(),
            context,
            CallAdmission {
                gate: &mut valid,
                effect: &mut effect,
            },
        );
        let (first, ()) = tokio::join!(first, tcp.observed.notified());
        assert_eq!(first.ok().unwrap(), [0.25]);
        assert_eq!(effect, NetworkEffect::MayHaveApplied);
        let mut deny = || Err(Denied);
        let failure = match request_scores(
            &client,
            &tcp.origin,
            batch(context),
            Credentials::Bearer("private-fixture-key"),
            policy(),
            context,
            CallAdmission {
                gate: &mut deny,
                effect: &mut effect,
            },
        )
        .await
        {
            Err(error) => error.with_effect(effect),
            Ok(_) => panic!("later denied batch returned scores"),
        };
        assert!(matches!(
            failure,
            crate::native_rerank::GuardedRerankError::Admission {
                source: Denied,
                effect: NetworkEffect::MayHaveApplied
            }
        ));
        let captured = tcp.server.await.unwrap();
        assert_eq!(captured.len(), 1);
        assert!(captured[0].head.starts_with("POST / HTTP/1.1"));
        assert_eq!(captured[0].body["documents"].as_array().unwrap().len(), 1);
    }
}
