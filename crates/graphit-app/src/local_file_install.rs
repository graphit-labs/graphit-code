//! Explicit canonical local sources for the authenticated PROJECT Hub installer.
use crate::remote_file_install::client::PhysicalMode;
use graphit_core::project_identity::ProjectId;
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, NativeBinding,
    policy::{HubResourcePolicy, RequestContext},
};
use rmcp::model::CallToolRequestParams;
use serde::{Deserialize, Serialize};
use std::{io, path::PathBuf};

pub(crate) const TOOL: &str = "graphit_hub_install_local";

#[derive(Clone, Copy, clap::ValueEnum, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Agent,
    Skill,
    Command,
    Workflow,
    Rule,
    Mcp,
    Language,
    Power,
}
impl Kind {
    pub(crate) fn artifact_type(self) -> ArtifactType {
        match self {
            Self::Agent => ArtifactType::Agent,
            Self::Skill => ArtifactType::Skill,
            Self::Command => ArtifactType::Command,
            Self::Workflow => ArtifactType::Workflow,
            Self::Rule => ArtifactType::Rule,
            Self::Mcp => ArtifactType::Mcp,
            Self::Language => ArtifactType::Language,
            Self::Power => ArtifactType::Power,
        }
    }
}
fn present_mode<'de, D: serde::Deserializer<'de>>(
    value: D,
) -> Result<Option<PhysicalMode>, D::Error> {
    PhysicalMode::deserialize(value).map(Some)
}
#[derive(clap::Args, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Arguments {
    /// Absolute artifact directory; its canonical root is captured before publication.
    #[arg(long)]
    pub source_dir: PathBuf,
    /// Provenance only; this does not grant access to the consumer PROJECT.
    #[arg(long)]
    pub producer: String,
    #[arg(long, value_enum)]
    #[serde(rename = "artifact_type")]
    pub kind: Kind,
    #[arg(long)]
    pub id: String,
    /// Literal artifact version, independent of the selected PROJECT version.
    #[arg(long)]
    pub version: String,
    #[arg(long)]
    pub project_dir: PathBuf,
    #[arg(skip)]
    pub project_version: Option<String>,
    #[arg(long)]
    pub binding: Option<NativeBinding>,
    #[arg(long)]
    pub alias: Option<String>,
    #[arg(long)]
    pub entry_name: Option<String>,
    /// PROJECT installation mode. The checkout receives a copy.
    #[arg(long, value_enum)]
    #[serde(
        default,
        deserialize_with = "present_mode",
        skip_serializing_if = "Option::is_none"
    )]
    pub mode: Option<PhysicalMode>,
}
impl Arguments {
    pub(crate) fn validate(&self, policy: &HubResourcePolicy) -> io::Result<()> {
        if !self.source_dir.is_absolute()
            || !self.project_dir.is_absolute()
            || self.version == "latest"
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.producer
            .parse::<ProjectId>()
            .map_err(io::Error::other)?;
        ArtifactId::new(&self.id, policy.projection()).map_err(io::Error::other)?;
        ConcreteVersion::new(&self.version, policy.projection()).map_err(io::Error::other)?;
        if self.project_version.as_deref().is_some_and(|value| {
            value.is_empty() || value.trim() != value || value.chars().any(char::is_control)
        }) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        for name in self
            .alias
            .as_deref()
            .into_iter()
            .chain(self.entry_name.as_deref())
        {
            if name.contains('/') {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            ArtifactId::new(name, policy.projection()).map_err(io::Error::other)?;
        }
        Ok(())
    }
}
pub fn prepare(
    input: &Arguments,
    project_version: Option<&str>,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    original.check()?;
    input.validate(policy)?;
    let mut value = serde_json::to_value(input).map_err(io::Error::other)?;
    if let Some(version) = project_version {
        value["project_version"] = version.into();
    }
    let bytes = serde_json::to_vec(&value).map_err(io::Error::other)?;
    if bytes.len() > original.content_limits().max_input_bytes {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(CallToolRequestParams::new(TOOL).with_arguments(
        value
            .as_object()
            .ok_or(io::ErrorKind::InvalidInput)?
            .clone(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::{brand::Brand, config::ConfigSnapshot, file_lock::Cancellation};
    use serde_json::json;
    use std::time::Instant;
    #[test]
    fn local_input_has_no_remote_or_implicit_scope_and_preserves_literal_version() {
        let brand = Brand::default();
        let config = ConfigSnapshot::new(
            &brand,
            [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
        )
        .unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let cancel = Cancellation::default();
        let original = policy.request_context(Instant::now(), &cancel).unwrap();
        let root = std::env::temp_dir();
        let value = json!({"source_dir":root.join("local-source"),"producer":"01J00000000000000000000001","artifact_type":"skill","id":"fixture/local","version":"release/one","project_dir":root.join("consumer")});
        let input: Arguments = serde_json::from_value(value.clone()).unwrap();
        let wire = prepare(&input, Some("main"), &policy, &original).unwrap();
        assert_eq!(wire.name, TOOL);
        assert_eq!(wire.arguments.unwrap()["version"], "release/one");
        for (key, invalid) in [
            ("provider", json!("remote")),
            ("global", json!(true)),
            ("artifact_type", json!("ast")),
            ("mode", serde_json::Value::Null),
        ] {
            let mut bad = value.clone();
            bad[key] = invalid;
            assert!(serde_json::from_value::<Arguments>(bad).is_err());
        }
        let mut bad: Arguments = serde_json::from_value(value).unwrap();
        bad.source_dir = "relative".into();
        assert!(prepare(&bad, None, &policy, &original).is_err());
    }
}
