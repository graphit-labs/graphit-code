//! Logical owners and configured destinations for artifact publication.
//!
//! Paths are ordinary filesystem locations. Constructing a scope does not open,
//! capture or revalidate directories, and links and mount points are supported.
use super::PublicationSlot;
use crate::{
    brand::Brand,
    installation_identity::InstallationId,
    project_identity::ProjectId,
    project_paths::{InstallationPaths, ProjectPaths},
};
use serde::{
    Deserialize, Serialize,
    de::{self, Visitor},
};
use std::{
    fmt, io,
    path::{Path, PathBuf},
};

mod user;
pub use user::UserIdentity;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum StoreOwner {
    User {
        #[serde(with = "installation_id")]
        installation_id: InstallationId,
        identity: UserIdentity,
    },
    Project {
        project_id: ProjectId,
    },
    Installation {
        #[serde(with = "installation_id")]
        installation_id: InstallationId,
    },
}
mod installation_id {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        id: &InstallationId,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(id.as_str())
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<InstallationId, D::Error> {
        struct IdVisitor;
        impl<'de> Visitor<'de> for IdVisitor {
            type Value = InstallationId;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a validated installation identifier")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                InstallationId::new(value).map_err(E::custom)
            }
        }
        deserializer.deserialize_str(IdVisitor)
    }
}

/// The logical owner and configured locations used by one publication consumer.
#[derive(Clone, PartialEq, Eq)]
pub struct PublicationScope {
    owner: StoreOwner,
    brand: Brand,
    owner_root: PathBuf,
    runtime_root: PathBuf,
    destination_roots: Vec<PathBuf>,
}

impl fmt::Debug for PublicationScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PublicationScope { .. }")
    }
}

impl PublicationScope {
    pub fn project(project: &ProjectPaths, project_id: ProjectId) -> io::Result<Self> {
        Ok(Self {
            owner: StoreOwner::Project { project_id },
            brand: project.brand().clone(),
            owner_root: project.root().to_owned(),
            runtime_root: project.project_runtime(project_id, &[])?,
            destination_roots: vec![project.root().to_owned()],
        })
    }

    pub fn installation(
        installation: &InstallationPaths,
        installation_id: InstallationId,
        destination_roots: impl IntoIterator<Item = PathBuf>,
    ) -> io::Result<Self> {
        Self::installation_scope(
            installation,
            installation_id,
            destination_roots,
            &["runtime"],
        )
    }

    /// Native instructions use a separate publication catalogue from other artifacts.
    pub fn installation_instructions(
        installation: &InstallationPaths,
        installation_id: InstallationId,
        destination_roots: impl IntoIterator<Item = PathBuf>,
    ) -> io::Result<Self> {
        Self::installation_scope(
            installation,
            installation_id,
            destination_roots,
            &["runtime", "native-instructions"],
        )
    }

    pub fn installation_models(
        installation: &InstallationPaths,
        installation_id: InstallationId,
        destination_roots: impl IntoIterator<Item = PathBuf>,
    ) -> io::Result<Self> {
        Self::installation_scope(
            installation,
            installation_id,
            destination_roots,
            &["runtime", "models"],
        )
    }

    fn installation_scope(
        installation: &InstallationPaths,
        installation_id: InstallationId,
        destination_roots: impl IntoIterator<Item = PathBuf>,
        runtime_parts: &[&str],
    ) -> io::Result<Self> {
        let mut destination_roots: Vec<_> = destination_roots.into_iter().collect();
        if destination_roots.is_empty() || destination_roots.iter().any(|root| !root.is_absolute())
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        destination_roots.sort();
        destination_roots.dedup();
        Ok(Self {
            owner: StoreOwner::Installation { installation_id },
            brand: installation.brand().clone(),
            owner_root: installation.root().to_owned(),
            runtime_root: installation.global_path(runtime_parts)?,
            destination_roots,
        })
    }

    pub fn owner(&self) -> &StoreOwner {
        &self.owner
    }
    pub fn brand(&self) -> &Brand {
        &self.brand
    }
    pub fn owner_root(&self) -> &Path {
        &self.owner_root
    }
    pub fn runtime_root(&self) -> &Path {
        &self.runtime_root
    }
    pub fn allowed_roots(&self) -> impl ExactSizeIterator<Item = &Path> {
        self.destination_roots.iter().map(PathBuf::as_path)
    }
    pub fn publication_slot(&self) -> PublicationSlot {
        match self.owner {
            StoreOwner::Project { .. } => PublicationSlot::ProjectRoot(self.owner_root.clone()),
            _ => PublicationSlot::Scope,
        }
    }
    /// Check an external publication destination lexically against its configured
    /// base. This is not a filesystem confinement or directory identity check.
    pub(super) fn destination(&self, path: &Path) -> io::Result<()> {
        crate::native_path::valid_path(path)?;
        if path.starts_with(&self.runtime_root)
            || self.runtime_root.starts_with(path)
            || self
                .destination_roots
                .iter()
                .any(|root| root.starts_with(path))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        for root in &self.destination_roots {
            if path.starts_with(root) {
                return Ok(());
            }
        }
        Err(io::ErrorKind::InvalidInput.into())
    }
}

#[cfg(test)]
mod tests;
