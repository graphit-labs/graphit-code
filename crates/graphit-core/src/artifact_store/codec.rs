use super::*;
use serde::de::{DeserializeSeed, MapAccess, Visitor};
use std::collections::HashSet;
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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

/// Provenance of content rendered from the installed Graphit package.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GeneratedOrigin {
    pub package: String,
    pub version: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Candidate {
    pub key: ArtifactKey,
    pub slot: PublicationSlot,
    pub source: NativePath,
    pub generated_origin: Option<GeneratedOrigin>,
    pub destination: NativePath,
    pub kind: Kind,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", deny_unknown_fields)]
pub(super) enum Proof {
    Link {
        receipt: receipt::Receipt,
    },
    Copy {
        policy: CopyPolicy,
        tree: tree::TreeReceipt,
    },
}
impl Proof {
    pub(super) fn validate(&self, candidate: &Candidate, store: &ArtifactStore) -> io::Result<()> {
        match self {
            Self::Link { receipt } => receipt.validate(candidate),
            Self::Copy { tree, .. } => tree.validate(store.limits.copy, candidate.kind),
        }
    }
    pub(super) fn mode(&self) -> ArtifactMode {
        match self {
            Self::Link { .. } => ArtifactMode::Link,
            Self::Copy { policy, .. } => ArtifactMode::Copy(*policy),
        }
    }
    pub(super) fn register_ids(&self, ids: &mut HashSet<tree::PhysicalId>) -> io::Result<()> {
        let mut insert = |id| {
            if ids.insert(id) {
                Ok(())
            } else {
                Err(io::Error::from(io::ErrorKind::InvalidData))
            }
        };
        match self {
            Self::Link { receipt } => {
                let id = match receipt.stamp {
                    receipt::Stamp::Unix { device, inode, .. } => {
                        tree::PhysicalId::Unix { device, inode }
                    }
                    receipt::Stamp::Windows {
                        volume,
                        index,
                        created,
                    } => tree::PhysicalId::Windows {
                        volume,
                        index,
                        created,
                    },
                };
                insert(id)
            }
            Self::Copy { tree, .. } => {
                for node in &tree.nodes {
                    insert(node.id.clone())?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactEntry {
    pub(super) candidate: Candidate,
    pub(super) receipt: Proof,
}
impl ArtifactEntry {
    pub fn key(&self) -> &ArtifactKey {
        &self.candidate.key
    }
    pub fn publication_slot(&self) -> PublicationSlot {
        self.candidate.slot
    }
    pub(super) fn matches_publication(&self, slot: PublicationSlot, key: &ArtifactKey) -> bool {
        self.publication_slot() == slot && self.key() == key
    }
    pub fn generated_origin(&self) -> Option<&GeneratedOrigin> {
        self.candidate.generated_origin.as_ref()
    }
    /// Logical installed-content hash, independent of the source location.
    pub fn content_fingerprint(&self) -> Option<[u8; 32]> {
        match &self.receipt {
            Proof::Copy { tree, .. } => {
                Some(tree::fingerprint(tree.nodes.iter().map(|node| {
                    (node.relative.0.as_path(), node.mode, &node.content)
                })))
            }
            Proof::Link { .. } => None,
        }
    }
    pub fn source(&self) -> &Path {
        &self.candidate.source.0
    }
    pub fn destination(&self) -> &Path {
        &self.candidate.destination.0
    }
    pub fn mode(&self) -> ArtifactMode {
        match self.receipt {
            Proof::Link { .. } => ArtifactMode::Link,
            Proof::Copy { policy, .. } => ArtifactMode::Copy(policy),
        }
    }
    pub fn kind(&self) -> NodeKind {
        self.candidate.kind.into()
    }
}
impl fmt::Debug for ArtifactEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArtifactEntry")
            .field("kind", &self.kind())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "operation", deny_unknown_fields)]
pub(super) enum Pending {
    NativeDocument(Box<super::native_document::Operation>),
    Publication(Box<publication::Operation>),
    Inspect { entry: Box<ArtifactEntry> },
}
impl Pending {
    pub(super) fn publication_slot(&self) -> Option<PublicationSlot> {
        match self {
            Self::Publication(op) => Some(op.candidate.slot),
            Self::Inspect { entry, .. } => Some(entry.publication_slot()),
            Self::NativeDocument(_) => None,
        }
    }
    pub fn key(&self) -> Option<&ArtifactKey> {
        match self {
            Self::NativeDocument(op) => Some(&op.key),
            Self::Publication(op) => Some(&op.candidate.key),
            Self::Inspect { entry, .. } => Some(entry.key()),
        }
    }
    pub fn phase(&self) -> ArtifactPending {
        match self {
            Self::NativeDocument(op) => ArtifactPending::NativeDocument(op.state.phase()),
            Self::Publication(op) => op.phase.into(),
            Self::Inspect { .. } => ArtifactPending::Inspect,
        }
    }
}

#[derive(Clone, Serialize)]
pub(super) struct Document {
    pub scope: ScopeBindingRecord,
    pub entries: Vec<ArtifactEntry>,
    pub pending: Option<Pending>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub abandoned_publications: Vec<publication::AbandonedPublication>,
}

#[derive(Serialize)]
#[serde(tag = "operation")]
enum NativePending<'a, T: ?Sized> {
    NativeDocument(&'a T),
}

/// A serialization view over one already validated, physically captured base.
/// The caller chooses only the native pending value and its current scope data.
#[derive(Serialize)]
pub(super) struct DocumentProjection<'a, T: ?Sized> {
    scope: &'a ScopeBindingRecord,
    entries: &'a [ArtifactEntry],
    pending: Option<NativePending<'a, T>>,
    #[serde(skip_serializing_if = "<[publication::AbandonedPublication]>::is_empty")]
    abandoned_publications: &'a [publication::AbandonedPublication],
}
impl<'a, T: ?Sized> DocumentProjection<'a, T> {
    pub(super) fn new(
        base: &'a Document,
        scope: &'a ScopeBindingRecord,
        pending: Option<&'a T>,
    ) -> Self {
        Self {
            scope,
            entries: &base.entries,
            pending: pending.map(NativePending::NativeDocument),
            abandoned_publications: &base.abandoned_publications,
        }
    }
}

impl Document {
    pub fn empty(store: &ArtifactStore) -> io::Result<Self> {
        Ok(Self {
            scope: store.scope.binding()?,
            entries: Vec::new(),
            pending: None,
            abandoned_publications: Vec::new(),
        })
    }
    pub fn validate(&self, store: &ArtifactStore) -> io::Result<()> {
        if self.scope.owner() != store.owner() || !self.scope.matches_catalog(&store.scope) {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        let mut keys = HashSet::new();
        let mut destinations = HashSet::new();
        let mut stamps = HashSet::new();
        for entry in &self.entries {
            validate_candidate_data(&entry.candidate, store)?;
            entry.receipt.validate(&entry.candidate, store)?;
            if !keys.insert((entry.publication_slot(), entry.key()))
                || !destinations.insert(entry.destination())
            {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }
            entry.receipt.register_ids(&mut stamps)?;
        }
        let mut abandoned_paths = HashSet::new();
        for abandoned in &self.abandoned_publications {
            abandoned.validate(store)?;
            for stage in abandoned.operation.stages() {
                if !abandoned_paths.insert(&stage.path.0)
                    || self.entries.iter().any(|entry| {
                        stage.path.0.starts_with(entry.destination())
                            || entry.destination().starts_with(&stage.path.0)
                    })
                    || matches!(&self.pending, Some(Pending::Publication(active))
                        if active.stages().any(|other| other.path == stage.path))
                {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                if let Some(receipt) = &stage.receipt
                    && !stamps.insert(receipt.id.clone())
                {
                    return Err(io::ErrorKind::InvalidData.into());
                }
            }
        }
        match &self.pending {
            Some(Pending::NativeDocument(operation)) => {
                operation.validate(self, store)?;
                operation.validate_destinations(&self.scope)?;
            }
            Some(Pending::Publication(operation)) => {
                operation.validate(self, store)?;
                operation.validate_destinations(&self.scope)?;
            }
            Some(Pending::Inspect { entry, .. })
                if !matches!(entry.receipt, Proof::Copy { .. })
                    || !self.entries.iter().any(|e| e == entry.as_ref()) =>
            {
                return Err(io::ErrorKind::InvalidData.into());
            }
            Some(Pending::Inspect { entry, .. }) => self.scope.destination(entry.destination())?,
            None => {}
        }
        Ok(())
    }
}

pub(super) fn validate_candidate(candidate: &Candidate, store: &ArtifactStore) -> io::Result<()> {
    validate_candidate_data(candidate, store)?;
    store.require_publication_slot(candidate)?;
    store.destination_scope(&candidate.destination.0)
}

pub(super) fn validate_candidate_data(
    candidate: &Candidate,
    store: &ArtifactStore,
) -> io::Result<()> {
    candidate.key.validate(store.limits.max_key_bytes)?;
    if let Some(origin) = &candidate.generated_origin
        && (origin.package.is_empty()
            || origin.version.is_empty()
            || origin.package.len() > store.limits.max_key_bytes
            || origin.version.len() > store.limits.max_key_bytes
            || origin
                .package
                .chars()
                .chain(origin.version.chars())
                .any(char::is_control))
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    candidate.slot.validate(store.owner())?;
    valid_path(&candidate.source.0)?;
    valid_path(&candidate.destination.0)?;
    if candidate.source == candidate.destination
        || candidate.destination.0.starts_with(store.runtime_root())
        || store.runtime_root().starts_with(&candidate.destination.0)
        || (candidate.kind == Kind::Directory
            && candidate
                .destination
                .0
                .parent()
                .is_some_and(|p| p.starts_with(&candidate.source.0)))
    {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    Ok(())
}

struct DocumentSeed<'a>(&'a ArtifactStore);
impl<'de> DeserializeSeed<'de> for DocumentSeed<'_> {
    type Value = Document;
    fn deserialize<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<Document, D::Error> {
        decoder.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for DocumentSeed<'_> {
    type Value = Document;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a closed artifact document")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Document, M::Error> {
        let (mut scope, mut entries, mut pending) = (None, None, None);
        let mut abandoned_publications = None;
        let mut seen = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(serde::de::Error::custom("duplicate artifact field"));
            }
            match key.as_str() {
                "scope" => {
                    scope = Some(
                        map.next_value_seed(super::scopes::BindingSeed(self.0.scope.limits()))?,
                    )
                }
                "entries" => {
                    entries = Some(map.next_value_seed(super::decode::Sequence {
                        seed: super::decode::EntrySeed(super::decode::Limits::from_store(self.0)),
                    })?)
                }
                "abandoned_publications" => {
                    abandoned_publications =
                        Some(map.next_value_seed(super::decode::Sequence {
                            seed: AbandonedPublicationSeed(self.0),
                        })?);
                }
                "pending" => {
                    pending = Some(map.next_value_seed(super::decode::PendingSeed::optional(
                        super::decode::Limits::from_store(self.0),
                    ))?)
                }
                _ => return Err(serde::de::Error::custom("unknown artifact field")),
            }
        }
        Ok(Document {
            scope: scope.ok_or_else(|| serde::de::Error::missing_field("scope"))?,
            entries: entries.ok_or_else(|| serde::de::Error::missing_field("entries"))?,
            pending: pending.ok_or_else(|| serde::de::Error::missing_field("pending"))?,
            abandoned_publications: abandoned_publications.unwrap_or_default(),
        })
    }
}

#[derive(Clone, Copy)]
struct AbandonedPublicationSeed<'a>(&'a ArtifactStore);
impl<'de> DeserializeSeed<'de> for AbandonedPublicationSeed<'_> {
    type Value = publication::AbandonedPublication;
    fn deserialize<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<Self::Value, D::Error> {
        decoder.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for AbandonedPublicationSeed<'_> {
    type Value = publication::AbandonedPublication;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a closed abandoned publication intent")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let (mut scope, mut operation) = (None, None);
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "scope" if scope.is_none() => {
                    scope = Some(
                        map.next_value_seed(super::scopes::BindingSeed(self.0.scope.limits()))?,
                    )
                }
                "operation" if operation.is_none() => {
                    operation = Some(map.next_value_seed(super::decode::PublicationRecordSeed(
                        super::decode::Limits::from_store(self.0),
                    ))?)
                }
                _ => {
                    return Err(serde::de::Error::custom(
                        "duplicate or unknown abandoned publication field",
                    ));
                }
            }
        }
        Ok(publication::AbandonedPublication {
            scope: scope.ok_or_else(|| serde::de::Error::missing_field("scope"))?,
            operation: operation.ok_or_else(|| serde::de::Error::missing_field("operation"))?,
        })
    }
}

pub(super) fn decode(bytes: &[u8], store: &ArtifactStore) -> io::Result<Document> {
    if bytes.len() > store.limits.max_document_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let document = DocumentSeed(store)
        .deserialize(&mut decoder)
        .map_err(|_| io::ErrorKind::InvalidData)?;
    decoder.end().map_err(|_| io::ErrorKind::InvalidData)?;
    Ok(document)
}

struct ByteWriter {
    bytes: Vec<u8>,
    limit: usize,
}
impl io::Write for ByteWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::from(io::ErrorKind::FileTooLarge));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(super) fn encode<T: Serialize + ?Sized>(document: &T, limit: usize) -> io::Result<Vec<u8>> {
    let mut writer = ByteWriter {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut writer, document).map_err(|error| {
        io::Error::from(error.io_error_kind().unwrap_or(io::ErrorKind::InvalidData))
    })?;
    Ok(writer.bytes)
}

fn check_writer(writer: &super::ArtifactWriter) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    if let Some(native) = &writer.native {
        native.request.check()?;
    }
    #[cfg(not(target_os = "linux"))]
    let _ = writer;
    Ok(())
}
pub(super) fn encode_for_writer<T: Serialize + ?Sized>(
    document: &T,
    writer: &super::ArtifactWriter,
) -> io::Result<Vec<u8>> {
    check_writer(writer)?;
    let bytes = encode(document, writer.store.limits.max_document_bytes)?;
    check_writer(writer)?;
    Ok(bytes)
}
