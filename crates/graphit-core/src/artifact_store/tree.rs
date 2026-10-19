use super::{codec::Kind, *};
use serde::{Deserializer, Serializer};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    io::{Read, Write},
};

pub(super) const BUFFER_BYTES: usize = 64 * 1024;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct RelativePath(pub PathBuf);
impl Serialize for RelativePath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        relative_path(&self.0).map_err(serde::ser::Error::custom)?;
        self.0.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for RelativePath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let path = PathBuf::deserialize(deserializer)?;
        relative_path(&path).map_err(serde::de::Error::custom)?;
        Ok(Self(path))
    }
}
fn relative_path(path: &Path) -> io::Result<()> {
    super::snapshot::validate_relative(path)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub(super) use crate::native_file_identity::{PhysicalId, handle_id, physical_id};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum Mode {
    Unix(u32),
    Windows(bool),
}
impl Mode {
    pub fn observe(metadata: &fs::Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            Self::Unix(metadata.permissions().mode() & 0o777)
        }
        #[cfg(not(unix))]
        {
            Self::Windows(metadata.permissions().readonly())
        }
    }
    pub fn validate(self) -> io::Result<()> {
        let valid = match self {
            #[cfg(unix)]
            Self::Unix(mode) => mode <= 0o777,
            #[cfg(windows)]
            Self::Windows(_) => true,
            _ => false,
        };
        if valid {
            Ok(())
        } else {
            Err(io::ErrorKind::InvalidData.into())
        }
    }
    pub fn set(self, path: &Path) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Self::Unix(mode) => {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(mode))
            }
            #[cfg(windows)]
            Self::Windows(readonly) => {
                let mut permissions = fs::symlink_metadata(path)?.permissions();
                permissions.set_readonly(readonly);
                fs::set_permissions(path, permissions)
            }
            _ => Err(io::ErrorKind::Unsupported.into()),
        }
    }
    pub fn granted(self, directory: bool, access: Access) -> Self {
        match self {
            Self::Unix(mode) => Self::Unix(
                mode | if directory {
                    match access {
                        Access::Cleanup => 0o700,
                        Access::Read => 0o500,
                        Access::Move => 0o200,
                    }
                } else if access == Access::Move {
                    0
                } else {
                    0o400
                },
            ),
            Self::Windows(readonly) => {
                Self::Windows(readonly && (directory || access != Access::Cleanup))
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Access {
    Read,
    Cleanup,
    Move,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub(super) enum Content {
    Directory,
    File { bytes: u64, digest: [u8; 32] },
}
impl Content {
    pub fn directory(&self) -> bool {
        matches!(self, Self::Directory)
    }
    pub(super) fn bytes(&self) -> u64 {
        match self {
            Self::Directory => 0,
            Self::File { bytes, .. } => *bytes,
        }
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NodeReceipt {
    pub relative: RelativePath,
    pub id: PhysicalId,
    pub mode: Mode,
    pub content: Content,
    // Absent in partial trees and publications whose preserved Unix modes do
    // not permit reading. A native reader requires the final content fence.
    #[serde(deserialize_with = "Deserialize::deserialize")]
    pub verified: Option<ContentFence>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TreeReceipt {
    pub nodes: Vec<NodeReceipt>,
}
impl TreeReceipt {
    pub fn directory(path: &Path) -> io::Result<NodeReceipt> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        Ok(NodeReceipt {
            relative: RelativePath(PathBuf::new()),
            id: physical_id(path, &metadata)?,
            mode: Mode::observe(&metadata),
            content: Content::Directory,
            verified: None,
        })
    }
    pub fn validate(&self, limits: CopyLimits, kind: Kind) -> io::Result<()> {
        let mut budget = Budget::new(limits);
        let mut ids = HashSet::new();
        let mut directories = HashSet::new();
        let mut previous: Option<&Path> = None;
        for node in &self.nodes {
            relative_path(&node.relative.0)?;
            node.id.validate()?;
            node.mode.validate()?;
            if let Some(fence) = &node.verified {
                fence.validate()?;
            }
            budget.add(&node.relative.0, node.content.bytes())?;
            if !ids.insert(&node.id) || previous.is_some_and(|p| p >= node.relative.0.as_path()) {
                return Err(io::ErrorKind::InvalidData.into());
            }
            if previous.is_none() {
                if !node.relative.0.as_os_str().is_empty()
                    || node.content.directory() != (kind == Kind::Directory)
                {
                    return Err(io::ErrorKind::InvalidData.into());
                }
            } else if node
                .relative
                .0
                .parent()
                .is_none_or(|p| !directories.contains(p))
            {
                return Err(io::ErrorKind::InvalidData.into());
            }
            if node.content.directory() {
                directories.insert(node.relative.0.as_path());
            }
            previous = Some(&node.relative.0);
        }
        if self.nodes.is_empty() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(())
    }
    pub fn equivalent(&self, plan: &SourcePlan) -> bool {
        self.nodes.len() == plan.nodes.len()
            && self
                .nodes
                .iter()
                .zip(&plan.nodes)
                .enumerate()
                .all(|(i, (a, b))| {
                    a.relative == b.relative
                        && a.mode == plan.output_mode(i)
                        && a.content == b.content
                })
    }
    /// Compare installed content with the recorded hash. Physical bindings are
    /// captured anew for the subsequent publication transaction, never inferred
    /// from an old inode or from a retained second copy of the source.
    pub(super) fn recapture_content(
        &self,
        root: &Path,
        limits: CopyLimits,
        cancellation: &Cancellation,
        context: Option<&super::SnapshotContext>,
    ) -> io::Result<Self> {
        let plan = match context {
            Some(context) => SourcePlan::capture_with_context(
                root,
                if self
                    .nodes
                    .first()
                    .is_some_and(|node| node.content.directory())
                {
                    Kind::Directory
                } else {
                    Kind::File
                },
                CopyPolicy::RejectFileLinks,
                limits,
                context,
            ),
            None => SourcePlan::capture(
                root,
                if self
                    .nodes
                    .first()
                    .is_some_and(|node| node.content.directory())
                {
                    Kind::Directory
                } else {
                    Kind::File
                },
                CopyPolicy::RejectFileLinks,
                limits,
                cancellation,
            ),
        }?;
        if !self.equivalent(&plan) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        let receipt = Self {
            nodes: plan
                .nodes
                .iter()
                .map(|node| NodeReceipt {
                    relative: node.relative.clone(),
                    id: node.opened.id.clone(),
                    mode: node.mode,
                    content: node.content.clone(),
                    verified: Some(node.opened.content_fence()),
                })
                .collect(),
        };
        match context {
            Some(context) => plan.recheck_with_context(limits, context)?,
            None => plan.recheck(limits, cancellation)?,
        }
        Ok(receipt)
    }
    pub fn verify(
        &self,
        root: &Path,
        limits: CopyLimits,
        access: Option<Access>,
        missing: bool,
        cancellation: &Cancellation,
    ) -> io::Result<()> {
        self.verify_inner(root, limits, access, missing, cancellation, None)
    }
    pub fn verify_with_context(
        &self,
        root: &Path,
        limits: CopyLimits,
        access: Option<Access>,
        missing: bool,
        context: &super::SnapshotContext,
    ) -> io::Result<()> {
        self.verify_inner(
            root,
            limits,
            access,
            missing,
            context.cancellation(),
            Some(context),
        )
    }
    /// Bind known content to the exact destination metadata observed before
    /// and after verification. No fresh observation is paired with an old hash.
    pub(super) fn record_verified(
        &mut self,
        root: &Path,
        limits: CopyLimits,
        cancellation: &Cancellation,
        context: Option<&super::SnapshotContext>,
    ) -> io::Result<()> {
        check_cancelled(cancellation)?;
        if let Some(context) = context {
            context.check()?;
        }
        for node in &mut self.nodes {
            node.verified = None;
        }
        // Publication preserves modes, including unreadable files. Such a
        // publication is valid but cannot produce a native read capability.
        if self.nodes.iter().any(|node| match node.mode {
            Mode::Unix(bits) => {
                if node.content.directory() {
                    bits & 0o400 == 0 || bits & 0o100 == 0
                } else {
                    bits & 0o400 == 0
                }
            }
            Mode::Windows(_) => false,
        }) {
            return Ok(());
        }
        let mut observations = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            check_cancelled(cancellation)?;
            if let Some(context) = context {
                context.check()?;
            }
            let metadata =
                checked_node(root, node, None, false)?.ok_or(io::ErrorKind::AlreadyExists)?;
            let observation = Observation::capture(&node_path(root, &node.relative.0), &metadata)?;
            observation.matches_receipt(node)?;
            observations.push(observation);
        }
        self.verify_inner(root, limits, None, false, cancellation, context)?;
        for (node, before) in self.nodes.iter_mut().zip(observations) {
            check_cancelled(cancellation)?;
            if let Some(context) = context {
                context.check()?;
            }
            let metadata =
                checked_node(root, node, None, false)?.ok_or(io::ErrorKind::AlreadyExists)?;
            if Observation::capture(&node_path(root, &node.relative.0), &metadata)? != before {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            node.verified = Some(before.content_fence());
        }
        if let Some(context) = context {
            context.check()?;
        }
        check_cancelled(cancellation)
    }
    fn verify_inner(
        &self,
        root: &Path,
        limits: CopyLimits,
        access: Option<Access>,
        missing: bool,
        cancellation: &Cancellation,
        context: Option<&super::SnapshotContext>,
    ) -> io::Result<()> {
        let mut budget = Budget::new(limits);
        let mut buffer = vec![0; BUFFER_BYTES];
        if missing && fs::symlink_metadata(root).is_err_and(|e| e.kind() == io::ErrorKind::NotFound)
        {
            return Ok(());
        }
        for item in walker(root, Some(limits)) {
            check_cancelled(cancellation)?;
            if let Some(context) = context {
                context.check()?;
            }
            let item = item.map_err(io::Error::from)?;
            let relative = item
                .path()
                .strip_prefix(root)
                .map_err(|_| io::ErrorKind::InvalidData)?;
            let index = self
                .nodes
                .binary_search_by(|n| n.relative.0.as_path().cmp(relative))
                .map_err(|_| io::ErrorKind::AlreadyExists)?;
            let node = &self.nodes[index];
            let metadata =
                checked_node(root, node, access, false)?.ok_or(io::ErrorKind::AlreadyExists)?;
            budget.add(
                relative,
                if metadata.is_file() {
                    metadata.len()
                } else {
                    0
                },
            )?;
            if let Content::File { bytes, digest } = &node.content {
                let mut file = fs::File::open(item.path())?;
                if handle_id(&file)? != node.id
                    || file.metadata()?.len() != *bytes
                    || match context {
                        Some(c) => c.read_exact_length(&mut file, *bytes, &mut buffer, None)?,
                        None => hash(&mut file, *bytes, &mut buffer, cancellation)?,
                    } != *digest
                    || handle_id(&file)? != node.id
                    || file.metadata()?.len() != *bytes
                {
                    return Err(io::ErrorKind::AlreadyExists.into());
                }
                checked_node(root, node, access, false)?;
            }
        }
        if !missing && budget.nodes != self.nodes.len() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        Ok(())
    }
    /// Caller MUST durably record the exact receipt/location before calling.
    pub fn grant(
        &self,
        root: &Path,
        access: Access,
        missing: bool,
        mut after_effect: impl FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        self.grant_inner(root, access, missing, None, &mut after_effect)
    }
    pub fn grant_with_context(
        &self,
        root: &Path,
        access: Access,
        missing: bool,
        context: &super::SnapshotContext,
        after_effect: impl FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        self.grant_inner(root, access, missing, Some(context), after_effect)
    }
    fn grant_inner(
        &self,
        root: &Path,
        access: Access,
        missing: bool,
        context: Option<&super::SnapshotContext>,
        mut after_effect: impl FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        for node in &self.nodes {
            if let Some(context) = context {
                context.check()?;
            }
            let Some(metadata) = checked_node(root, node, Some(access), missing)? else {
                continue;
            };
            let grant = node.mode.granted(node.content.directory(), access);
            if Mode::observe(&metadata) != grant {
                grant.set(&node_path(root, &node.relative.0))?;
                after_effect()?;
            }
        }
        Ok(())
    }
    /// Regrant registered ancestors first, then restore children before parents.
    pub fn restore(
        &self,
        root: &Path,
        mut after_effect: impl FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        self.grant(root, Access::Read, false, || Ok(()))?;
        for node in self.nodes.iter().rev() {
            let metadata = checked_node(root, node, Some(Access::Read), false)?
                .ok_or(io::ErrorKind::AlreadyExists)?;
            if Mode::observe(&metadata) != node.mode {
                node.mode.set(&node_path(root, &node.relative.0))?;
                after_effect()?;
            }
        }
        Ok(())
    }
    /// Durable Cleanup authority and a full remaining-tree scan precede deletion.
    pub fn cleanup(
        &self,
        root: &Path,
        limits: CopyLimits,
        mut after_effect: impl FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        self.cleanup_inner(root, limits, None, &mut after_effect)
    }
    pub(super) fn cleanup_with_context(
        &self,
        root: &Path,
        limits: CopyLimits,
        context: &super::SnapshotContext,
        after_effect: impl FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        self.cleanup_inner(root, limits, Some(context), after_effect)
    }
    fn cleanup_inner(
        &self,
        root: &Path,
        limits: CopyLimits,
        context: Option<&super::SnapshotContext>,
        mut after_effect: impl FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        self.grant_inner(root, Access::Cleanup, true, context, &mut after_effect)?;
        self.verify_inner(
            root,
            limits,
            Some(Access::Cleanup),
            true,
            context.map_or(&Cancellation::default(), |c| c.cancellation()),
            context,
        )?;
        for node in self.nodes.iter().rev() {
            if let Some(context) = context {
                context.check()?;
            }
            if checked_node(root, node, Some(Access::Cleanup), true)?.is_none() {
                continue;
            }
            let path = node_path(root, &node.relative.0);
            if node.content.directory() {
                fs::remove_dir(path)?;
            } else {
                fs::remove_file(path)?;
            }
            after_effect()?;
            if let Some(context) = context {
                context.check()?;
            }
        }
        Ok(())
    }
}
pub(super) fn node_path(root: &Path, relative: &Path) -> PathBuf {
    if relative.as_os_str().is_empty() {
        root.to_owned()
    } else {
        root.join(relative)
    }
}

pub(super) fn checked_node(
    root: &Path,
    node: &NodeReceipt,
    access: Option<Access>,
    missing: bool,
) -> io::Result<Option<fs::Metadata>> {
    let path = node_path(root, &node.relative.0);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if missing && e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mode = Mode::observe(&metadata);
    if metadata.file_type().is_symlink()
        || metadata.is_dir() != node.content.directory()
        || (!metadata.is_dir() && !metadata.is_file())
        || physical_id(&path, &metadata)? != node.id
        || (mode != node.mode
            && access.is_none_or(|a| mode != node.mode.granted(node.content.directory(), a)))
    {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    Ok(Some(metadata))
}
fn walker(root: &Path, limits: Option<CopyLimits>) -> walkdir::IntoIter {
    let walk = walkdir::WalkDir::new(root)
        .follow_links(false)
        .follow_root_links(false);
    match limits {
        Some(limits) => walk
            .max_depth(limits.max_depth + 1)
            .max_open(limits.max_depth + 2)
            .into_iter(),
        None => walk.into_iter(),
    }
}
pub(super) struct Budget {
    limits: CopyLimits,
    nodes: usize,
    bytes: u64,
    paths: usize,
}
impl Budget {
    pub(super) fn new(limits: CopyLimits) -> Self {
        Self {
            limits,
            nodes: 0,
            bytes: 0,
            paths: 0,
        }
    }
    pub(super) fn add(&mut self, relative: &Path, bytes: u64) -> io::Result<()> {
        self.nodes = self
            .nodes
            .checked_add(1)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        self.paths = self
            .paths
            .checked_add(snapshot::path_units(relative)?)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if self.nodes > self.limits.max_tree_nodes
            || self.bytes > self.limits.max_total_file_bytes
            || self.paths > self.limits.max_total_relative_path_bytes
            || relative.components().count() > self.limits.max_depth
        {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        Ok(())
    }
}
fn hash(
    file: &mut fs::File,
    expected: u64,
    buffer: &mut [u8],
    cancellation: &Cancellation,
) -> io::Result<[u8; 32]> {
    let mut digest = Sha256::new();
    let mut bytes = 0u64;
    loop {
        check_cancelled(cancellation)?;
        let read = file.read(buffer)?;
        if read == 0 {
            break;
        }
        bytes = bytes
            .checked_add(read as u64)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if bytes > expected {
            return Err(io::ErrorKind::InvalidData.into());
        }
        digest.update(&buffer[..read]);
    }
    if bytes != expected {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(digest.finalize().into())
}

/// Persist only the temporal fence that is absent from the owning NodeReceipt.
/// Physical identity, mode, kind and file length remain in that one receipt.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ContentFence {
    #[serde(with = "observation_time")]
    modified: std::time::SystemTime,
    #[cfg(unix)]
    change: (i64, i64),
}
impl ContentFence {
    fn validate(&self) -> io::Result<()> {
        #[cfg(unix)]
        if !(0..1_000_000_000).contains(&self.change.1) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Observation {
    id: PhysicalId,
    directory: bool,
    bytes: u64,
    mode: Mode,
    #[serde(with = "observation_time")]
    modified: std::time::SystemTime,
    #[cfg(unix)]
    change: (i64, i64),
}

mod observation_time {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Timestamp {
        before_epoch: bool,
        seconds: u64,
        nanos: u32,
    }

    pub fn serialize<S: Serializer>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error> {
        let (before_epoch, duration) = match time.duration_since(UNIX_EPOCH) {
            Ok(value) => (false, value),
            Err(error) => (true, error.duration()),
        };
        Timestamp {
            before_epoch,
            seconds: duration.as_secs(),
            nanos: duration.subsec_nanos(),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(decoder: D) -> Result<SystemTime, D::Error> {
        let value = Timestamp::deserialize(decoder)?;
        if value.nanos >= 1_000_000_000
            || (value.before_epoch && value.seconds == 0 && value.nanos == 0)
        {
            return Err(serde::de::Error::custom("invalid observation timestamp"));
        }
        let duration = Duration::new(value.seconds, value.nanos);
        let time = if value.before_epoch {
            UNIX_EPOCH.checked_sub(duration)
        } else {
            UNIX_EPOCH.checked_add(duration)
        };
        time.ok_or_else(|| serde::de::Error::custom("observation timestamp out of range"))
    }
}
impl Observation {
    fn content_fence(&self) -> ContentFence {
        ContentFence {
            modified: self.modified,
            #[cfg(unix)]
            change: self.change,
        }
    }

    fn matches_receipt(&self, node: &NodeReceipt) -> io::Result<()> {
        if self.id != node.id
            || self.mode != node.mode
            || self.directory != node.content.directory()
            || (!self.directory && self.bytes != node.content.bytes())
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(())
    }
    pub(super) fn bytes(&self) -> u64 {
        self.bytes
    }
    // A selected FILE depends on its directory namespace, not sibling contents.
    // Selected inventory membership is checked separately; FILE and generic-tree
    // observations remain exact.
    pub(super) fn matches_directory_binding(&self, current: &Self) -> bool {
        self.directory && current.directory && self.id == current.id && self.mode == current.mode
    }
    pub(super) fn from_file(file: &fs::File) -> io::Result<Self> {
        let metadata = file.metadata()?;
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            id: handle_id(file)?,
            directory: metadata.is_dir(),
            bytes: metadata.len(),
            mode: Mode::observe(&metadata),
            modified: metadata.modified()?,
            #[cfg(unix)]
            change: (metadata.ctime(), metadata.ctime_nsec()),
        })
    }
    fn capture(path: &Path, metadata: &fs::Metadata) -> io::Result<Self> {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            id: physical_id(path, metadata)?,
            directory: metadata.is_dir(),
            bytes: metadata.len(),
            mode: Mode::observe(metadata),
            modified: metadata.modified()?,
            #[cfg(unix)]
            change: (metadata.ctime(), metadata.ctime_nsec()),
        })
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceNode {
    pub relative: RelativePath,
    pub mode: Mode,
    pub content: Content,
    original: Observation,
    opened: Observation,
    #[serde(with = "native_link")]
    link: Option<PathBuf>,
}

mod native_link {
    use super::*;

    pub fn serialize<S: Serializer>(
        path: &Option<PathBuf>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        if let Some(path) = path {
            crate::native_path::valid_native(path).map_err(serde::ser::Error::custom)?;
        }
        path.serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<PathBuf>, D::Error> {
        let path = Option::<PathBuf>::deserialize(deserializer)?;
        if let Some(path) = &path {
            crate::native_path::valid_native(path).map_err(serde::de::Error::custom)?;
        }
        Ok(path)
    }
}

impl SourceNode {
    pub(super) fn matches_verified_receipt(&self, receipt: &NodeReceipt) -> bool {
        self.relative == receipt.relative
            && self.opened.matches_receipt(receipt).is_ok()
            && receipt.verified.as_ref() == Some(&self.opened.content_fence())
            && self.mode == receipt.mode
            && self.content.directory() == receipt.content.directory()
            && self.content.bytes() == receipt.content.bytes()
    }
    pub(super) fn from_native_file(
        relative: &Path,
        content: Content,
        file: &fs::File,
    ) -> io::Result<Self> {
        let opened = Observation::from_file(file)?;
        if content.directory() != opened.directory
            || (!content.directory() && content.bytes() != opened.bytes)
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(Self {
            relative: RelativePath(relative.to_owned()),
            mode: opened.mode,
            content,
            original: opened.clone(),
            opened,
            link: None,
        })
    }
    pub(super) fn matches_native_file(&self, file: &fs::File) -> io::Result<bool> {
        Ok(self.opened == Observation::from_file(file)?)
    }
    pub(super) fn matches_native_directory(&self, file: &fs::File) -> io::Result<bool> {
        Ok(self
            .opened
            .matches_directory_binding(&Observation::from_file(file)?))
    }
    pub fn validate_binding(&self, policy: CopyPolicy) -> io::Result<()> {
        relative_path(&self.relative.0)?;
        self.mode.validate()?;
        self.original.id.validate()?;
        self.opened.id.validate()?;
        self.original.mode.validate()?;
        self.opened.mode.validate()?;
        if self.mode != self.opened.mode
            || self.content.directory() != self.opened.directory
            || (!self.content.directory() && self.content.bytes() != self.opened.bytes)
            || (self.link.is_none() && self.original != self.opened)
            || (self.link.is_some()
                && (policy == CopyPolicy::RejectFileLinks || self.content.directory()))
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(())
    }
}
#[derive(Clone)]
pub(super) struct SourcePlan {
    nodes: Vec<SourceNode>,
    source: PathBuf,
    policy: CopyPolicy,
}
fn source_metadata(
    path: &Path,
    policy: CopyPolicy,
) -> io::Result<(fs::Metadata, fs::Metadata, Option<PathBuf>)> {
    let original = fs::symlink_metadata(path)?;
    if original.file_type().is_symlink() {
        if policy == CopyPolicy::RejectFileLinks {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let metadata = fs::metadata(path)?;
        if !metadata.is_file() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok((original, metadata, Some(fs::read_link(path)?)))
    } else if original.is_file() || original.is_dir() {
        Ok((original.clone(), original, None))
    } else {
        Err(io::ErrorKind::InvalidInput.into())
    }
}
impl SourcePlan {
    pub(super) fn set_native_digest(&mut self, index: usize, digest: [u8; 32]) -> io::Result<()> {
        let node = self
            .nodes
            .get_mut(index)
            .ok_or(io::ErrorKind::InvalidData)?;
        if node.content.directory() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        node.content = Content::File {
            bytes: node.content.bytes(),
            digest,
        };
        Ok(())
    }
    pub(super) fn from_native_nodes(source: PathBuf, nodes: Vec<SourceNode>) -> Self {
        Self {
            source,
            nodes,
            policy: CopyPolicy::RejectFileLinks,
        }
    }
    pub fn capture(
        source: &Path,
        kind: Kind,
        policy: CopyPolicy,
        limits: CopyLimits,
        cancellation: &Cancellation,
    ) -> io::Result<Self> {
        Self::capture_inner(source, kind, policy, Some(limits), cancellation, None, None)
            .map(|(plan, _)| plan)
    }

    pub fn capture_with_context(
        source: &Path,
        kind: Kind,
        links: CopyPolicy,
        limits: CopyLimits,
        context: &snapshot::SnapshotContext,
    ) -> io::Result<Self> {
        let retention = snapshot::RetainedBytesLimits::new(1, 1, 1)?;
        let policy = snapshot::SnapshotPolicy::new(
            links,
            limits,
            snapshot::RetainedFileSet::new([], retention)?,
            retention,
        )?;
        Self::capture_snapshot(source, kind, &policy, context).map(|(plan, _)| plan)
    }

    pub fn capture_snapshot(
        source: &Path,
        kind: Kind,
        policy: &snapshot::SnapshotPolicy,
        context: &snapshot::SnapshotContext,
    ) -> io::Result<(Self, snapshot::RetainedSources)> {
        Self::capture_inner(
            source,
            kind,
            policy.links,
            Some(policy.copy),
            context.cancellation(),
            Some(policy),
            Some(context),
        )
    }

    pub(super) fn capture_generated_metadata(
        source: &Path,
        files: &[PathBuf],
        context: &snapshot::SnapshotContext,
    ) -> io::Result<Self> {
        context.check()?;
        let mut paths = std::collections::BTreeSet::from([PathBuf::new()]);
        for relative in files {
            snapshot::validate_relative(relative)?;
            if relative.as_os_str().is_empty() {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let mut path = Some(relative.as_path());
            while let Some(relative) = path {
                paths.insert(relative.to_owned());
                path = relative.parent();
            }
        }
        let mut nodes = Vec::new();
        for relative in paths {
            context.check()?;
            let path = node_path(source, &relative);
            let (before, metadata, link) = source_metadata(&path, CopyPolicy::RejectFileLinks)?;
            if link.is_some() {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let file = fs::File::open(&path)?;
            if physical_id(&path, &before)? != handle_id(&file)? {
                return Err(io::ErrorKind::InvalidData.into());
            }
            let content = if metadata.is_dir() {
                Content::Directory
            } else {
                // No payload digest is claimed by this metadata-only capture.
                Content::File {
                    bytes: metadata.len(),
                    digest: [0; 32],
                }
            };
            nodes.push(SourceNode::from_native_file(&relative, content, &file)?);
        }
        let plan = Self::from_native_nodes(source.to_owned(), nodes);
        plan.recheck_metadata_bindings(context)?;
        Ok(plan)
    }
    pub(super) fn recheck_metadata_bindings(
        &self,
        context: &snapshot::SnapshotContext,
    ) -> io::Result<()> {
        for index in 0..self.nodes.len() {
            self.check_metadata_reader_node(index, None, context)?;
        }
        context.check()
    }
    pub(super) fn check_metadata_reader_node(
        &self,
        index: usize,
        file: Option<&fs::File>,
        context: &snapshot::SnapshotContext,
    ) -> io::Result<()> {
        let node = self.indexed_node(index)?;
        let mut path = Some(node.relative.0.as_path());
        while let Some(relative) = path {
            context.check()?;
            let (_, node) = self.node(relative).ok_or(io::ErrorKind::InvalidData)?;
            let full = node_path(&self.source, relative);
            let (before, metadata, link) = source_metadata(&full, CopyPolicy::RejectFileLinks)?;
            if link.is_some() {
                return Err(io::ErrorKind::InvalidData.into());
            }
            let observed = Observation::capture(&full, &metadata)?;
            if if node.content.directory() {
                !node.opened.matches_directory_binding(&observed)
            } else {
                node.original != Observation::capture(&full, &before)? || node.opened != observed
            } {
                return Err(io::ErrorKind::InvalidData.into());
            }
            path = relative.parent();
        }
        if let Some(file) = file {
            self.check_reader_handle(index, file)?;
        }
        context.check()
    }
    pub(super) fn capture_generated(
        source: &Path,
        kind: Kind,
        context: &snapshot::SnapshotContext,
    ) -> io::Result<Self> {
        Self::capture_inner(
            source,
            kind,
            CopyPolicy::RejectFileLinks,
            None,
            context.cancellation(),
            None,
            Some(context),
        )
        .map(|(plan, _)| plan)
    }

    fn capture_inner(
        source: &Path,
        kind: Kind,
        policy: CopyPolicy,
        limits: Option<CopyLimits>,
        cancellation: &Cancellation,
        snapshot: Option<&snapshot::SnapshotPolicy>,
        context: Option<&snapshot::SnapshotContext>,
    ) -> io::Result<(Self, snapshot::RetainedSources)> {
        let mut plan = Self {
            nodes: Vec::new(),
            source: source.to_owned(),
            policy,
        };
        let mut budget = limits.map(Budget::new);
        let mut buffer = vec![0; BUFFER_BYTES];
        let mut retained = std::collections::BTreeMap::new();
        let mut retained_bytes = 0usize;
        for item in walker(source, limits) {
            check_cancelled(cancellation)?;
            if let Some(context) = context {
                context.check()?;
            }
            let item = item.map_err(io::Error::from)?;
            let relative = item
                .path()
                .strip_prefix(source)
                .map_err(|_| io::ErrorKind::InvalidData)?;
            let (original, metadata, link) = source_metadata(item.path(), policy)?;
            if context.is_some() {
                snapshot::validate_relative(relative)?;
            }
            if relative.as_os_str().is_empty() && metadata.is_dir() != (kind == Kind::Directory) {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            if let Some(budget) = &mut budget {
                budget.add(
                    relative,
                    if metadata.is_file() {
                        metadata.len()
                    } else {
                        0
                    },
                )?;
            }
            let opened_path = if link.is_some() {
                fs::canonicalize(item.path())?
            } else {
                item.path().to_owned()
            };
            let before = Observation::capture(item.path(), &original)?;
            let opened = Observation::capture(&opened_path, &metadata)?;
            let selected = snapshot.is_some_and(|policy| policy.selected.paths.contains(relative));
            if selected && !metadata.is_file() {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let mut retained_file = if selected {
                let size =
                    usize::try_from(metadata.len()).map_err(|_| io::ErrorKind::FileTooLarge)?;
                retained_bytes = retained_bytes
                    .checked_add(size)
                    .ok_or(io::ErrorKind::FileTooLarge)?;
                if snapshot.is_none_or(|policy| retained_bytes > policy.retention.bytes) {
                    return Err(io::ErrorKind::FileTooLarge.into());
                }
                // Allocate only after SnapshotContext admits this file's I/O.
                Some(Vec::new())
            } else {
                None
            };
            let content = if metadata.is_dir() {
                Content::Directory
            } else {
                let mut file = fs::File::open(item.path())?;
                if handle_id(&file)? != opened.id
                    || Observation::capture(&opened_path, &file.metadata()?)? != opened
                {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                let digest = if let Some(context) = context {
                    context.read_exact_length(
                        &mut file,
                        metadata.len(),
                        &mut buffer,
                        retained_file.as_mut(),
                    )?
                } else {
                    hash(&mut file, metadata.len(), &mut buffer, cancellation)?
                };
                if Observation::capture(&opened_path, &file.metadata()?)? != opened {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                Content::File {
                    bytes: metadata.len(),
                    digest,
                }
            };
            let node = SourceNode {
                relative: RelativePath(relative.to_owned()),
                mode: Mode::observe(&metadata),
                content,
                original: before,
                opened,
                link,
            };
            plan.check_node(&node)?;
            plan.nodes.push(node);
            if let Some(bytes) = retained_file {
                retained.insert(relative.to_owned(), bytes);
            }
        }
        plan.nodes.sort_by(|a, b| a.relative.cmp(&b.relative));
        if plan.nodes.is_empty() {
            return Err(io::ErrorKind::NotFound.into());
        }
        if snapshot.is_some_and(|policy| policy.selected.paths.len() != retained.len()) {
            return Err(io::ErrorKind::NotFound.into());
        }
        if let Some(context) = context {
            context.check()?;
        }
        Ok((plan, retained))
    }

    fn output_mode(&self, index: usize) -> Mode {
        self.nodes[index].mode
    }
    pub fn source(&self) -> &Path {
        &self.source
    }

    pub fn nodes(&self) -> impl ExactSizeIterator<Item = &SourceNode> {
        self.nodes.iter()
    }

    pub fn same_source(&self, other: &Self) -> bool {
        self.source == other.source && self.policy == other.policy && self.nodes == other.nodes
    }

    // PROJECT selection compares directory namespaces, not ignored sibling churn.
    // FILE observations, content and the selected inventory are still exact.
    pub(super) fn same_selected_source(&self, other: &Self) -> bool {
        self.source == other.source
            && self.policy == other.policy
            && self.nodes.len() == other.nodes.len()
            && self.nodes.iter().zip(&other.nodes).all(|(left, right)| {
                if !left.content.directory() {
                    return left == right;
                }
                left.relative == right.relative
                    && left.mode == right.mode
                    && left.content == right.content
                    && left.link == right.link
                    && left.original.matches_directory_binding(&right.original)
                    && left.opened.matches_directory_binding(&right.opened)
            })
    }

    pub fn fingerprint(&self) -> [u8; 32] {
        fingerprint(
            self.nodes
                .iter()
                .enumerate()
                .map(|(i, n)| (n.relative.0.as_path(), self.output_mode(i), &n.content)),
        )
    }
    pub(super) fn node(&self, relative: &Path) -> Option<(usize, &SourceNode)> {
        self.nodes
            .binary_search_by(|n| n.relative.0.as_path().cmp(relative))
            .ok()
            .map(|i| (i, &self.nodes[i]))
    }
    pub(super) fn indexed_node(&self, i: usize) -> io::Result<&SourceNode> {
        self.nodes
            .get(i)
            .ok_or_else(|| io::ErrorKind::InvalidData.into())
    }
    pub(super) fn open_verified(&self, node: &SourceNode) -> io::Result<fs::File> {
        self.check_node(node)?;
        let input = fs::File::open(node_path(&self.source, &node.relative.0))?;
        if handle_id(&input)? != node.opened.id {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(input)
    }
    // Read-only source consumers qualify the complete captured observation on
    // the same retained descriptor; Windows must not reopen its pathname here.
    pub(super) fn check_reader_node(
        &self,
        index: usize,
        file: Option<&fs::File>,
        context: &snapshot::SnapshotContext,
    ) -> io::Result<()> {
        context.check()?;
        let node = self.indexed_node(index)?;
        let (_, root) = self.node(Path::new("")).ok_or(io::ErrorKind::InvalidData)?;
        self.check_node(root)?;
        let mut parent = node.relative.0.parent();
        while let Some(path) = parent {
            if path.as_os_str().is_empty() {
                break;
            }
            context.check()?;
            let (_, ancestor) = self.node(path).ok_or(io::ErrorKind::InvalidData)?;
            if !ancestor.content.directory() {
                return Err(io::ErrorKind::InvalidData.into());
            }
            self.check_node(ancestor)?;
            parent = path.parent();
        }
        context.check()?;
        self.check_node(node)?;
        if let Some(file) = file {
            self.check_reader_handle(index, file)?;
        }
        context.check()
    }
    pub(super) fn check_reader_handle(&self, index: usize, file: &fs::File) -> io::Result<()> {
        self.reader_last_modified(index, file).map(|_| ())
    }
    pub(super) fn reader_last_modified(
        &self,
        index: usize,
        file: &fs::File,
    ) -> io::Result<std::time::SystemTime> {
        let observed = Observation::from_file(file)?;
        if observed != self.indexed_node(index)?.opened {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(observed.modified)
    }
    pub(super) fn verify_handle(&self, node: &SourceNode, file: &fs::File) -> io::Result<()> {
        if handle_id(file)? != node.opened.id {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.check_node(node)
    }
    pub(super) fn check_node(&self, node: &SourceNode) -> io::Result<()> {
        let path = node_path(&self.source, &node.relative.0);
        let (original, metadata, link) = source_metadata(&path, self.policy)?;
        let target = if link.is_some() {
            fs::canonicalize(&path)?
        } else {
            path.clone()
        };
        if node.original != Observation::capture(&path, &original)?
            || node.opened != Observation::capture(&target, &metadata)?
            || node.link != link
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(())
    }
    pub fn recheck(&self, limits: CopyLimits, cancellation: &Cancellation) -> io::Result<()> {
        self.recheck_inner(Some(limits), cancellation, None, true)
    }
    pub(super) fn recheck_with_context(
        &self,
        limits: CopyLimits,
        context: &snapshot::SnapshotContext,
    ) -> io::Result<()> {
        self.recheck_inner(Some(limits), context.cancellation(), Some(context), true)
    }
    pub(super) fn recheck_bindings_with_context(
        &self,
        limits: CopyLimits,
        context: &snapshot::SnapshotContext,
    ) -> io::Result<()> {
        self.recheck_inner(Some(limits), context.cancellation(), Some(context), false)
    }
    pub(super) fn recheck_generated_bindings(
        &self,
        context: &snapshot::SnapshotContext,
    ) -> io::Result<()> {
        self.recheck_inner(None, context.cancellation(), Some(context), false)
    }
    fn recheck_inner(
        &self,
        limits: Option<CopyLimits>,
        cancellation: &Cancellation,
        context: Option<&snapshot::SnapshotContext>,
        payload: bool,
    ) -> io::Result<()> {
        let mut budget = limits.map(Budget::new);
        let mut observed_nodes = 0usize;
        for item in walker(&self.source, limits) {
            check_cancelled(cancellation)?;
            if let Some(context) = context {
                context.check()?;
            }
            let item = item.map_err(io::Error::from)?;
            let relative = item
                .path()
                .strip_prefix(&self.source)
                .map_err(|_| io::ErrorKind::InvalidData)?;
            let i = self
                .nodes
                .binary_search_by(|n| n.relative.0.as_path().cmp(relative))
                .map_err(|_| io::ErrorKind::InvalidData)?;
            self.check_node(&self.nodes[i])?;
            observed_nodes = observed_nodes
                .checked_add(1)
                .ok_or(io::ErrorKind::FileTooLarge)?;
            // Binding revalidation observes inventory and identities only.
            // Copy preflight still bounds the payload that operation will read.
            if let Some(budget) = &mut budget {
                budget.add(
                    relative,
                    if payload {
                        self.nodes[i].content.bytes()
                    } else {
                        0
                    },
                )?;
            }
        }
        if observed_nodes != self.nodes.len() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if let Some(context) = context {
            context.check()?;
        }
        Ok(())
    }
    /// Incremental receipts are for runtime error cleanup, never crash adoption.
    pub fn build(
        &self,
        root: &Path,
        cancellation: &Cancellation,
        context: Option<&snapshot::SnapshotContext>,
        created: &mut TreeReceipt,
        mut after_effect: impl FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        self.build_tree(root, cancellation, context, created, &mut after_effect)
    }
}

pub(super) fn fingerprint<'a>(
    nodes: impl ExactSizeIterator<Item = (&'a Path, Mode, &'a Content)>,
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"graphit/payload-tree/v1\0");
    digest.update((nodes.len() as u64).to_le_bytes());
    for (relative, mode, content) in nodes {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let units = relative.as_os_str().as_bytes();
            digest.update((units.len() as u64).to_le_bytes());
            digest.update(units);
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            digest.update(((relative.as_os_str().encode_wide().count() as u64) * 2).to_le_bytes());
            for unit in relative.as_os_str().encode_wide() {
                digest.update(unit.to_le_bytes());
            }
        }
        match mode {
            Mode::Unix(mode) => {
                digest.update(b"u");
                digest.update(mode.to_le_bytes());
            }
            Mode::Windows(readonly) => {
                digest.update(b"w");
                digest.update([u8::from(readonly)]);
            }
        }
        match content {
            Content::Directory => digest.update(b"d"),
            Content::File {
                bytes,
                digest: content,
            } => {
                digest.update(b"f");
                digest.update(bytes.to_le_bytes());
                digest.update(content);
            }
        }
    }
    digest.finalize().into()
}

/// One creator and receipt updater for ordinary copies and derived payloads.
pub(super) struct TreeBuilder<'a> {
    root: &'a Path,
    cancellation: &'a Cancellation,
    context: Option<&'a snapshot::SnapshotContext>,
    created: &'a mut TreeReceipt,
    after_effect: &'a mut dyn FnMut() -> io::Result<()>,
}
impl<'a> TreeBuilder<'a> {
    pub(super) fn new(
        root: &'a Path,
        cancellation: &'a Cancellation,
        context: Option<&'a snapshot::SnapshotContext>,
        created: &'a mut TreeReceipt,
        after_effect: &'a mut dyn FnMut() -> io::Result<()>,
    ) -> Self {
        Self {
            root,
            cancellation,
            context,
            created,
            after_effect,
        }
    }
    pub(super) fn check(&self) -> io::Result<()> {
        check_cancelled(self.cancellation)?;
        if let Some(context) = self.context {
            context.check()?;
        }
        Ok(())
    }
    pub(super) fn directory(&mut self, relative: &Path) -> io::Result<()> {
        self.check()?;
        let path = node_path(self.root, relative);
        #[cfg(unix)]
        let mut builder = fs::DirBuilder::new();
        #[cfg(not(unix))]
        let builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path)?;
        let metadata = fs::symlink_metadata(&path)?;
        self.created.nodes.push(NodeReceipt {
            relative: RelativePath(relative.to_owned()),
            id: physical_id(&path, &metadata)?,
            mode: Mode::observe(&metadata),
            content: Content::Directory,
            verified: None,
        });
        (self.after_effect)()
    }
    pub(super) fn file(&mut self, relative: &Path) -> io::Result<fs::File> {
        self.check()?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let output = options.open(node_path(self.root, relative))?;
        self.created.nodes.push(NodeReceipt {
            relative: RelativePath(relative.to_owned()),
            id: handle_id(&output)?,
            mode: Mode::observe(&output.metadata()?),
            content: Content::File {
                bytes: 0,
                digest: Sha256::digest([]).into(),
            },
            verified: None,
        });
        (self.after_effect)()?;
        Ok(output)
    }
    pub(super) fn write(
        &mut self,
        output: &mut fs::File,
        digest: &mut Sha256,
        written: &mut u64,
        mut chunk: &[u8],
    ) -> io::Result<()> {
        while !chunk.is_empty() {
            self.check()?;
            let bytes = output.write(chunk)?;
            if bytes == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            digest.update(&chunk[..bytes]);
            *written = written
                .checked_add(bytes as u64)
                .ok_or(io::ErrorKind::FileTooLarge)?;
            self.created
                .nodes
                .last_mut()
                .ok_or(io::ErrorKind::InvalidData)?
                .content = Content::File {
                bytes: *written,
                digest: digest.clone().finalize().into(),
            };
            chunk = &chunk[bytes..];
            (self.after_effect)()?;
        }
        Ok(())
    }
    pub(super) fn finish(
        &mut self,
        modes: impl ExactSizeIterator<Item = Mode> + DoubleEndedIterator,
    ) -> io::Result<()> {
        if modes.len() != self.created.nodes.len() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        for (i, mode) in modes.enumerate().rev() {
            self.check()?;
            let node = &mut self.created.nodes[i];
            mode.set(&node_path(self.root, &node.relative.0))?;
            node.mode = mode;
            (self.after_effect)()?;
        }
        self.check()
    }
}

/// Private materialization protocol shared by raw and validated output trees.
/// The caller must hold the store's writer and ownership proof before effects.
pub(super) trait BuildTree {
    fn build_tree(
        &self,
        root: &Path,
        cancellation: &Cancellation,
        context: Option<&snapshot::SnapshotContext>,
        created: &mut TreeReceipt,
        after_effect: &mut dyn FnMut() -> io::Result<()>,
    ) -> io::Result<()>;
}
impl BuildTree for SourcePlan {
    fn build_tree(
        &self,
        root: &Path,
        cancellation: &Cancellation,
        context: Option<&snapshot::SnapshotContext>,
        created: &mut TreeReceipt,
        after_effect: &mut dyn FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        if let Some(context) = context {
            context.check()?;
        }
        let mut builder = TreeBuilder::new(root, cancellation, context, created, after_effect);
        let mut buffer = vec![0; BUFFER_BYTES];
        for source in &self.nodes {
            builder.check()?;
            self.check_node(source)?;
            if source.content.directory() {
                builder.directory(&source.relative.0)?;
            } else {
                let mut input = self.open_verified(source)?;
                let mut output = builder.file(&source.relative.0)?;
                let mut digest = Sha256::new();
                let mut written = 0u64;
                if let Some(context) = context {
                    context.consume_file(
                        &mut input,
                        source.content.bytes(),
                        &mut buffer,
                        |chunk| builder.write(&mut output, &mut digest, &mut written, chunk),
                    )?;
                } else {
                    loop {
                        builder.check()?;
                        let read = input.read(&mut buffer)?;
                        if read == 0 {
                            break;
                        }
                        if written
                            .checked_add(read as u64)
                            .is_none_or(|n| n > source.content.bytes())
                        {
                            return Err(io::ErrorKind::InvalidData.into());
                        }
                        builder.write(&mut output, &mut digest, &mut written, &buffer[..read])?;
                    }
                }
                if written != source.content.bytes()
                    || (Content::File {
                        bytes: written,
                        digest: digest.finalize().into(),
                    }) != source.content
                {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                self.verify_handle(source, &input)?;
                output.sync_all()?;
            }
        }
        builder.finish((0..self.nodes.len()).map(|i| self.output_mode(i)))
    }
}

#[cfg(test)]
mod path_string_tests {
    use super::*;

    #[test]
    fn relative_strings_preserve_source_paths_and_reject_escape() {
        for text in [
            "",
            "docs/ação com espaços/日本語.md",
            "quote\"and\\slash\tline\n",
        ] {
            let value = RelativePath(PathBuf::from(text));
            let encoded = serde_json::to_value(&value).unwrap();
            assert_eq!(encoded, serde_json::json!(text));
            assert!(serde_json::from_value::<RelativePath>(encoded).unwrap() == value);
        }
        for text in ["../outside", "a/../outside", "/absolute", "nul\0"] {
            assert!(serde_json::from_value::<RelativePath>(serde_json::json!(text)).is_err());
        }
    }

    #[test]
    fn symlink_target_is_a_validated_optional_string() {
        #[derive(Serialize, Deserialize)]
        struct Link {
            #[serde(with = "native_link")]
            target: Option<PathBuf>,
        }
        for text in [None, Some("../ação com espaços"), Some("quote\"and\\slash")] {
            let value = Link {
                target: text.map(PathBuf::from),
            };
            let encoded = serde_json::to_value(&value).unwrap();
            assert_eq!(encoded["target"], serde_json::json!(text));
            assert_eq!(
                serde_json::from_value::<Link>(encoded).unwrap().target,
                value.target
            );
        }
        assert!(serde_json::from_value::<Link>(serde_json::json!({"target":"x\0"})).is_err());
    }
}

#[cfg(test)]
mod content_fence_tests {
    use super::*;
    use std::time::Duration;

    fn captured_file() -> (tempfile::TempDir, fs::File, SourceNode, NodeReceipt) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("installed");
        fs::write(&path, b"original").unwrap();
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let node = SourceNode::from_native_file(
            Path::new(""),
            Content::File {
                bytes: 8,
                digest: Sha256::digest(b"original").into(),
            },
            &file,
        )
        .unwrap();
        let receipt = NodeReceipt {
            relative: node.relative.clone(),
            id: node.opened.id.clone(),
            mode: node.mode,
            content: node.content.clone(),
            verified: Some(node.opened.content_fence()),
        };
        (directory, file, node, receipt)
    }

    #[test]
    fn persisted_receipt_stores_one_identity_and_only_temporal_verified_fields() {
        let (_directory, _file, node, receipt) = captured_file();
        let value = serde_json::to_value(&receipt).unwrap();
        let fence = value["verified"].as_object().unwrap();
        assert!(fence.contains_key("modified"));
        assert!(
            fence
                .keys()
                .all(|key| matches!(key.as_str(), "modified" | "change"))
        );
        for field in ["id", "directory", "mode", "bytes"] {
            assert!(!fence.contains_key(field));
        }
        let restored: NodeReceipt = serde_json::from_value(value.clone()).unwrap();
        assert!(node.matches_verified_receipt(&restored));
        let tree = TreeReceipt {
            nodes: vec![restored],
        };
        tree.validate(CopyLimits::new(8, 1, 1, 1).unwrap(), Kind::File)
            .unwrap();

        let mut previous = value.clone();
        previous["verified"] = serde_json::to_value(&node.opened).unwrap();
        let old_bytes = serde_json::to_vec(&previous).unwrap().len();
        let new_bytes = serde_json::to_vec(&value).unwrap().len();
        assert!(new_bytes < old_bytes);
        eprintln!("one verified file receipt: {old_bytes} -> {new_bytes} JSON bytes");
        // The eliminated representation is not another accepted input format.
        assert!(serde_json::from_value::<NodeReceipt>(previous).is_err());
    }

    #[test]
    fn native_copy_match_requires_receipt_identity_mode_length_and_temporal_fence() {
        let (_directory, file, node, receipt) = captured_file();
        assert!(node.matches_verified_receipt(&receipt));
        let mut changed = receipt.clone();
        changed.id = PhysicalId::largest();
        assert!(!node.matches_verified_receipt(&changed));
        changed = receipt.clone();
        changed.content = Content::File {
            bytes: 9,
            digest: Sha256::digest(b"original").into(),
        };
        assert!(!node.matches_verified_receipt(&changed));
        changed = receipt.clone();
        changed.verified = None;
        assert!(!node.matches_verified_receipt(&changed));
        changed = receipt.clone();
        changed.mode = match changed.mode {
            Mode::Unix(bits) => Mode::Unix(bits ^ 0o100),
            Mode::Windows(readonly) => Mode::Windows(!readonly),
        };
        assert!(!node.matches_verified_receipt(&changed));

        // Keep identity, file length and caller-supplied content equal: metadata
        // alone must refuse reuse of the previous hash after a native change.
        file.set_times(
            fs::FileTimes::new().set_modified(
                node.opened
                    .modified
                    .checked_add(Duration::from_secs(10))
                    .unwrap(),
            ),
        )
        .unwrap();
        let changed =
            SourceNode::from_native_file(Path::new(""), node.content.clone(), &file).unwrap();
        assert!(!changed.matches_verified_receipt(&receipt));
    }

    #[test]
    fn native_mode_uses_its_scalar_type_without_an_os_tag() {
        for (mode, expected) in [
            (Mode::Unix(0o644), serde_json::json!(0o644)),
            (Mode::Windows(false), serde_json::json!(false)),
            (Mode::Windows(true), serde_json::json!(true)),
        ] {
            assert_eq!(serde_json::to_value(mode).unwrap(), expected);
            assert!(serde_json::from_value::<Mode>(expected).unwrap() == mode);
        }
        for invalid in [
            serde_json::json!({"os":"Unix","value":420}),
            serde_json::json!({"os":"Windows","value":false}),
            serde_json::json!(-1),
            serde_json::json!(1.5),
            serde_json::json!("420"),
        ] {
            assert!(serde_json::from_value::<Mode>(invalid).is_err());
        }
        #[cfg(unix)]
        {
            assert!(Mode::Unix(0o777).validate().is_ok());
            assert!(Mode::Unix(0o1000).validate().is_err());
            assert!(Mode::Windows(false).validate().is_err());
        }
        #[cfg(windows)]
        {
            assert!(Mode::Windows(false).validate().is_ok());
            assert!(Mode::Unix(0o644).validate().is_err());
        }
    }

    #[test]
    fn temporal_receipt_rejects_invalid_timestamp_components() {
        let (_directory, _file, _node, receipt) = captured_file();
        let mut value = serde_json::to_value(&receipt).unwrap();
        value["verified"]["modified"]["nanos"] = serde_json::json!(1_000_000_000u64);
        assert!(serde_json::from_value::<NodeReceipt>(value).is_err());
        #[cfg(unix)]
        {
            let mut value = serde_json::to_value(&receipt).unwrap();
            value["verified"]["change"][1] = serde_json::json!(1_000_000_000i64);
            let node = serde_json::from_value::<NodeReceipt>(value).unwrap();
            let tree = TreeReceipt { nodes: vec![node] };
            assert!(
                tree.validate(CopyLimits::new(8, 1, 1, 1).unwrap(), Kind::File)
                    .is_err()
            );
        }
    }
}
