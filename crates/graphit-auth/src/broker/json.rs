//! Admission during Serde parsing, without a second JSON tokenizer.
use super::{BrokerError, BrokerRequestContext, JsonLimits};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

struct Budget<'a> {
    limits: JsonLimits,
    remaining: usize,
    context: BrokerRequestContext<'a>,
    failure: Option<BrokerError>,
}
impl Budget<'_> {
    fn check<E: de::Error>(&mut self) -> Result<(), E> {
        self.context.check().map_err(|error| {
            self.failure = Some(error);
            E::custom("interrupted")
        })
    }
    fn admit<E: de::Error>(&mut self, depth: usize) -> Result<(), E> {
        self.check()?;
        if depth > self.limits.depth || self.remaining == 0 {
            return self.fail(BrokerError::Limit);
        }
        self.remaining -= 1;
        Ok(())
    }
    fn fail<T, E: de::Error>(&mut self, error: BrokerError) -> Result<T, E> {
        self.failure = Some(error);
        Err(E::custom("bounded JSON"))
    }
    fn scalar<E: de::Error>(&mut self, value: &str) -> Result<(), E> {
        self.check()?;
        if value.len() > self.limits.scalar_bytes {
            self.fail(BrokerError::Limit)
        } else {
            Ok(())
        }
    }
}
struct Seed<'b, 'a> {
    budget: &'b mut Budget<'a>,
    depth: usize,
}
struct Slot<'b, 'a> {
    seed: Seed<'b, 'a>,
    available: bool,
}
impl<'de> DeserializeSeed<'de> for Slot<'_, '_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        if !self.available {
            return self.seed.budget.fail(BrokerError::Limit);
        }
        self.seed.deserialize(deserializer)
    }
}
struct Key<'b, 'a> {
    budget: &'b mut Budget<'a>,
    depth: usize,
    available: bool,
}
impl<'de> DeserializeSeed<'de> for Key<'_, '_> {
    type Value = String;
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<String, D::Error> {
        if !self.available {
            return self.budget.fail(BrokerError::Limit);
        }
        self.budget.admit(self.depth)?;
        deserializer.deserialize_str(self)
    }
}
impl Visitor<'_> for Key<'_, '_> {
    type Value = String;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("bounded JSON key")
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<String, E> {
        self.budget.scalar(value)?;
        Ok(value.into())
    }
}
impl<'de> DeserializeSeed<'de> for Seed<'_, '_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        self.budget.admit(self.depth)?;
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed<'_, '_> {
    type Value = Value;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("bounded JSON")
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("number"))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        self.budget.scalar(value)?;
        Ok(Value::String(value.into()))
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Value, M::Error> {
        let mut values = Map::new();
        while let Some(key) = map.next_key_seed(Key {
            available: values.len() < self.budget.limits.collection_entries,
            budget: self.budget,
            depth: self.depth + 1,
        })? {
            if values.contains_key(&key) {
                return self.budget.fail(BrokerError::InvalidDocument);
            }
            let value = map.next_value_seed(Seed {
                budget: self.budget,
                depth: self.depth + 1,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut sequence: S) -> Result<Value, S::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(Slot {
            available: values.len() < self.budget.limits.collection_entries,
            seed: Seed {
                budget: self.budget,
                depth: self.depth + 1,
            },
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
}
pub(super) fn parse(
    input: &[u8],
    limits: JsonLimits,
    context: BrokerRequestContext<'_>,
) -> Result<Value, BrokerError> {
    context.check()?;
    if input.len() > limits.bytes {
        return Err(BrokerError::Limit);
    }
    let mut budget = Budget {
        limits,
        remaining: limits.nodes,
        context,
        failure: None,
    };
    let mut deserializer = serde_json::Deserializer::from_slice(input);
    let result = Seed {
        budget: &mut budget,
        depth: 1,
    }
    .deserialize(&mut deserializer);
    let result = result.map_err(|_| budget.failure.unwrap_or(BrokerError::InvalidDocument))?;
    deserializer
        .end()
        .map_err(|_| BrokerError::InvalidDocument)?;
    context.check()?;
    Ok(result)
}
