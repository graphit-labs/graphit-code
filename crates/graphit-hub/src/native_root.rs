//! Finite native root metadata. Integrity and identity are data, never read admission.
use crate::{
    ArtifactId, ArtifactType, ConcreteVersion,
    policy::RequestContext,
    remote_metadata::{MetadataError, MetadataLimits},
};
use graphit_core::project_identity::ProjectId;
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Write},
    time::Instant,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeRole {
    Graph,
    Source,
    /// The single AST corpus containing text and any nullable NN vectors.
    EntitySearch,
    /// The single Knowledge corpus, including any schema-evolved nullable vectors.
    Wiki,
}
impl NativeRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Graph => "graph",
            Self::Source => "source",
            Self::EntitySearch => "entity_search",
            Self::Wiki => "wiki",
        }
    }
}

/// The physical SDK manifest bytes, not its ETag or a corpus digest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NativeSnapshot {
    Lance {
        dataset_version: u64,
        manifest_path: String,
        #[serde(with = "digest")]
        manifest_sha256: [u8; 32],
    },
    Icebug {
        manifest_path: String,
        generation: String,
        #[serde(with = "digest")]
        manifest_sha256: [u8; 32],
    },
}
impl NativeSnapshot {
    pub fn manifest_path(&self) -> &str {
        match self {
            Self::Lance { manifest_path, .. } | Self::Icebug { manifest_path, .. } => manifest_path,
        }
    }
    pub fn manifest_sha256(&self) -> &[u8; 32] {
        match self {
            Self::Lance {
                manifest_sha256, ..
            }
            | Self::Icebug {
                manifest_sha256, ..
            } => manifest_sha256,
        }
    }
}

/// Publication data; encoding validates these public fields before retaining output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeComponent {
    pub role: NativeRole,
    pub path: String,
    pub snapshot: NativeSnapshot,
}

#[derive(Serialize)]
pub struct NativeRootInput<'a> {
    pub producer: ProjectId,
    pub artifact_type: ArtifactType,
    pub id: &'a str,
    pub version: &'a str,
    pub components: &'a [NativeComponent],
}

#[derive(Debug, Eq, PartialEq)]
pub struct NativeRoot {
    producer: ProjectId,
    artifact_type: ArtifactType,
    id: ArtifactId,
    version: ConcreteVersion,
    components: Vec<NativeComponent>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    format: String,
    v: u8,
    producer: ProjectId,
    artifact_type: ArtifactType,
    id: String,
    version: String,
    components: Vec<NativeComponent>,
}
#[derive(Serialize)]
struct Output<'a, 'b> {
    format: &'static str,
    v: u8,
    #[serde(flatten)]
    input: &'a NativeRootInput<'b>,
}
impl NativeRoot {
    pub fn decode(
        bytes: &[u8],
        limits: MetadataLimits,
        context: &RequestContext<'_>,
    ) -> Result<Self, MetadataError> {
        policy(limits, context)?;
        if bytes.len() > limits.bytes {
            return Err(MetadataError::Limit);
        }
        let parsed = serde_json::from_slice::<Wire>(bytes);
        check(context)?;
        let wire = parsed.map_err(|_| MetadataError::InvalidDocument)?;
        if wire.format != "graphit-native-root" || wire.v != 1 {
            return Err(MetadataError::InvalidDocument);
        }
        let (id, version) = validate(
            &NativeRootInput {
                producer: wire.producer,
                artifact_type: wire.artifact_type,
                id: &wire.id,
                version: &wire.version,
                components: &wire.components,
            },
            limits,
            context,
        )?;
        Ok(Self {
            producer: wire.producer,
            artifact_type: wire.artifact_type,
            id,
            version,
            components: wire.components,
        })
    }

    pub fn encode(
        input: &NativeRootInput<'_>,
        limits: MetadataLimits,
        context: &RequestContext<'_>,
    ) -> Result<Vec<u8>, MetadataError> {
        policy(limits, context)?;
        validate(input, limits, context)?;
        let mut writer = OutputWriter {
            bytes: Vec::new(),
            limit: limits.bytes,
            context,
            failure: None,
        };
        let output = Output {
            format: "graphit-native-root",
            v: 1,
            input,
        };
        let result = serde_json::to_writer(&mut writer, &output);
        check(context)?;
        result.map_err(|_| writer.failure.unwrap_or(MetadataError::InvalidDocument))?;
        Ok(writer.bytes)
    }

    pub fn producer(&self) -> ProjectId {
        self.producer
    }
    pub fn artifact_type(&self) -> ArtifactType {
        self.artifact_type
    }
    pub fn id(&self) -> &ArtifactId {
        &self.id
    }
    pub fn version(&self) -> &ConcreteVersion {
        &self.version
    }
    pub fn components(&self) -> &[NativeComponent] {
        &self.components
    }
}

fn check(context: &RequestContext<'_>) -> Result<(), MetadataError> {
    if context.cancellation().is_cancelled() {
        Err(MetadataError::Cancelled)
    } else if context
        .deadline()
        .is_some_and(|deadline| Instant::now() >= deadline)
    {
        Err(MetadataError::Deadline)
    } else {
        Ok(())
    }
}
fn policy(limits: MetadataLimits, context: &RequestContext<'_>) -> Result<(), MetadataError> {
    if [
        limits.bytes,
        limits.nodes,
        limits.scalar_bytes,
        limits.collection_entries,
        limits.versions,
        limits.dependencies,
    ]
    .contains(&0)
    {
        return Err(MetadataError::InvalidPolicy);
    }
    check(context)
}
fn scalar(value: &str, limits: MetadataLimits) -> Result<(), MetadataError> {
    if value.len() > limits.scalar_bytes {
        Err(MetadataError::Limit)
    } else {
        Ok(())
    }
}
fn relative(value: &str, limits: MetadataLimits) -> Result<(), MetadataError> {
    scalar(value, limits)?;
    if value.is_empty()
        || value.contains([':', '\\'])
        || value.chars().any(char::is_control)
        || value.split('/').any(|s| matches!(s, "" | "." | ".."))
    {
        return Err(MetadataError::InvalidDocument);
    }
    Ok(())
}
fn contains(parent: &str, path: &str) -> bool {
    path == parent
        || path
            .strip_prefix(parent)
            .is_some_and(|tail| tail.starts_with('/'))
}
fn validate(
    input: &NativeRootInput<'_>,
    limits: MetadataLimits,
    context: &RequestContext<'_>,
) -> Result<(ArtifactId, ConcreteVersion), MetadataError> {
    check(context)?;
    let max = match input.artifact_type {
        ArtifactType::Ast => 3,
        ArtifactType::Knowledge => 1,
        _ => return Err(MetadataError::InvalidDocument),
    };
    if input.components.is_empty() || input.components.len() > max {
        return Err(MetadataError::InvalidDocument);
    }
    // Count the closed shape: containers, keys and scalar values.
    let nodes = 15usize
        .checked_add(15 * input.components.len())
        .ok_or(MetadataError::Limit)?;
    if limits.collection_entries < 7 || nodes > limits.nodes {
        return Err(MetadataError::Limit);
    }
    scalar("graphit-native-root", limits)?;
    scalar(&input.producer.to_string(), limits)?;
    scalar(input.id, limits)?;
    scalar(input.version, limits)?;
    if input.version.eq_ignore_ascii_case("latest") {
        return Err(MetadataError::InvalidDocument);
    }
    let id =
        ArtifactId::new(input.id, limits.projection).map_err(|_| MetadataError::InvalidDocument)?;
    let version = ConcreteVersion::new(input.version, limits.projection)
        .map_err(|_| MetadataError::InvalidDocument)?;
    let required = if input.artifact_type == ArtifactType::Ast {
        NativeRole::Graph
    } else {
        NativeRole::Wiki
    };
    let mut present = false;
    for (index, component) in input.components.iter().enumerate() {
        check(context)?;
        scalar(component.role.as_str(), limits)?;
        relative(&component.path, limits)?;
        if input.components[..index].iter().any(|previous| {
            previous.role == component.role
                || contains(&previous.path, &component.path)
                || contains(&component.path, &previous.path)
        }) {
            return Err(MetadataError::InvalidDocument);
        }
        if (input.artifact_type == ArtifactType::Knowledge
            && (component.role != NativeRole::Wiki || component.path != "index.lance"))
            || (input.artifact_type == ArtifactType::Ast
                && (component.role == NativeRole::Wiki
                    || (component.role == NativeRole::EntitySearch
                        && component.path != "corpus.lance")))
        {
            return Err(MetadataError::InvalidDocument);
        }
        present |= component.role == required;
        relative(component.snapshot.manifest_path(), limits)?;
        // Every key and fixed scalar in this finite wire is shorter than its digest.
        if limits.scalar_bytes < 64 {
            return Err(MetadataError::Limit);
        }
        match &component.snapshot {
            NativeSnapshot::Lance {
                dataset_version, ..
            } => {
                if component.role == NativeRole::Graph || *dataset_version == 0 {
                    return Err(MetadataError::InvalidDocument);
                }
            }
            NativeSnapshot::Icebug {
                manifest_path,
                generation,
                ..
            } => {
                scalar(generation, limits)?;
                if component.role != NativeRole::Graph
                    || manifest_path != "icebug.json"
                    || generation.is_empty()
                {
                    return Err(MetadataError::InvalidDocument);
                }
            }
        }
    }
    if !present {
        return Err(MetadataError::InvalidDocument);
    }
    check(context)?;
    Ok((id, version))
}

struct OutputWriter<'a, 'r> {
    bytes: Vec<u8>,
    limit: usize,
    context: &'a RequestContext<'r>,
    failure: Option<MetadataError>,
}
impl Write for OutputWriter<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = check(self.context).and_then(|()| {
            if self
                .bytes
                .len()
                .checked_add(bytes.len())
                .is_none_or(|n| n > self.limit)
            {
                return Err(MetadataError::Limit);
            }
            self.bytes
                .try_reserve(bytes.len())
                .map_err(|_| MetadataError::Limit)?;
            Ok(())
        });
        if let Err(error) = result {
            self.failure = Some(error);
            return Err(io::Error::other("native root output rejected"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
mod digest {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};
    pub(super) fn serialize<S: Serializer>(
        value: &[u8; 32],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        const HEX: &[u8] = b"0123456789abcdef";
        let output: String = value
            .iter()
            .flat_map(|byte| {
                [
                    char::from(HEX[(byte >> 4) as usize]),
                    char::from(HEX[(byte & 15) as usize]),
                ]
            })
            .collect();
        serializer.serialize_str(&output)
    }
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[u8; 32], D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.len() != 64 {
            return Err(D::Error::custom("invalid native manifest digest"));
        }
        let mut output = [0; 32];
        for (byte, pair) in output.iter_mut().zip(value.as_bytes().as_chunks::<2>().0) {
            let digit = |v| match v {
                b'0'..=b'9' => Some(v - b'0'),
                b'a'..=b'f' => Some(v - b'a' + 10),
                _ => None,
            };
            *byte = digit(pair[0])
                .zip(digit(pair[1]))
                .map(|(a, b)| a * 16 + b)
                .ok_or_else(|| D::Error::custom("invalid native manifest digest"))?;
        }
        Ok(output)
    }
}
#[cfg(test)]
mod tests;
