//! Compact descriptive data retained with one installed source tuple.
use super::{HubLimits, invalid};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ArtifactDisplayMetadata {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    tags: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawDisplayMetadata {
    name: String,
    description: Option<String>,
    tags: Vec<String>,
}
impl RawDisplayMetadata {
    pub(super) fn restore(self, limits: HubLimits) -> io::Result<ArtifactDisplayMetadata> {
        let display = ArtifactDisplayMetadata {
            name: self.name,
            description: self.description,
            tags: self.tags,
        };
        display.validate(limits)?;
        Ok(display)
    }
}
impl ArtifactDisplayMetadata {
    pub fn new(
        name: &str,
        description: Option<&str>,
        tags: &[String],
        limits: HubLimits,
    ) -> io::Result<Self> {
        validate(name, description, tags, limits)?;
        Ok(Self {
            name: name.into(),
            description: description.map(str::to_owned),
            tags: tags.to_vec(),
        })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
    pub(crate) fn validate(&self, limits: HubLimits) -> io::Result<()> {
        validate(&self.name, self.description.as_deref(), &self.tags, limits)
    }
}
fn validate(
    name: &str,
    description: Option<&str>,
    tags: &[String],
    limits: HubLimits,
) -> io::Result<()> {
    if name.is_empty() || tags.len() > limits.entries {
        return Err(invalid());
    }
    let mut bytes = 0usize;
    for value in std::iter::once(name)
        .chain(description)
        .chain(tags.iter().map(String::as_str))
    {
        if value.len() > limits.json_string_bytes {
            return Err(invalid());
        }
        bytes = bytes.checked_add(value.len()).ok_or_else(invalid)?;
        if bytes > limits.document_bytes {
            return Err(invalid());
        }
    }
    Ok(())
}
