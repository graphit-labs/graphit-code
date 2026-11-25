//! Shared bounded decoding for the existing embedding protocols.
use crate::{context::InferenceContext, embedding::EmbeddingError};
use serde::{
    Deserialize,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use serde_json::value::RawValue;
use std::fmt;

#[derive(Clone, Copy)]
pub(crate) enum Format {
    Indexed,
    Cohere,
    GoogleSingle,
    GoogleBatch,
}
pub(crate) struct Decoded {
    pub entries: Vec<(usize, Vec<f32>)>,
    pub failure: Option<EmbeddingError>,
    pub usage: Option<Box<RawValue>>,
}
#[derive(Deserialize)]
struct Response<'a> {
    #[serde(borrow)]
    data: Option<&'a RawValue>,
    #[serde(borrow)]
    embeddings: Option<&'a RawValue>,
    #[serde(borrow)]
    embedding: Option<&'a RawValue>,
    #[serde(borrow)]
    usage: Option<&'a RawValue>,
    #[serde(borrow)]
    meta: Option<&'a RawValue>,
    #[serde(borrow, rename = "usageMetadata")]
    usage_metadata: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct FloatEmbeddings<'a> {
    #[serde(borrow)]
    float: &'a RawValue,
}
#[derive(Deserialize)]
struct Values<'a> {
    #[serde(borrow)]
    values: &'a RawValue,
}
#[derive(Deserialize)]
struct Meta<'a> {
    #[serde(borrow)]
    billed_units: Option<&'a RawValue>,
}

pub(crate) fn decode(
    body: &[u8],
    count: usize,
    width: usize,
    format: Format,
    context: InferenceContext<'_>,
) -> Result<Decoded, EmbeddingError> {
    context.check()?;
    // Complete JSON and EOF are established before retaining any usable result.
    // RawValue borrows each payload; it does not create an unbounded JSON DOM.
    let response: Response<'_> = serde_json::from_slice(body).map_err(|_| EmbeddingError::Codec)?;
    context.check()?;
    let rows = match format {
        Format::Indexed => response.data.ok_or(EmbeddingError::Shape)?,
        Format::Cohere => {
            serde_json::from_str::<FloatEmbeddings<'_>>(
                response.embeddings.ok_or(EmbeddingError::Shape)?.get(),
            )
            .map_err(|_| EmbeddingError::Shape)?
            .float
        }
        Format::GoogleSingle => response.embedding.ok_or(EmbeddingError::Shape)?,
        Format::GoogleBatch => response.embeddings.ok_or(EmbeddingError::Shape)?,
    };
    let usage = response
        .usage
        .or(response.usage_metadata)
        .or_else(|| {
            response
                .meta
                .and_then(|meta| serde_json::from_str::<Meta<'_>>(meta.get()).ok())
                .and_then(|meta| meta.billed_units)
        })
        .map(RawValue::to_owned);
    let mut decoder = serde_json::Deserializer::from_str(rows.get());
    let mut decoded = if matches!(format, Format::GoogleSingle) {
        if count != 1 {
            return Err(EmbeddingError::Shape);
        }
        let mut result = Decoded {
            entries: Vec::new(),
            failure: None,
            usage: None,
        };
        match google_vector(rows, width, context) {
            Ok(vector) => result.entries.push((0, vector)),
            Err(error) => result.failure = Some(error),
        }
        result
    } else {
        let result = Rows {
            count,
            width,
            format,
            context,
        }
        .deserialize(&mut decoder);
        context.check()?;
        result.map_err(|_| EmbeddingError::Shape)?
    };
    context.check()?;
    decoded.usage = usage;
    Ok(decoded)
}

struct Rows<'a> {
    count: usize,
    width: usize,
    format: Format,
    context: InferenceContext<'a>,
}
impl<'de> DeserializeSeed<'de> for Rows<'_> {
    type Value = Decoded;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Decoded, D::Error> {
        d.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for Rows<'_> {
    type Value = Decoded;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("embedding rows")
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Decoded, S::Error> {
        let mut vectors = Vec::new();
        vectors
            .try_reserve_exact(self.count)
            .map_err(|_| de::Error::custom("embedding allocation failed"))?;
        vectors.resize_with(self.count, || None);
        let mut seen = vec![false; self.count];
        let mut received = 0usize;
        let mut first = None;
        while let Some(raw) = seq.next_element::<&RawValue>()? {
            self.context
                .check()
                .map_err(|_| de::Error::custom("embedding operation ended"))?;
            match self.format {
                Format::Indexed => {
                    let mut parser = serde_json::Deserializer::from_str(raw.get());
                    match (IndexedRow {
                        count: self.count,
                        context: self.context,
                    })
                    .deserialize(&mut parser)
                    {
                        Ok(row) => {
                            let valid =
                                row.fields == 1 && row.indices.len() == 1 && row.vector_fields == 1;
                            let decoded = if valid {
                                row.vector.ok_or(EmbeddingError::Shape).and_then(|vector| {
                                    finite_vector(vector, self.width, self.context)
                                })
                            } else {
                                Err(EmbeddingError::Shape)
                            };
                            let mut vector = match decoded {
                                Ok(value) => Some(value),
                                Err(error) => {
                                    first.get_or_insert(error);
                                    None
                                }
                            };
                            for index in row.indices {
                                if seen[index] || !valid {
                                    vectors[index] = None;
                                    first.get_or_insert(EmbeddingError::Shape);
                                } else {
                                    vectors[index] = vector.take();
                                }
                                seen[index] = true;
                            }
                        }
                        Err(_) => {
                            first.get_or_insert(EmbeddingError::Shape);
                        }
                    }
                }
                Format::Cohere | Format::GoogleBatch => {
                    if received < self.count {
                        let decoded = if matches!(self.format, Format::Cohere) {
                            finite_vector(raw, self.width, self.context)
                        } else {
                            google_vector(raw, self.width, self.context)
                        };
                        match decoded {
                            Ok(vector) => vectors[received] = Some(vector),
                            Err(error) => {
                                first.get_or_insert(error);
                            }
                        }
                    }
                }
                Format::GoogleSingle => unreachable!("single responses use the values object"),
            }
            received = received
                .checked_add(1)
                .ok_or_else(|| de::Error::custom("embedding cardinality overflow"))?;
        }
        if received != self.count {
            first.get_or_insert(EmbeddingError::Shape);
            if !matches!(self.format, Format::Indexed) {
                vectors.iter_mut().for_each(|vector| *vector = None);
            }
        }
        if vectors.iter().any(Option::is_none) {
            first.get_or_insert(EmbeddingError::Shape);
        }
        self.context
            .check()
            .map_err(|_| de::Error::custom("embedding operation ended"))?;
        let entries = vectors
            .into_iter()
            .enumerate()
            .filter_map(|(index, vector)| vector.map(|vector| (index, vector)))
            .collect();
        Ok(Decoded {
            entries,
            failure: first,
            usage: None,
        })
    }
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "lowercase")]
enum Field {
    Index,
    Embedding,
    #[serde(other)]
    Other,
}
struct IndexedRow<'a> {
    count: usize,
    context: InferenceContext<'a>,
}
struct IndexedValue<'a> {
    indices: Vec<usize>,
    fields: usize,
    vector: Option<&'a RawValue>,
    vector_fields: usize,
}
impl<'de> DeserializeSeed<'de> for IndexedRow<'_> {
    type Value = IndexedValue<'de>;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for IndexedRow<'_> {
    type Value = IndexedValue<'de>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("indexed embedding row")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut row = IndexedValue {
            indices: Vec::new(),
            fields: 0,
            vector: None,
            vector_fields: 0,
        };
        while let Some(field) = map.next_key::<Field>()? {
            self.context
                .check()
                .map_err(|_| de::Error::custom("embedding operation ended"))?;
            match field {
                Field::Index => {
                    row.fields = row.fields.saturating_add(1);
                    let value = map.next_value::<&RawValue>()?;
                    if let Ok(index) = serde_json::from_str::<usize>(value.get())
                        && index < self.count
                        && !row.indices.contains(&index)
                    {
                        row.indices
                            .try_reserve(1)
                            .map_err(|_| de::Error::custom("embedding allocation failed"))?;
                        row.indices.push(index);
                    }
                }
                Field::Embedding => {
                    row.vector_fields = row.vector_fields.saturating_add(1);
                    row.vector = Some(map.next_value::<&RawValue>()?);
                }
                Field::Other => {
                    map.next_value::<de::IgnoredAny>()?;
                }
            }
        }
        Ok(row)
    }
}

fn google_vector(
    raw: &RawValue,
    width: usize,
    context: InferenceContext<'_>,
) -> Result<Vec<f32>, EmbeddingError> {
    let value: Values<'_> = serde_json::from_str(raw.get()).map_err(|_| EmbeddingError::Shape)?;
    finite_vector(value.values, width, context)
}
fn finite_vector(
    raw: &RawValue,
    width: usize,
    context: InferenceContext<'_>,
) -> Result<Vec<f32>, EmbeddingError> {
    let mut decoder = serde_json::Deserializer::from_str(raw.get());
    let mut error = None;
    let result = Vector {
        width,
        context,
        error: &mut error,
    }
    .deserialize(&mut decoder);
    context.check()?;
    result.map_err(|_| error.unwrap_or(EmbeddingError::Codec))
}
struct Vector<'a, 'e> {
    width: usize,
    context: InferenceContext<'a>,
    error: &'e mut Option<EmbeddingError>,
}
impl<'de> DeserializeSeed<'de> for Vector<'_, '_> {
    type Value = Vec<f32>;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Vec<f32>, D::Error> {
        d.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for Vector<'_, '_> {
    type Value = Vec<f32>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("finite embedding vector")
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Vec<f32>, S::Error> {
        let mut vector = Vec::new();
        vector
            .try_reserve_exact(self.width)
            .map_err(|_| de::Error::custom("embedding allocation failed"))?;
        while let Some(value) = seq.next_element::<f64>()? {
            self.context
                .check()
                .map_err(|_| de::Error::custom("embedding operation ended"))?;
            let value = value as f32;
            if vector.len() == self.width || !value.is_finite() {
                *self.error = Some(if value.is_finite() {
                    EmbeddingError::Shape
                } else {
                    EmbeddingError::NonFinite
                });
                return Err(de::Error::custom("invalid embedding vector"));
            }
            vector.push(value);
        }
        if vector.len() != self.width {
            *self.error = Some(EmbeddingError::Shape);
            return Err(de::Error::custom("invalid embedding width"));
        }
        Ok(vector)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::time::{Duration, Instant};
    fn run(body: &str, count: usize, format: Format) -> Result<Decoded, EmbeddingError> {
        let cancel = Cancellation::default();
        decode(
            body.as_bytes(),
            count,
            2,
            format,
            InferenceContext::new(&cancel, Instant::now() + Duration::from_secs(2)),
        )
    }
    #[test]
    fn indexed_partial_is_unique_and_duplicate_indices_invalidate_all_candidates() {
        let result=run(r#"{"data":[{"index":0,"embedding":[1,2]},{"index":1,"embedding":[3,4]},{"index":1,"embedding":[5,6]}]}"#,3,Format::Indexed).unwrap();
        assert_eq!(result.entries, vec![(0, vec![1., 2.])]);
        assert_eq!(result.failure, Some(EmbeddingError::Shape));
        let result=run(r#"{"data":[{"index":0,"embedding":[1,2]},{"index":1,"index":0,"embedding":[3,4]},{"index":2,"embedding":[5,6]}]}"#,3,Format::Indexed).unwrap();
        assert_eq!(result.entries, vec![(2, vec![5., 6.])]);
        assert!(result.failure.is_some());
    }
    #[test]
    fn complete_json_required_and_missing_indexed_rows_preserve_only_known_inputs() {
        assert!(
            run(
                r#"{"data":[{"index":0,"embedding":[1,2]}]"#,
                2,
                Format::Indexed
            )
            .is_err()
        );
        assert!(
            run(
                r#"{"data":[{"index":0,"embedding":[1,2]}]} trailing"#,
                2,
                Format::Indexed
            )
            .is_err()
        );
        let result = run(
            r#"{"data":[{"index":1,"embedding":[1,2]}],"usage":{"total_tokens":7}}"#,
            2,
            Format::Indexed,
        )
        .unwrap();
        assert_eq!(result.entries, vec![(1, vec![1., 2.])]);
        assert_eq!(result.usage.unwrap().get(), r#"{"total_tokens":7}"#);
    }
    #[test]
    fn cohere_and_google_keep_valid_positions_only_when_cardinality_is_exact() {
        let cohere = run(
            r#"{"embeddings":{"float":[[1,2],[3],[4,5]]}}"#,
            3,
            Format::Cohere,
        )
        .unwrap();
        assert_eq!(cohere.entries, vec![(0, vec![1., 2.]), (2, vec![4., 5.])]);
        assert!(cohere.failure.is_some());
        let short = run(r#"{"embeddings":{"float":[[1,2]]}}"#, 2, Format::Cohere).unwrap();
        assert!(short.entries.is_empty());
        let google = run(
            r#"{"embeddings":[{"values":[1,2]},{"values":[1e39,0]}]}"#,
            2,
            Format::GoogleBatch,
        )
        .unwrap();
        assert_eq!(google.entries, vec![(0, vec![1., 2.])]);
        assert_eq!(google.failure, Some(EmbeddingError::NonFinite));
        let single = run(r#"{"embedding":{"values":[1,2]}}"#, 1, Format::GoogleSingle).unwrap();
        assert!(single.failure.is_none());
    }
}
