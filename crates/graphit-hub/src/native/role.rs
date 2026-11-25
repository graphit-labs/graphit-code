//! Persistable role identity. References never authenticate their holders.
use crate::{ArtifactId, ProjectionLimits, policy::RequestContext};
use graphit_core::brand::Brand;
use serde::{
    Deserialize, Serialize,
    de::{self, DeserializeSeed, MapAccess, Visitor},
    ser::SerializeStruct,
};
use std::{
    fmt,
    io::{self, Write},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Scout,
    Tracker,
    Scribe,
    Coordinator,
    Dream,
}
impl Role {
    pub const ALL: [Self; 5] = [
        Self::Scout,
        Self::Tracker,
        Self::Scribe,
        Self::Coordinator,
        Self::Dream,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scout => "scout",
            Self::Tracker => "tracker",
            Self::Scribe => "scribe",
            Self::Coordinator => "coordinator",
            Self::Dream => "dream",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoleReferenceError {
    Invalid,
    Limit,
    Request,
}
impl fmt::Display for RoleReferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid role reference",
            Self::Limit => "role reference limit",
            Self::Request => "role reference request unavailable",
        })
    }
}
impl std::error::Error for RoleReferenceError {}

/// Validated provenance data; neither origin authenticates its holder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoleSource {
    HubArtifact(ArtifactId),
    ToolOwned(Brand),
}
impl Serialize for RoleSource {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut object = serializer.serialize_struct("RoleSource", 2)?;
        match self {
            Self::HubArtifact(id) => {
                object.serialize_field("kind", "hub_artifact")?;
                object.serialize_field("artifact_id", id.as_str())?;
            }
            Self::ToolOwned(brand) => {
                object.serialize_field("kind", "tool_owned")?;
                object.serialize_field("brand", brand.key())?;
            }
        }
        object.end()
    }
}

/// Data only: possession, decoding and construction never grant authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleCredentialReference {
    role: Role,
    source: RoleSource,
    policy_sha256: [u8; 32],
}
impl RoleCredentialReference {
    pub fn new(role: Role, source: RoleSource, policy_sha256: [u8; 32]) -> Self {
        Self {
            role,
            source,
            policy_sha256,
        }
    }
    pub fn role(&self) -> Role {
        self.role
    }
    pub fn source(&self) -> &RoleSource {
        &self.source
    }
    pub fn policy_sha256(&self) -> &[u8; 32] {
        &self.policy_sha256
    }
    pub fn decode(
        bytes: &[u8],
        max_bytes: usize,
        limits: ProjectionLimits,
        request: &RequestContext<'_>,
    ) -> Result<Self, RoleReferenceError> {
        request
            .remaining()
            .map_err(|_| RoleReferenceError::Request)?;
        if max_bytes == 0 || bytes.len() > max_bytes {
            return Err(RoleReferenceError::Limit);
        }
        let mut decoder = serde_json::Deserializer::from_slice(bytes);
        let result = RefSeed(limits)
            .deserialize(&mut decoder)
            .map_err(|_| RoleReferenceError::Invalid)?;
        decoder.end().map_err(|_| RoleReferenceError::Invalid)?;
        request
            .remaining()
            .map_err(|_| RoleReferenceError::Request)?;
        Ok(result)
    }
    pub fn encode(
        &self,
        max_bytes: usize,
        request: &RequestContext<'_>,
    ) -> Result<Vec<u8>, RoleReferenceError> {
        request
            .remaining()
            .map_err(|_| RoleReferenceError::Request)?;
        if max_bytes == 0 {
            return Err(RoleReferenceError::Limit);
        }
        let mut count = CountWriter {
            len: 0,
            max: max_bytes,
            request,
        };
        serde_json::to_writer(&mut count, self).map_err(|_| {
            if request.remaining().is_err() {
                RoleReferenceError::Request
            } else {
                RoleReferenceError::Limit
            }
        })?;
        let mut result = Vec::new();
        result
            .try_reserve_exact(count.len)
            .map_err(|_| RoleReferenceError::Limit)?;
        serde_json::to_writer(&mut result, self).map_err(|_| RoleReferenceError::Invalid)?;
        request
            .remaining()
            .map_err(|_| RoleReferenceError::Request)?;
        Ok(result)
    }
}
impl Serialize for RoleCredentialReference {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut hash = [0u8; 64];
        for (i, b) in self.policy_sha256.iter().enumerate() {
            hash[i * 2] = b"0123456789abcdef"[(b >> 4) as usize];
            hash[i * 2 + 1] = b"0123456789abcdef"[(b & 15) as usize];
        }
        let mut object = s.serialize_struct("RoleCredentialReference", 4)?;
        object.serialize_field("format_version", &1u8)?;
        object.serialize_field("role", &self.role)?;
        object.serialize_field("source", &self.source)?;
        object.serialize_field(
            "policy_sha256",
            std::str::from_utf8(&hash).map_err(serde::ser::Error::custom)?,
        )?;
        object.end()
    }
}
struct CountWriter<'a, 'b> {
    len: usize,
    max: usize,
    request: &'a RequestContext<'b>,
}
impl Write for CountWriter<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        // write_all retries Interrupted. Request cancellation is terminal here.
        self.request.remaining().map_err(|_| io::ErrorKind::Other)?;
        self.len = self
            .len
            .checked_add(bytes.len())
            .filter(|n| *n <= self.max)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum Key {
    FormatVersion,
    Role,
    Source,
    PolicySha256,
}
struct RefSeed(ProjectionLimits);
impl<'de> DeserializeSeed<'de> for RefSeed {
    type Value = RoleCredentialReference;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for RefSeed {
    type Value = RoleCredentialReference;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a closed role reference")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut m: M) -> Result<Self::Value, M::Error> {
        let (mut version, mut role, mut id, mut hash) = (None, None, None, None);
        while let Some(key) = m.next_key::<Key>()? {
            match key {
                Key::FormatVersion if version.is_none() => {
                    let v = m.next_value::<u8>()?;
                    if v != 1 {
                        return Err(de::Error::custom("invalid version"));
                    }
                    version = Some(v);
                }
                Key::Role if role.is_none() => role = Some(m.next_value::<Role>()?),
                Key::Source if id.is_none() => id = Some(m.next_value_seed(SourceSeed(self.0))?),
                Key::PolicySha256 if hash.is_none() => hash = Some(m.next_value_seed(HashSeed)?),
                _ => return Err(de::Error::custom("duplicate field")),
            }
        }
        if version.is_none() {
            return Err(de::Error::custom("missing version"));
        }
        Ok(RoleCredentialReference::new(
            role.ok_or_else(|| de::Error::custom("missing role"))?,
            id.ok_or_else(|| de::Error::custom("missing source"))?,
            hash.ok_or_else(|| de::Error::custom("missing policy"))?,
        ))
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum SourceKind {
    HubArtifact,
    ToolOwned,
}
#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum SourceKey {
    Kind,
    ArtifactId,
    Brand,
}
struct SourceSeed(ProjectionLimits);
impl<'de> DeserializeSeed<'de> for SourceSeed {
    type Value = RoleSource;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for SourceSeed {
    type Value = RoleSource;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a closed role source")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let (mut kind, mut artifact, mut brand) = (None, None, None);
        while let Some(key) = map.next_key::<SourceKey>()? {
            match key {
                SourceKey::Kind if kind.is_none() => kind = Some(map.next_value::<SourceKind>()?),
                SourceKey::ArtifactId if artifact.is_none() => {
                    artifact = Some(map.next_value_seed(IdSeed(self.0))?)
                }
                SourceKey::Brand if brand.is_none() => {
                    brand = Some(map.next_value_seed(BrandSeed)?)
                }
                _ => return Err(de::Error::custom("duplicate source field")),
            }
        }
        match (kind, artifact, brand) {
            (Some(SourceKind::HubArtifact), Some(id), None) => Ok(RoleSource::HubArtifact(id)),
            (Some(SourceKind::ToolOwned), None, Some(brand)) => Ok(RoleSource::ToolOwned(brand)),
            _ => Err(de::Error::custom("invalid source fields")),
        }
    }
}
struct BrandSeed;
impl<'de> DeserializeSeed<'de> for BrandSeed {
    type Value = Brand;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_str(self)
    }
}
impl Visitor<'_> for BrandSeed {
    type Value = Brand;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a canonical brand")
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        let brand = Brand::new(value).map_err(|_| de::Error::custom("invalid brand"))?;
        if brand.key() != value {
            return Err(de::Error::custom("noncanonical brand"));
        }
        Ok(brand)
    }
}

struct IdSeed(ProjectionLimits);
impl<'de> DeserializeSeed<'de> for IdSeed {
    type Value = ArtifactId;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<ArtifactId, D::Error> {
        d.deserialize_str(self)
    }
}
impl Visitor<'_> for IdSeed {
    type Value = ArtifactId;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a bounded artifact identifier")
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<ArtifactId, E> {
        ArtifactId::new(v, self.0).map_err(|_| de::Error::custom("invalid artifact"))
    }
}
struct HashSeed;
impl<'de> DeserializeSeed<'de> for HashSeed {
    type Value = [u8; 32];
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_str(self)
    }
}
impl Visitor<'_> for HashSeed {
    type Value = [u8; 32];
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a lowercase SHA-256 fingerprint")
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        if v.len() != 64 {
            return Err(de::Error::custom("invalid fingerprint"));
        }
        let mut result = [0; 32];
        for (i, b) in v.bytes().enumerate() {
            let digit = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                _ => return Err(de::Error::custom("invalid fingerprint")),
            };
            result[i / 2] |= digit << (if i % 2 == 0 { 4 } else { 0 });
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
