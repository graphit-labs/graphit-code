use super::*;
use std::collections::HashSet;
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Kind {
    File,
    Directory,
}
impl From<NodeKind> for Kind {
    fn from(kind: NodeKind) -> Self {
        match kind {
            NodeKind::File => Self::File,
            NodeKind::Directory => Self::Directory,
        }
    }
}
impl From<Kind> for NodeKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::File => Self::File,
            Kind::Directory => Self::Directory,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GeneratedOrigin {
    pub package: String,
    pub version: String,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactEntry {
    pub(super) key: ArtifactKey,
    pub(super) slot: PublicationSlot,
    pub(super) source: PathBuf,
    pub(super) generated_origin: Option<GeneratedOrigin>,
    pub(super) destination: PathBuf,
    pub(super) kind: Kind,
    pub(super) mode: ArtifactMode,
    pub(super) content_hash: Option<[u8; 32]>,
}
impl ArtifactEntry {
    pub fn key(&self) -> &ArtifactKey {
        &self.key
    }
    pub fn publication_slot(&self) -> PublicationSlot {
        self.slot.clone()
    }
    pub fn generated_origin(&self) -> Option<&GeneratedOrigin> {
        self.generated_origin.as_ref()
    }
    pub fn content_fingerprint(&self) -> Option<[u8; 32]> {
        self.content_hash
    }
    pub fn source(&self) -> &Path {
        &self.source
    }
    pub fn destination(&self) -> &Path {
        &self.destination
    }
    pub fn mode(&self) -> ArtifactMode {
        self.mode
    }
    pub fn kind(&self) -> NodeKind {
        self.kind.into()
    }
}
impl fmt::Debug for ArtifactEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArtifactEntry")
            .field("key", &self.key)
            .field("kind", &self.kind())
            .finish_non_exhaustive()
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Document {
    pub owner: StoreOwner,
    pub entries: Vec<ArtifactEntry>,
}
impl Document {
    pub fn validate(&self, store: &ArtifactStore) -> io::Result<()> {
        if &self.owner != store.owner() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mut keys = HashSet::new();
        let mut destinations = HashSet::new();
        for entry in &self.entries {
            entry.key.validate(store.limits.max_key_bytes)?;
            entry.slot.validate(store.owner())?;
            crate::native_path::absolute_input(entry.source())?;
            valid_path(entry.destination())?;
            if !keys.insert((&entry.slot, &entry.key))
                || !destinations.insert(entry.destination())
                || entry.source() == entry.destination()
            {
                return Err(io::ErrorKind::InvalidData.into());
            }
            if matches!(entry.mode, ArtifactMode::Copy(_)) != entry.content_hash.is_some() {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        Ok(())
    }
}
