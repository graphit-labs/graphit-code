//! Publication selection data inside a shared installation catalogue.
use super::*;

/// A configured checkout location distinguishes consumers sharing one PROJECT
/// catalogue. It conveys no authority and contains no physical filesystem identity.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "root",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum PublicationSlot {
    Scope,
    ProjectRoot(PathBuf),
}

impl fmt::Debug for PublicationSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Scope => "PublicationSlot::Scope",
            Self::ProjectRoot(_) => "PublicationSlot::ProjectRoot { .. }",
        })
    }
}

impl PublicationSlot {
    pub(super) fn validate(&self, owner: &StoreOwner) -> io::Result<()> {
        match (owner, self) {
            (StoreOwner::Project { .. }, Self::ProjectRoot(root)) if root.is_absolute() => Ok(()),
            (StoreOwner::Installation { .. } | StoreOwner::User { .. }, Self::Scope) => Ok(()),
            _ => Err(io::ErrorKind::InvalidData.into()),
        }
    }
}

impl ArtifactStore {
    pub fn publication_slot(&self) -> PublicationSlot {
        self.scope().publication_slot()
    }
}

impl ArtifactWriter {
    pub fn publication_slot(&self) -> PublicationSlot {
        self.store.publication_slot()
    }
}
