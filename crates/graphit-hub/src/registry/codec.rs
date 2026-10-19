//! Bounded strict JSON, including duplicates inside otherwise unknown fields.
use super::{HubDocument, HubLimits, RawHubDocument, invalid};
use serde::{
    Serialize,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;
use std::{
    cell::Cell,
    fmt,
    io::{self, Write},
};

struct CountWriter {
    bytes: usize,
    limit: usize,
}
impl Write for CountWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .filter(|n| *n <= self.limit)
            .ok_or_else(invalid)?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(super) fn count(document: &HubDocument, limits: HubLimits) -> io::Result<usize> {
    let mut writer = CountWriter {
        bytes: 0,
        limit: limits.document_bytes,
    };
    serde_json::to_writer(&mut writer, document).map_err(|_| invalid())?;
    Ok(writer.bytes)
}
pub(super) fn encode(
    document: &HubDocument,
    limits: HubLimits,
    bytes: usize,
) -> io::Result<Vec<u8>> {
    if bytes > limits.document_bytes {
        return Err(invalid());
    }
    let mut output = Vec::with_capacity(bytes);
    document
        .serialize(&mut serde_json::Serializer::new(&mut output))
        .map_err(|_| invalid())?;
    if output.len() != bytes {
        return Err(invalid());
    }
    // Preflight the complete document, including path strings and intents.
    strict(&output, limits)?;
    Ok(output)
}
pub(super) fn decode(bytes: &[u8], limits: HubLimits) -> io::Result<RawHubDocument> {
    strict(bytes, limits)?;
    serde_json::from_slice(bytes).map_err(|_| invalid())
}
fn strict(bytes: &[u8], limits: HubLimits) -> io::Result<Value> {
    if bytes.len() > limits.document_bytes {
        return Err(invalid());
    }
    let nodes = Cell::new(0usize);
    let entries = Cell::new(0usize);
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let value = Seed {
        limits,
        nodes: &nodes,
        entries: &entries,
        context: Context::Root,
        depth: 0,
    }
    .deserialize(&mut decoder)
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    decoder.end().map_err(|_| invalid())?;
    Ok(value)
}
#[derive(Clone, Copy)]
struct Seed<'a> {
    limits: HubLimits,
    nodes: &'a Cell<usize>,
    entries: &'a Cell<usize>,
    context: Context,
    depth: usize,
}
#[derive(Clone, Copy)]
enum Context {
    Root,
    ArtifactTypes,
    Entries,
    Versions,
    Version,
    Other,
    Membership,
    Parents,
    Identity,
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<Value, D::Error> {
        if matches!(self.context, Context::Version) {
            let entries = self
                .entries
                .get()
                .checked_add(1)
                .filter(|n| *n <= self.limits.entries)
                .ok_or_else(|| de::Error::custom("artifact entry limit exceeded"))?;
            self.entries.set(entries);
        }
        let nodes = self
            .nodes
            .get()
            .checked_add(1)
            .filter(|n| *n <= self.limits.json_nodes)
            .ok_or_else(|| de::Error::custom("JSON node budget exceeded"))?;
        if self.depth > 32 {
            return Err(de::Error::custom("JSON depth exceeded"));
        }
        self.nodes.set(nodes);
        decoder.deserialize_any(self)
    }
}
impl Seed<'_> {
    fn child(self) -> Self {
        Self {
            depth: self.depth + 1,
            context: Context::Other,
            ..self
        }
    }
    fn string<E: de::Error>(self, value: &str) -> Result<(), E> {
        if matches!(self.context, Context::Identity)
            && self.limits.projection.identity(value).is_err()
        {
            return Err(E::custom("identity budget exceeded"));
        }
        if value.len() > self.limits.json_string_bytes {
            Err(E::custom("JSON string budget exceeded"))
        } else {
            Ok(())
        }
    }
}
struct Refuse;
impl<'de> DeserializeSeed<'de> for Refuse {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(self, _: D) -> Result<Value, D::Error> {
        Err(de::Error::custom("array budget exceeded"))
    }
}

impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("finite non-null JSON without duplicate keys")
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Err(E::custom("required JSON data must not be null"))
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(value.into())
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(value.into())
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(value.into())
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("nonfinite number"))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        self.string(value)?;
        Ok(value.into())
    }
    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
        self.string(&value)?;
        Ok(value.into())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        let cap = match self.context {
            Context::Versions => self.limits.entries,
            Context::Membership => crate::NativeBinding::ALL.len(),
            Context::Parents => 2,
            _ => self.limits.json_nodes,
        };
        loop {
            let value = if values.len() == cap {
                sequence.next_element_seed(Refuse)?
            } else {
                let mut child = self.child();
                child.context = match self.context {
                    Context::Versions => Context::Version,
                    _ => child.context,
                };
                sequence.next_element_seed(child)?
            };
            let Some(value) = value else {
                break;
            };
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut values = serde_json::Map::new();
        // Keys use the same bounded seed; duplicate detection precedes decoding its value.
        let mut key_seed = self.child();
        if matches!(self.context, Context::Entries) {
            key_seed.context = Context::Identity;
        }
        while let Some(key) = map.next_key_seed(key_seed)? {
            let Value::String(key) = key else {
                return Err(de::Error::custom("object key must be string"));
            };
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate JSON key"));
            }
            let mut child = self.child();
            match self.context {
                Context::Root if key == "artifacts" => child.context = Context::ArtifactTypes,
                Context::ArtifactTypes => child.context = Context::Entries,
                Context::Entries => {
                    child.context = Context::Versions;
                    if key.len() > self.limits.key_bytes {
                        return Err(de::Error::custom("artifact key budget exceeded"));
                    }
                }
                _ => {
                    child.context = match key.as_str() {
                        "agents" | "installations" => Context::Membership,
                        "possible_parents" => Context::Parents,
                        "id" | "physical_id" | "version" => Context::Identity,
                        _ => Context::Other,
                    };
                }
            }
            let value = map.next_value_seed(child)?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

#[cfg(test)]
#[test]
fn excess_array_item_is_refused_before_its_invalid_body_is_decoded() {
    let limits = super::tests::limits();
    let nodes = Cell::new(0);
    let entries = Cell::new(0);
    let mut decoder = serde_json::Deserializer::from_slice(b"[1,2,{invalid]");
    let error = Seed {
        limits,
        nodes: &nodes,
        entries: &entries,
        context: Context::Parents,
        depth: 0,
    }
    .deserialize(&mut decoder)
    .unwrap_err();
    assert!(error.to_string().contains("array budget exceeded"));
    assert_eq!(
        nodes.get(),
        3,
        "root and two values; excess body was not decoded"
    );
    let mut decoder = serde_json::Deserializer::from_slice(b"[1,2]");
    let nodes = Cell::new(0);
    assert!(
        Seed {
            limits,
            nodes: &nodes,
            entries: &entries,
            context: Context::Parents,
            depth: 0
        }
        .deserialize(&mut decoder)
        .is_ok()
    );
}
