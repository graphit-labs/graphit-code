//! Stable project identity and shared atomic project-document persistence.

use crate::brand::Brand;
use crate::config::{ConfigDocument, ConfigSource, FILE_LIMIT, RawSettings, present_value};
use crate::file_lock::{Cancellation, FileLock};
use crate::local_files::{
    atomic_replace, canonical_directory, check_cancelled, directory_exists, ensure_directory,
    invalid_data, invalid_input, read_regular, regular_metadata,
};
use crate::project_paths::ProjectPaths;
use serde::{Deserialize, Deserializer, Serialize, de};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{File, Permissions};
use std::io;
use std::path::Path;
use std::str::FromStr;
use std::time::{Duration, Instant};

const DOCUMENT_LIMIT: usize = 64 * 1024;

/// Canonical, nonzero ULID. Construction never accepts a path or friendly name.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProjectId(ulid::Ulid);

impl FromStr for ProjectId {
    type Err = io::Error;

    fn from_str(text: &str) -> io::Result<Self> {
        let id =
            ulid::Ulid::from_string(text).map_err(|_| invalid_input("invalid project ULID"))?;
        if id.is_nil() || id.to_string() != text {
            return Err(invalid_input("project ULID must be canonical and nonzero"));
        }
        Ok(Self(id))
    }
}

impl TryFrom<String> for ProjectId {
    type Error = io::Error;

    fn try_from(text: String) -> io::Result<Self> {
        text.parse()
    }
}

impl From<ProjectId> for String {
    fn from(id: ProjectId) -> Self {
        id.0.to_string()
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Mutable input data; writers validate and normalize it before persistence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectMetadata {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "unique_cluster"
    )]
    pub cluster: BTreeMap<String, Vec<String>>,
    pub ephemeral: bool,
}

impl ProjectMetadata {
    pub fn new(name: &str) -> io::Result<Self> {
        Ok(Self {
            name: normalized_name(name)?,
            description: String::new(),
            cluster: BTreeMap::new(),
            ephemeral: false,
        })
    }

    /// Automatic local name; explicit names use `new`'s stricter validation.
    pub fn for_directory(directory: impl AsRef<Path>) -> io::Result<Self> {
        let directory = canonical_directory(directory.as_ref())?;
        let basename = directory.file_name().unwrap_or_default().to_string_lossy();
        let mut name = String::new();
        for character in basename.chars() {
            if name.len() == 64 {
                break;
            }
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                name.push(character.to_ascii_lowercase());
            } else if !name.ends_with('-') {
                name.push('-');
            }
        }
        let name = name.trim_matches(['.', '_', '-']);
        Self::new(if name.is_empty() { "project" } else { name })
    }

    fn normalized(&self) -> io::Result<Self> {
        // Bound input before allocating normalized copies or an encoded document.
        let mut size = self.name.len().checked_add(self.description.len());
        for (key, values) in &self.cluster {
            size = size.and_then(|size| size.checked_add(key.len()));
            for value in values {
                size = size.and_then(|size| size.checked_add(value.len().max(1)));
                if size.is_none_or(|size| size > DOCUMENT_LIMIT) {
                    return Err(invalid_input("metadata exceeds its byte budget"));
                }
            }
        }
        if size.is_none_or(|size| size > DOCUMENT_LIMIT) {
            return Err(invalid_input("metadata exceeds its byte budget"));
        }
        let mut cluster = BTreeMap::new();
        let mut keys = BTreeSet::new();
        for (key, values) in &self.cluster {
            let key = key.trim();
            if key.is_empty() {
                continue;
            }
            if !keys.insert(key) {
                return Err(invalid_input("cluster keys collide after normalization"));
            }
            let values: BTreeSet<_> = values
                .iter()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .collect();
            if values.is_empty() {
                continue;
            }
            cluster.insert(
                key.to_owned(),
                values.into_iter().map(str::to_owned).collect(),
            );
        }
        Ok(Self {
            name: normalized_name(&self.name)?,
            description: self.description.clone(),
            cluster,
            ephemeral: self.ephemeral,
        })
    }
}

fn normalized_name(name: &str) -> io::Result<String> {
    let name = name.trim();
    let valid_edge = |byte: &u8| byte.is_ascii_alphanumeric();
    if name.is_empty()
        || name.len() > 64
        || !name.as_bytes().first().is_some_and(valid_edge)
        || !name.as_bytes().last().is_some_and(valid_edge)
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(invalid_input(
            "project name must be a 1–64 byte ASCII segment with alphanumeric edges",
        ));
    }
    Ok(name.to_ascii_lowercase())
}

fn unique_cluster<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, Vec<String>>, D::Error> {
    struct Unique;
    impl<'de> de::Visitor<'de> for Unique {
        type Value = BTreeMap<String, Vec<String>>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a cluster map with unique keys")
        }
        fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut cluster = BTreeMap::new();
            while let Some((key, values)) = map.next_entry::<String, Vec<String>>()? {
                if cluster.insert(key, values).is_some() {
                    return Err(de::Error::custom("duplicate cluster key"));
                }
            }
            Ok(cluster)
        }
    }
    deserializer.deserialize_map(Unique)
}

/// Validated identity snapshots cannot mutate their ID or stored metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "IdentityDocument")]
pub struct ProjectIdentity {
    id: ProjectId,
    metadata: ProjectMetadata,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityDocument {
    id: ProjectId,
    metadata: ProjectMetadata,
}

impl TryFrom<IdentityDocument> for ProjectIdentity {
    type Error = io::Error;
    fn try_from(document: IdentityDocument) -> io::Result<Self> {
        let metadata = document.metadata.normalized()?;
        if metadata != document.metadata {
            return Err(invalid_data("stored metadata must already be canonical"));
        }
        Ok(Self {
            id: document.id,
            metadata,
        })
    }
}

impl ProjectIdentity {
    pub fn id(&self) -> ProjectId {
        self.id
    }
    pub fn metadata(&self) -> &ProjectMetadata {
        &self.metadata
    }
    pub fn allows_persistent_stores(&self) -> bool {
        !self.metadata.ephemeral
    }
}

/// Explicit project root; construction and reads create no filesystem state.
#[derive(Clone, Debug)]
pub struct ProjectIdentityStore {
    project: ProjectPaths,
}

impl ProjectIdentityStore {
    pub fn at(project: &ProjectPaths) -> Self {
        Self {
            project: project.clone(),
        }
    }

    pub fn root(&self) -> &Path {
        self.project.root()
    }

    pub fn load(&self) -> io::Result<Option<ProjectIdentity>> {
        Ok(read_identity(self.project.root(), self.project.brand())?.map(|(identity, _)| identity))
    }

    /// `timeout` bounds lock contention; filesystem calls remain synchronous.
    pub fn writer(
        &self,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> io::Result<ProjectIdentityWriter> {
        check_cancelled(cancellation)?;
        let brand = self.project.brand_directory();
        ensure_directory(&brand)?;
        let (lock, parent) = project_document_lock(&brand, timeout, cancellation)?;
        Ok(ProjectIdentityWriter {
            store: self.clone(),
            cancellation: cancellation.clone(),
            _lock: lock,
            _parent: parent,
        })
    }
}

// Both producers retain the same private lock entry and its parent handle.
// Parent preparation remains the caller's responsibility.
pub(crate) fn project_document_lock(
    parent_path: &Path,
    timeout: Duration,
    cancellation: &Cancellation,
) -> io::Result<(FileLock, File)> {
    let parent = graphit_fs::read_directory::open_root_readonly(parent_path, &mut || {
        check_cancelled(cancellation)
    })?;
    let lock_path = parent_path.join(".project.json.lock");
    regular_metadata(&lock_path)?;
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| invalid_input("timeout is too large"))?;
    let file = graphit_fs::write_relative::open_private_lock_relative(
        &parent,
        std::ffi::OsStr::new(".project.json.lock"),
        DOCUMENT_LIMIT,
        &mut || check_cancelled(cancellation),
    )?;
    let lock = FileLock::acquire_owned_file(file, lock_path, Some(deadline), cancellation)?;
    Ok((lock, parent))
}

/// A single owned writer; drop releases its stable lock without unlinking it.
#[derive(Debug)]
pub struct ProjectIdentityWriter {
    store: ProjectIdentityStore,
    cancellation: Cancellation,
    _lock: FileLock,
    _parent: File,
}

impl ProjectIdentityWriter {
    /// Create with a generated ID only when absent; preserve existing identity and metadata.
    pub fn ensure(&mut self, metadata: &ProjectMetadata) -> io::Result<ProjectIdentity> {
        self.ensure_identity(metadata, None)
    }

    /// Initialize a known project; reject a conflicting ID without replacing metadata.
    pub fn ensure_with_id(
        &mut self,
        metadata: &ProjectMetadata,
        requested_id: ProjectId,
    ) -> io::Result<ProjectIdentity> {
        self.ensure_identity(metadata, Some(requested_id))
    }

    fn ensure_identity(
        &mut self,
        metadata: &ProjectMetadata,
        requested_id: Option<ProjectId>,
    ) -> io::Result<ProjectIdentity> {
        check_cancelled(&self.cancellation)?;
        let metadata = metadata.normalized()?;
        if let Some((identity, _)) =
            read_identity(self.store.project.root(), self.store.project.brand())?
        {
            if requested_id.is_some_and(|id| id != identity.id) {
                return Err(invalid_input(
                    "project identity conflicts with requested ID",
                ));
            }
            return Ok(identity);
        }
        let identity = ProjectIdentity {
            id: requested_id.unwrap_or_else(|| ProjectId(ulid::Ulid::generate())),
            metadata,
        };
        self.save(&identity, None)?;
        Ok(identity)
    }

    /// Update metadata only if the persisted identity is exactly the expected ID.
    pub fn update(
        &mut self,
        expected_id: ProjectId,
        metadata: &ProjectMetadata,
    ) -> io::Result<ProjectIdentity> {
        check_cancelled(&self.cancellation)?;
        let metadata = metadata.normalized()?;
        let (existing, permissions) =
            read_identity(self.store.project.root(), self.store.project.brand())?.ok_or_else(
                || io::Error::new(io::ErrorKind::NotFound, "project identity is absent"),
            )?;
        if existing.id != expected_id {
            return Err(invalid_input("project identity conflicts with expected ID"));
        }
        if metadata == existing.metadata {
            return Ok(existing);
        }
        let identity = ProjectIdentity {
            id: existing.id,
            metadata,
        };
        self.save(&identity, Some(permissions))?;
        Ok(identity)
    }

    fn save(&self, identity: &ProjectIdentity, permissions: Option<Permissions>) -> io::Result<()> {
        crate::config::count_serialized(identity, DOCUMENT_LIMIT)
            .map_err(|_| invalid_input("identity metadata exceeds its JSON byte budget"))?;
        // Both metadata and configuration writers hold .project.json.lock. Read
        // the other sections under that lock instead of replacing their state.
        let config =
            match read_project_document(self.store.project.root(), self.store.project.brand())? {
                Some((_, config, _)) => config,
                None => ConfigDocument::new(self.store.project.brand(), ConfigSource::Project, [])?,
            };
        let bytes = config.encode_with_identity(Some(identity))?;
        let path = self.store.project.identity_file();
        regular_metadata(&path)?;
        atomic_replace(&path, &bytes, permissions, &self.cancellation)
    }
}

pub(crate) fn read_identity(
    root: &Path,
    brand: &Brand,
) -> io::Result<Option<(ProjectIdentity, Permissions)>> {
    Ok(read_project_document(root, brand)?
        .map(|(identity, _, permissions)| (identity, permissions)))
}

pub(crate) fn read_project_document(
    root: &Path,
    brand: &Brand,
) -> io::Result<Option<(ProjectIdentity, ConfigDocument, Permissions)>> {
    if !directory_exists(&root.join(brand.directory()))? {
        return Ok(None);
    }
    let Some((bytes, permissions)) = read_regular(
        &root.join(brand.directory()).join("project.json"),
        FILE_LIMIT,
    )?
    else {
        return Ok(None);
    };
    let (identity, config) = decode_project_document(brand, &bytes)?;
    Ok(Some((identity, config, permissions)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectDocument {
    id: ProjectId,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default, deserialize_with = "unique_cluster")]
    cluster: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    ephemeral: bool,
    #[serde(default)]
    settings: RawSettings,
    #[serde(default, deserialize_with = "present_value")]
    githooks: Option<serde_json::Value>,
}

pub(crate) fn decode_project_document(
    brand: &Brand,
    bytes: &[u8],
) -> io::Result<(ProjectIdentity, ConfigDocument)> {
    if bytes.len() > FILE_LIMIT {
        return Err(invalid_data("project document exceeds its byte budget"));
    }
    let document: ProjectDocument =
        serde_json::from_slice(bytes).map_err(|_| invalid_data("invalid project document JSON"))?;
    let metadata = ProjectMetadata {
        name: document.name,
        description: document.description,
        cluster: document.cluster,
        ephemeral: document.ephemeral,
    };
    if metadata
        .normalized()
        .map_err(|_| invalid_data("invalid project metadata"))?
        != metadata
    {
        return Err(invalid_data("stored metadata must already be canonical"));
    }
    let config = ConfigDocument::from_sections(
        brand,
        ConfigSource::Project,
        document.settings,
        document.githooks,
    )?;
    let identity = ProjectIdentity {
        id: document.id,
        metadata,
    };
    crate::config::count_serialized(&identity, DOCUMENT_LIMIT)
        .map_err(|_| invalid_data("identity metadata exceeds its JSON byte budget"))?;
    Ok((identity, config))
}
