//! Serialized observations only. Decoding never constructs a captured source.

use super::{
    CopyLimits, CopyPolicy, NativePath, codec::Kind, snapshot::SourceSnapshot, tree::SourceNode,
};
use serde::{
    Deserialize, Serialize,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use std::{fmt, io};

#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct SnapshotBindingRecord {
    source: NativePath,
    canonical_source: NativePath,
    kind: Kind,
    policy: CopyPolicy,
    nodes: Vec<SourceNode>,
}

impl SnapshotBindingRecord {
    pub(super) fn from_snapshot(snapshot: &SourceSnapshot) -> io::Result<Self> {
        Ok(Self {
            source: NativePath::new(snapshot.plan().source().to_owned())?,
            canonical_source: NativePath::new(snapshot.canonical_source().to_owned())?,
            kind: snapshot.kind().into(),
            policy: snapshot.link_policy(),
            nodes: snapshot.plan().nodes().cloned().collect(),
        })
    }

    pub fn pure_restore(
        bytes: &[u8],
        limits: CopyLimits,
        max_document_bytes: usize,
    ) -> io::Result<Self> {
        if max_document_bytes == 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if bytes.len() > max_document_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let mut decoder = serde_json::Deserializer::from_slice(bytes);
        let record = RecordSeed(limits)
            .deserialize(&mut decoder)
            .map_err(|_| io::ErrorKind::InvalidData)?;
        decoder.end().map_err(|_| io::ErrorKind::InvalidData)?;
        record.validate_data(limits)?;
        Ok(record)
    }

    pub fn source(&self) -> &std::path::Path {
        self.source.as_path()
    }
    /// Captured link policy as data; this does not authorize a new read.
    pub fn link_policy(&self) -> CopyPolicy {
        self.policy
    }

    /// Whether a validated record contains this exact relative node and kind.
    /// This compares recorded data without filesystem I/O or granting access.
    pub fn contains_node(&self, relative: &std::path::Path, kind: graphit_fs::NodeKind) -> bool {
        self.nodes
            .binary_search_by(|node| node.relative.0.as_path().cmp(relative))
            .is_ok_and(|index| {
                self.nodes[index].content.directory() == (kind == graphit_fs::NodeKind::Directory)
            })
    }

    /// Fingerprint of this bounded data tree; not a captured-source proof.
    pub fn fingerprint(&self) -> [u8; 32] {
        super::tree::fingerprint(
            self.nodes
                .iter()
                .map(|node| (node.relative.0.as_path(), node.mode, &node.content)),
        )
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn matches_snapshot(&self, snapshot: &SourceSnapshot) -> bool {
        self.source.as_path() == snapshot.plan().source()
            && self.canonical_source.as_path() == snapshot.canonical_source()
            && self.kind == Kind::from(snapshot.kind())
            && self.policy == snapshot.link_policy()
            && self.nodes.iter().eq(snapshot.plan().nodes())
    }

    /// Validate this data record against a caller policy, without filesystem I/O.
    pub fn validate_data(&self, limits: CopyLimits) -> io::Result<()> {
        if self.nodes.len() > limits.max_tree_nodes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let root = self.nodes.first().ok_or(io::ErrorKind::InvalidData)?;
        if !root.relative.0.as_os_str().is_empty()
            || root.content.directory() != (self.kind == Kind::Directory)
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mut bytes = 0u64;
        let mut paths = 0usize;
        for (i, node) in self.nodes.iter().enumerate() {
            node.validate_binding(self.policy)?;
            bytes = bytes
                .checked_add(node.content.bytes())
                .ok_or(io::ErrorKind::FileTooLarge)?;
            paths = paths
                .checked_add(super::snapshot::path_units(&node.relative.0)?)
                .ok_or(io::ErrorKind::FileTooLarge)?;
            if bytes > limits.max_total_file_bytes
                || paths > limits.max_total_relative_path_bytes
                || node.relative.0.components().count() > limits.max_depth
            {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            if i != 0 {
                if self.nodes[i - 1].relative >= node.relative {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                let parent = node.relative.0.parent().ok_or(io::ErrorKind::InvalidData)?;
                let parent_node = self.nodes[..i]
                    .binary_search_by(|n| n.relative.0.as_path().cmp(parent))
                    .map_err(|_| io::ErrorKind::InvalidData)?;
                if !self.nodes[parent_node].content.directory() {
                    return Err(io::ErrorKind::InvalidData.into());
                }
            }
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum Field {
    Source,
    CanonicalSource,
    Kind,
    Policy,
    Nodes,
}

struct RecordSeed(CopyLimits);
impl<'de> DeserializeSeed<'de> for RecordSeed {
    type Value = SnapshotBindingRecord;
    fn deserialize<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<Self::Value, D::Error> {
        struct RecordVisitor(CopyLimits);
        impl<'de> Visitor<'de> for RecordVisitor {
            type Value = SnapshotBindingRecord;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a closed source binding record")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let (mut source, mut canonical_source, mut kind, mut policy, mut nodes) =
                    (None, None, None, None, None);
                while let Some(field) = map.next_key::<Field>()? {
                    match field {
                        Field::Source => {
                            if source.is_some() {
                                return Err(de::Error::duplicate_field("source"));
                            }
                            source = Some(map.next_value()?);
                        }
                        Field::CanonicalSource => {
                            if canonical_source.is_some() {
                                return Err(de::Error::duplicate_field("canonical_source"));
                            }
                            canonical_source = Some(map.next_value()?);
                        }
                        Field::Kind => {
                            if kind.is_some() {
                                return Err(de::Error::duplicate_field("kind"));
                            }
                            kind = Some(map.next_value()?);
                        }
                        Field::Policy => {
                            if policy.is_some() {
                                return Err(de::Error::duplicate_field("policy"));
                            }
                            policy = Some(map.next_value()?);
                        }
                        Field::Nodes => {
                            if nodes.is_some() {
                                return Err(de::Error::duplicate_field("nodes"));
                            }
                            nodes = Some(map.next_value_seed(NodeSeed(self.0))?);
                        }
                    }
                }
                Ok(SnapshotBindingRecord {
                    source: source.ok_or_else(|| de::Error::missing_field("source"))?,
                    canonical_source: canonical_source
                        .ok_or_else(|| de::Error::missing_field("canonical_source"))?,
                    kind: kind.ok_or_else(|| de::Error::missing_field("kind"))?,
                    policy: policy.ok_or_else(|| de::Error::missing_field("policy"))?,
                    nodes: nodes.ok_or_else(|| de::Error::missing_field("nodes"))?,
                })
            }
        }
        decoder.deserialize_map(RecordVisitor(self.0))
    }
}

struct RejectExtraNode;
impl<'de> DeserializeSeed<'de> for RejectExtraNode {
    type Value = SourceNode;
    fn deserialize<D: serde::Deserializer<'de>>(self, _: D) -> Result<Self::Value, D::Error> {
        Err(de::Error::custom("source node budget exceeded"))
    }
}

struct NodeSeed(CopyLimits);
impl<'de> DeserializeSeed<'de> for NodeSeed {
    type Value = Vec<SourceNode>;
    fn deserialize<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<Self::Value, D::Error> {
        struct NodesVisitor(CopyLimits);
        impl<'de> Visitor<'de> for NodesVisitor {
            type Value = Vec<SourceNode>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a bounded source node array")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut nodes = Vec::new();
                let mut budget = super::tree::Budget::new(self.0);
                loop {
                    let next = if nodes.len() == self.0.max_tree_nodes {
                        seq.next_element_seed(RejectExtraNode)?
                    } else {
                        seq.next_element::<SourceNode>()?
                    };
                    let Some(node) = next else {
                        break;
                    };
                    budget
                        .add(&node.relative.0, node.content.bytes())
                        .map_err(de::Error::custom)?;
                    nodes.push(node);
                }
                Ok(nodes)
            }
        }
        decoder.deserialize_seq(NodesVisitor(self.0))
    }
}
