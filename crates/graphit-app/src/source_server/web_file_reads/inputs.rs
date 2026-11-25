use crate::remote_file_install::client::{Kind, RemoteContentArguments};
use graphit_core::project_identity::ProjectId;
use graphit_hub::{ArtifactId, ConcreteVersion, NativeBinding, policy::HubResourcePolicy};
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Component, Path},
};

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Scope {
    GlobalCache,
    Project {
        project_id: ProjectId,
        project_version: String,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Installed {
    pub scope: Scope,
    #[serde(rename = "artifact_type")]
    pub kind: Kind,
    pub id: String,
    pub version: String,
    pub path: String,
    #[serde(default, deserialize_with = "non_null")]
    pub binding: Option<NativeBinding>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct List {
    pub scope: Scope,
    #[serde(default, rename = "artifact_type", deserialize_with = "non_null")]
    pub kind: Option<Kind>,
    #[serde(default, deserialize_with = "non_null")]
    pub id: Option<String>,
    #[serde(default, deserialize_with = "non_null")]
    pub version: Option<String>,
    #[serde(default, deserialize_with = "non_null")]
    pub binding: Option<NativeBinding>,
    #[serde(default)]
    pub offset: usize,
    pub limit: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Content {
    #[serde(default, deserialize_with = "non_null")]
    remote: Option<RemoteContentArguments>,
    #[serde(default, deserialize_with = "non_null")]
    installed: Option<Installed>,
}
fn non_null<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
pub(crate) enum Query {
    Remote(RemoteContentArguments),
    Installed(Installed),
    List(List),
}
fn invalid(_: impl std::fmt::Display) -> io::Error {
    io::ErrorKind::InvalidInput.into()
}
pub(super) fn path(value: &str, maximum: usize) -> io::Result<()> {
    if value.is_empty()
        || value.len() > maximum
        || value.contains('\\')
        || !Path::new(value)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}
impl Query {
    pub(crate) fn decode(bytes: &[u8], list: bool) -> io::Result<Self> {
        if list {
            return serde_json::from_slice(bytes)
                .map(Self::List)
                .map_err(invalid);
        }
        let content: Content = serde_json::from_slice(bytes).map_err(invalid)?;
        match (content.remote, content.installed) {
            (Some(input), None) => Ok(Self::Remote(input)),
            (None, Some(input)) => Ok(Self::Installed(input)),
            _ => Err(io::ErrorKind::InvalidInput.into()),
        }
    }
    pub(super) fn scope(&self) -> Option<&Scope> {
        match self {
            Self::Remote(_) => None,
            Self::Installed(input) => Some(&input.scope),
            Self::List(input) => Some(&input.scope),
        }
    }
    pub(super) fn validate(&self, policy: &HubResourcePolicy) -> io::Result<()> {
        let (id, version, binding) = match self {
            Self::Remote(input) => {
                input.selection.validate(policy).map_err(invalid)?;
                if input.selection.identity_provider.is_some() {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                return path(&input.path, policy.content().max_scalar_bytes);
            }
            Self::Installed(input) => {
                path(&input.path, policy.content().max_scalar_bytes)?;
                if matches!(input.scope, Scope::Project { .. }) && input.binding.is_none() {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                (
                    Some(input.id.as_str()),
                    Some(input.version.as_str()),
                    input.binding,
                )
            }
            Self::List(input) => {
                if input.limit == 0
                    || input.limit > policy.content().max_collection_entries
                    || input.offset.checked_add(input.limit).is_none()
                {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                (input.id.as_deref(), input.version.as_deref(), input.binding)
            }
        };
        if let Some(id) = id {
            ArtifactId::new(id, policy.projection()).map_err(invalid)?;
        }
        if let Some(version) = version {
            ConcreteVersion::new(version, policy.projection()).map_err(invalid)?;
        }
        match self.scope() {
            Some(Scope::GlobalCache) if binding.is_some() => {
                Err(io::ErrorKind::InvalidInput.into())
            }
            Some(Scope::Project {
                project_version, ..
            }) if project_version.is_empty() || project_version.contains('\0') => {
                Err(io::ErrorKind::InvalidInput.into())
            }
            _ => Ok(()),
        }
    }
}
