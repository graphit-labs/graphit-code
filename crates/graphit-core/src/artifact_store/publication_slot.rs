//! Publication selection DATA inside one shared installation catalogue.
use super::*;
use crate::native_file_identity::PhysicalId;

/// Identifies a publication consumer, never authority to access its destinations.
/// PROJECT installations use the physically captured root identity; other scopes
/// retain their scope-wide publication slot. Permissions are not slot identity.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "root",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum PublicationSlot {
    Scope,
    ProjectRoot(OwnerRootIdentity),
}

impl PublicationSlot {
    pub(super) fn capture(owner: &StoreOwner, root: &PhysicalId) -> Self {
        if !matches!(owner, StoreOwner::Project { .. }) {
            return Self::Scope;
        }
        Self::ProjectRoot(match *root {
            PhysicalId::Unix { device, inode } => OwnerRootIdentity::Unix { device, inode },
            PhysicalId::Windows {
                volume,
                index,
                created,
            } => OwnerRootIdentity::Windows {
                volume,
                index,
                created,
            },
        })
    }

    pub(super) fn validate(self, owner: &StoreOwner) -> io::Result<()> {
        match (owner, self) {
            (StoreOwner::Project { .. }, Self::ProjectRoot(root)) => {
                let identity = match root {
                    OwnerRootIdentity::Unix { device, inode } => PhysicalId::Unix { device, inode },
                    OwnerRootIdentity::Windows {
                        volume,
                        index,
                        created,
                    } => PhysicalId::Windows {
                        volume,
                        index,
                        created,
                    },
                };
                identity.validate()
            }
            (StoreOwner::Installation { .. } | StoreOwner::User { .. }, Self::Scope) => Ok(()),
            _ => Err(io::ErrorKind::InvalidData.into()),
        }
    }
}

impl fmt::Debug for PublicationSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Scope => "PublicationSlot::Scope",
            Self::ProjectRoot(_) => "PublicationSlot::ProjectRoot { .. }",
        })
    }
}

impl ArtifactStore {
    /// Current publication selection from the scope's real capture. This DATA
    /// does not replace the original request or destination/source guards.
    pub fn publication_slot(&self) -> PublicationSlot {
        self.scope.publication_slot()
    }

    pub(super) fn require_publication_slot(&self, candidate: &Candidate) -> io::Result<()> {
        if candidate.slot == self.publication_slot() {
            Ok(())
        } else {
            Err(io::ErrorKind::InvalidInput.into())
        }
    }
}

impl ArtifactWriter {
    pub fn publication_slot(&self) -> PublicationSlot {
        self.store.publication_slot()
    }
}

impl PublicationIntent {
    pub fn publication_slot(&self) -> PublicationSlot {
        self.candidate.slot
    }
}

impl ArtifactIntent {
    /// Native-document intents remain shared and have no checkout slot.
    pub fn publication_slot(&self) -> Option<PublicationSlot> {
        self.publication().map(PublicationIntent::publication_slot)
    }
}
