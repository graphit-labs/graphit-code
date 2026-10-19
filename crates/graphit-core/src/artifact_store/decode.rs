//! Policy-bound decoding. Raw enum bodies are borrowed, never expanded to Value.
use super::*;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::marker::PhantomData;

#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub copy: CopyLimits,
}
impl Limits {
    pub fn from_store(store: &ArtifactStore) -> Self {
        Self {
            copy: store.limits.copy,
        }
    }
}
#[derive(Clone, Copy)]
pub(super) struct StringSeed(pub usize);
impl<'de> DeserializeSeed<'de> for StringSeed {
    type Value = String;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<String, D::Error> {
        d.deserialize_str(self)
    }
}
impl<'de> Visitor<'de> for StringSeed {
    type Value = String;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a bounded string")
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<String, E> {
        if value.len() > self.0 {
            return Err(E::custom("string capacity exceeded"));
        }
        Ok(value.to_owned())
    }
}
struct ValueSeed<T>(PhantomData<T>);
impl<T> Copy for ValueSeed<T> {}
impl<T> Clone for ValueSeed<T> {
    fn clone(&self) -> Self {
        *self
    }
}
fn value<T>() -> ValueSeed<T> {
    ValueSeed(PhantomData)
}
impl<'de, T: Deserialize<'de>> DeserializeSeed<'de> for ValueSeed<T> {
    type Value = T;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<T, D::Error> {
        T::deserialize(d)
    }
}
#[derive(Clone, Copy)]
struct Optional<S>(S);
impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for Optional<S> {
    type Value = Option<S::Value>;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_option(self)
    }
}
impl<'de, S: DeserializeSeed<'de>> Visitor<'de> for Optional<S> {
    type Value = Option<S::Value>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an optional bounded value")
    }
    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_some<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.0.deserialize(d).map(Some)
    }
}
struct Refuse<T>(PhantomData<T>);
impl<'de, T> DeserializeSeed<'de> for Refuse<T> {
    type Value = T;
    fn deserialize<D: serde::Deserializer<'de>>(self, _: D) -> Result<T, D::Error> {
        Err(de::Error::custom("sequence capacity exceeded"))
    }
}
#[derive(Clone, Copy)]
pub(super) struct Sequence<S> {
    pub seed: S,
}
impl<'de, S: DeserializeSeed<'de> + Copy> DeserializeSeed<'de> for Sequence<S> {
    type Value = Vec<S::Value>;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_seq(self)
    }
}
impl<'de, S: DeserializeSeed<'de> + Copy> Visitor<'de> for Sequence<S> {
    type Value = Vec<S::Value>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a catalogue sequence")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut result = Vec::new();
        while let Some(item) = seq.next_element_seed(self.seed)? {
            result.push(item);
        }
        Ok(result)
    }
}
macro_rules! record {
    ($name:ident => $ty:path, $ctx:ident, [$($tag:literal => $expected:literal),*], {$($field:ident => $seed:expr),+ $(,)?}) => {
        #[derive(Clone,Copy)] pub(super) struct $name(pub Limits);
        impl<'de> DeserializeSeed<'de> for $name {
            type Value=$ty;
            fn deserialize<D:serde::Deserializer<'de>>(self,d:D)->Result<Self::Value,D::Error>{d.deserialize_map(self)}
        }
        impl<'de> Visitor<'de> for $name {
            type Value=$ty;
            fn expecting(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str("a closed policy-bounded record")}
            fn visit_map<A:MapAccess<'de>>(self,mut map:A)->Result<Self::Value,A::Error>{
                let $ctx=self.0;let _=$ctx;
                $(let mut $field=None;)+
                let mut tags=std::collections::HashSet::<String>::new();
                while let Some(key)=map.next_key_seed(StringSeed(64))?{
                    if !tags.insert(key.clone()) { return Err(de::Error::custom("duplicate record field")); }
                    match key.as_str(){
                        $(stringify!($field)=>{if $field.is_some(){return Err(de::Error::custom("duplicate record field"));}$field=Some(map.next_value_seed($seed)?);},)+
                        $($tag=>{if map.next_value_seed(StringSeed(32))? != $expected {return Err(de::Error::custom("invalid record tag"));}},)*
                        _=>return Err(de::Error::custom("unknown record field")),
                    }
                }
                $(if !tags.contains($tag){return Err(de::Error::missing_field($tag));})*
                Ok(Self::Value{$($field:$field.ok_or_else(||de::Error::missing_field(stringify!($field)))?),+})
            }
        }
    }
}
#[derive(Clone, Copy)]
struct TreeSeed(CopyLimits);
impl<'de> DeserializeSeed<'de> for TreeSeed {
    type Value = tree::TreeReceipt;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for TreeSeed {
    type Value = tree::TreeReceipt;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a bounded tree receipt")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut nodes = None;
        while let Some(key) = map.next_key_seed(StringSeed(16))? {
            if key != "nodes" || nodes.is_some() {
                return Err(de::Error::custom("invalid tree field"));
            }
            nodes = Some(map.next_value_seed(NodesSeed(self.0))?);
        }
        Ok(tree::TreeReceipt {
            nodes: nodes.ok_or_else(|| de::Error::missing_field("nodes"))?,
        })
    }
}
#[derive(Clone, Copy)]
struct NodesSeed(CopyLimits);
impl<'de> DeserializeSeed<'de> for NodesSeed {
    type Value = Vec<tree::NodeReceipt>;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for NodesSeed {
    type Value = Vec<tree::NodeReceipt>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("nodes within the current native tree budget")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut result = Vec::new();
        let mut budget = tree::Budget::new(self.0);
        loop {
            let next = if result.len() == self.0.max_tree_nodes {
                seq.next_element_seed(Refuse(PhantomData))?
            } else {
                seq.next_element::<tree::NodeReceipt>()?
            };
            let Some(node) = next else {
                return Ok(result);
            };
            budget
                .add(&node.relative.0, node.content.bytes())
                .map_err(de::Error::custom)?;
            result.push(node);
        }
    }
}
record!(CandidateSeed => codec::Candidate, limits, [], {key=>value(),slot=>value(),source=>value(),generated_origin=>value(),destination=>value(),kind=>value()});
record!(EntrySeed => codec::ArtifactEntry, limits, [], {candidate=>CandidateSeed(limits),receipt=>ProofSeed(limits)});
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkBody {
    mode: String,
    receipt: super::receipt::Receipt,
}
record!(CopyBodySeed => CopyBody, limits, ["mode"=>"Copy"], {policy=>value(),tree=>TreeSeed(limits.copy)});
pub(super) struct CopyBody {
    policy: CopyPolicy,
    tree: tree::TreeReceipt,
}
record!(InspectBodySeed => InspectBody, limits, ["operation"=>"Inspect"], {entry=>EntrySeed(limits)});
pub(super) struct InspectBody {
    entry: ArtifactEntry,
}
record!(PublicationBodySeed => publication::Operation, limits, ["operation"=>"Publication"], {
    candidate=>CandidateSeed(limits),mode=>value(),stage=>value(),old_stage=>value(),prepared=>Optional(ProofSeed(limits)),old=>Optional(EntrySeed(limits)),phase=>value(),goal=>value(),aborting=>value(),read_access=>value(),movement=>value()
});
record!(PublicationRecordSeed => publication::Operation, limits, [], {
    candidate=>CandidateSeed(limits),mode=>value(),stage=>value(),old_stage=>value(),prepared=>Optional(ProofSeed(limits)),old=>Optional(EntrySeed(limits)),phase=>value(),goal=>value(),aborting=>value(),read_access=>value(),movement=>value()
});
record!(NativeDocumentBodySeed => super::native_document::Operation, limits, ["operation"=>"NativeDocument"], {
    id=>StringSeed(26),key=>value(),destination=>value(),parent=>value(),previous=>value(),state=>value()
});
#[derive(Clone, Copy)]
struct ProofSeed(Limits);
impl<'de> DeserializeSeed<'de> for ProofSeed {
    type Value = Proof;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Proof, D::Error> {
        let raw = <&serde_json::value::RawValue>::deserialize(d)?;
        match tag(raw, "mode").map_err(de::Error::custom)?.as_str() {
            "Link" => {
                let body: LinkBody = serde_json::from_str(raw.get()).map_err(de::Error::custom)?;
                if body.mode != "Link" {
                    return Err(de::Error::custom("invalid link tag"));
                }
                Ok(Proof::Link {
                    receipt: body.receipt,
                })
            }
            "Copy" => {
                let body = unpack(raw, CopyBodySeed(self.0)).map_err(de::Error::custom)?;
                Ok(Proof::Copy {
                    policy: body.policy,
                    tree: body.tree,
                })
            }
            _ => Err(de::Error::custom("invalid proof mode")),
        }
    }
}
#[derive(Clone, Copy)]
pub(super) struct PendingSeed(pub Limits);
impl PendingSeed {
    pub fn optional(limits: Limits) -> impl for<'de> DeserializeSeed<'de, Value = Option<Pending>> {
        Optional(Self(limits))
    }
}
impl<'de> DeserializeSeed<'de> for PendingSeed {
    type Value = Pending;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Pending, D::Error> {
        let raw = <&serde_json::value::RawValue>::deserialize(d)?;
        let result = match tag(raw, "operation").map_err(de::Error::custom)?.as_str() {
            "NativeDocument" => unpack(raw, NativeDocumentBodySeed(self.0))
                .map(|op| Pending::NativeDocument(Box::new(op))),
            "Publication" => unpack(raw, PublicationBodySeed(self.0))
                .map(|op| Pending::Publication(Box::new(op))),
            "Inspect" => unpack(raw, InspectBodySeed(self.0)).map(|body| Pending::Inspect {
                entry: Box::new(body.entry),
            }),
            _ => return Err(de::Error::custom("invalid pending operation")),
        };
        result.map_err(de::Error::custom)
    }
}
fn unpack<'de, S: DeserializeSeed<'de>>(
    raw: &'de serde_json::value::RawValue,
    seed: S,
) -> Result<S::Value, serde_json::Error> {
    let mut decoder = serde_json::Deserializer::from_str(raw.get());
    let value = seed.deserialize(&mut decoder)?;
    decoder.end()?;
    Ok(value)
}
fn tag(raw: &serde_json::value::RawValue, name: &str) -> Result<String, serde_json::Error> {
    struct Tag<'a>(&'a str);
    impl<'de> Visitor<'de> for Tag<'_> {
        type Value = String;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a closed enum discriminator")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<String, A::Error> {
            let mut tag = None;
            let mut fields = std::collections::HashSet::new();
            while let Some(key) = map.next_key_seed(StringSeed(64))? {
                if fields.len() >= 32 || !fields.insert(key.clone()) {
                    return Err(de::Error::custom("duplicate or excessive enum field"));
                }
                if key == self.0 {
                    tag = Some(map.next_value_seed(StringSeed(32))?);
                } else {
                    map.next_value::<de::IgnoredAny>()?;
                }
            }
            tag.ok_or_else(|| de::Error::custom("missing enum discriminator"))
        }
    }
    let mut decoder = serde_json::Deserializer::from_str(raw.get());
    serde::Deserializer::deserialize_map(&mut decoder, Tag(name))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn limits() -> Limits {
        let copy = CopyLimits::new(1, 1, 1, 1).unwrap();
        Limits { copy }
    }
    fn node() -> tree::NodeReceipt {
        tree::NodeReceipt {
            relative: tree::RelativePath(PathBuf::new()),
            id: tree::PhysicalId::largest(),
            mode: if cfg!(unix) {
                tree::Mode::Unix(0o700)
            } else {
                tree::Mode::Windows(false)
            },
            content: tree::Content::Directory,
            verified: None,
        }
    }
    #[test]
    fn receipt_capacity_refuses_excess_body_before_node_decode() {
        let first = serde_json::to_string(&node()).unwrap();
        let exact = format!("{{\"nodes\":[{first}]}}");
        assert_eq!(
            TreeSeed(limits().copy)
                .deserialize(&mut serde_json::Deserializer::from_str(&exact))
                .unwrap()
                .nodes
                .len(),
            1
        );
        let excess = format!("{{\"nodes\":[{first}, {{malformed-body]}}");
        let error = TreeSeed(limits().copy)
            .deserialize(&mut serde_json::Deserializer::from_str(&excess))
            .err()
            .unwrap();
        assert!(
            error.to_string().contains("sequence capacity exceeded"),
            "{error}"
        );
    }
    #[test]
    fn string_seed_checks_decoded_length_before_owned_field_allocation() {
        assert_eq!(
            StringSeed(3)
                .deserialize(&mut serde_json::Deserializer::from_str("\"h\\u0075b\""))
                .unwrap(),
            "hub"
        );
        assert!(
            StringSeed(2)
                .deserialize(&mut serde_json::Deserializer::from_str("\"h\\u0075b\""))
                .unwrap_err()
                .to_string()
                .contains("capacity")
        );
    }
}
