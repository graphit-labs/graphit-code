//! Read-only source capture. Descriptors and hashes convey no physical authority.

use super::{
    CopyLimits, CopyPolicy,
    tree::{Content, Mode, SourcePlan},
};
use graphit_fs::NodeKind;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

pub use crate::file_io::SnapshotContext;

/// Bounds retained content bytes and selected file count. Unselected bodies are not retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetainedBytesLimits {
    pub(super) bytes: usize,
    pub(super) files: usize,
}

impl RetainedBytesLimits {
    pub fn new(bytes: usize, files: usize) -> io::Result<Self> {
        if bytes == 0 || files == 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self { bytes, files })
    }
}

/// Validated immutable selection, with native relative paths (empty means root).
#[derive(Clone)]
pub struct RetainedFileSet {
    pub(super) paths: BTreeSet<PathBuf>,
}

impl RetainedFileSet {
    pub fn new(
        paths: impl IntoIterator<Item = PathBuf>,
        limits: RetainedBytesLimits,
    ) -> io::Result<Self> {
        let mut selected = BTreeSet::new();
        for path in paths {
            validate_relative(&path)?;
            if selected.len() >= limits.files {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            if !selected.insert(path) {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        }
        Ok(Self { paths: selected })
    }
}

/// Capture policies have no implicit resource defaults.
#[derive(Clone)]
pub struct SnapshotPolicy {
    pub(super) copy: CopyLimits,
    pub(super) links: CopyPolicy,
    pub(super) selected: RetainedFileSet,
    pub(super) retention: RetainedBytesLimits,
}

impl SnapshotPolicy {
    pub fn new(
        links: CopyPolicy,
        copy: CopyLimits,
        selected: RetainedFileSet,
        retention: RetainedBytesLimits,
    ) -> io::Result<Self> {
        // A set may have been constructed under a different policy.
        let selected = RetainedFileSet::new(selected.paths, retention)?;
        Ok(Self {
            copy,
            links,
            selected,
            retention,
        })
    }
}

// The enclosing snapshot Arc shares the complete immutable capture. Keeping
// each Vec in place avoids a second allocation/copy when converting to Arc<[u8]>.
pub(super) type RetainedSources = BTreeMap<PathBuf, Vec<u8>>;

struct CapturedSource {
    plan: SourcePlan,
    kind: NodeKind,
    retained: RetainedSources,
    payload_digests: bool,
    coverage: Option<super::native_source::SelectionCoverage>,
}

/// Immutable content manifest and explicitly retained bytes; not filesystem authority.
#[derive(Clone)]
pub struct SourceSnapshot(Arc<CapturedSource>);
impl SourceSnapshot {
    pub(super) fn plan(&self) -> &SourcePlan {
        &self.0.plan
    }
    pub(super) fn source(&self) -> &Path {
        self.0.plan.source()
    }
    pub(super) fn selected(
        plan: SourcePlan,
        retained: RetainedSources,
        coverage: super::native_source::SelectionCoverage,
    ) -> Self {
        Self(Arc::new(CapturedSource {
            plan,
            kind: NodeKind::Directory,
            retained,
            payload_digests: true,
            coverage: Some(coverage),
        }))
    }
    pub fn capture(
        source: &Path,
        kind: NodeKind,
        policy: &SnapshotPolicy,
        context: &SnapshotContext,
    ) -> io::Result<Self> {
        context.check()?;
        let (plan, retained) = SourcePlan::capture_snapshot(source, kind.into(), policy, context)?;
        for relative in &policy.selected.paths {
            context.check()?;
            if !retained.contains_key(relative) {
                let kind = if plan.node(relative).is_some() {
                    io::ErrorKind::InvalidInput
                } else {
                    io::ErrorKind::NotFound
                };
                return Err(io::Error::new(
                    kind,
                    "requested retained source is not a file in the captured manifest",
                ));
            }
        }
        context.check()?;
        Ok(Self(Arc::new(CapturedSource {
            plan,
            kind,
            retained,
            payload_digests: true,
            coverage: None,
        })))
    }
    pub(super) fn capture_generated_metadata(
        source: &Path,
        files: &[PathBuf],
        context: &SnapshotContext,
    ) -> io::Result<Self> {
        context.check()?;
        let plan = SourcePlan::capture_generated_metadata(source, files, context)?;
        context.check()?;
        Ok(Self(Arc::new(CapturedSource {
            plan,
            kind: NodeKind::Directory,
            retained: BTreeMap::new(),
            payload_digests: false,
            coverage: None,
        })))
    }
    pub fn capture_generated(
        source: &Path,
        kind: NodeKind,
        context: &SnapshotContext,
    ) -> io::Result<Self> {
        context.check()?;
        let plan = SourcePlan::capture_generated(source, kind.into(), context)?;
        context.check()?;
        Ok(Self(Arc::new(CapturedSource {
            plan,
            kind,
            retained: BTreeMap::new(),
            payload_digests: true,
            coverage: None,
        })))
    }
    pub(super) fn has_payload_digests(&self) -> bool {
        self.0.payload_digests
    }
    pub(super) fn covers_selected_path(&self, path: &Path) -> io::Result<bool> {
        validate_relative(path)?;
        self.0
            .coverage
            .as_ref()
            .map(|coverage| coverage.covers(path))
            .ok_or_else(|| io::ErrorKind::InvalidInput.into())
    }
    pub fn retained(&self, relative: &Path) -> Option<&[u8]> {
        self.0.retained.get(relative).map(Vec::as_slice)
    }
    pub fn retained_bytes(&self) -> usize {
        self.0.retained.values().map(Vec::len).sum()
    }
    pub fn kind(&self) -> NodeKind {
        self.0.kind
    }
    pub fn fingerprint(&self) -> [u8; 32] {
        self.0.plan.fingerprint()
    }
    pub fn nodes(&self) -> impl ExactSizeIterator<Item = SnapshotNode<'_>> {
        self.0.plan.nodes().map(|node| SnapshotNode {
            relative: &node.relative.0,
            mode: PayloadMode(node.mode),
            content: &node.content,
            has_payload_digest: self.has_payload_digests(),
        })
    }
}

/// Observational native mode, usable as an explicit mode for generated output.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PayloadMode(pub(super) Mode);

impl PayloadMode {
    /// Desired attributes for a newly generated private file: Unix 0600 or Windows writable.
    ///
    /// This value performs no IO and grants no ACL, creation or publication authority.
    pub fn private_file() -> Self {
        #[cfg(unix)]
        let mode = Mode::Unix(0o600);
        #[cfg(not(unix))]
        let mode = Mode::Windows(false);
        Self(mode)
    }

    /// Desired attributes for a newly generated private directory: Unix 0700 or Windows writable.
    ///
    /// The actual private creation and ACL checks remain with the filesystem producer.
    pub fn private_directory() -> Self {
        #[cfg(unix)]
        let mode = Mode::Unix(0o700);
        #[cfg(not(unix))]
        let mode = Mode::Windows(false);
        Self(mode)
    }

    pub fn from_permissions(permissions: &std::fs::Permissions) -> io::Result<Self> {
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            Mode::Unix(permissions.mode() & 0o777)
        };
        #[cfg(not(unix))]
        let mode = Mode::Windows(permissions.readonly());
        mode.validate()?;
        Ok(Self(mode))
    }

    pub fn unix_bits(self) -> Option<u32> {
        match self.0 {
            Mode::Unix(bits) => Some(bits),
            _ => None,
        }
    }

    pub fn windows_readonly(self) -> Option<bool> {
        match self.0 {
            Mode::Windows(value) => Some(value),
            _ => None,
        }
    }
}

/// A manifest entry with the observed content length, digest and mode.
pub struct SnapshotNode<'a> {
    pub(super) relative: &'a Path,
    pub(super) mode: PayloadMode,
    pub(super) content: &'a Content,
    pub(super) has_payload_digest: bool,
}

impl SnapshotNode<'_> {
    pub fn relative(&self) -> &Path {
        self.relative
    }
    pub fn mode(&self) -> PayloadMode {
        self.mode
    }
    pub fn kind(&self) -> NodeKind {
        if self.content.directory() {
            NodeKind::Directory
        } else {
            NodeKind::File
        }
    }
    pub fn bytes(&self) -> u64 {
        self.content.bytes()
    }
    pub fn digest(&self) -> Option<[u8; 32]> {
        if !self.has_payload_digest {
            return None;
        }
        match self.content {
            Content::Directory => None,
            Content::File { digest, .. } => Some(*digest),
        }
    }
}

pub(super) fn validate_relative(path: &Path) -> io::Result<()> {
    crate::native_path::valid_native(path)?;
    if path
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}
pub(super) fn canonical_relative(path: &Path) -> io::Result<PathBuf> {
    validate_relative(path)?;
    // Rebuild validated names with native separators, as filesystem capture does.
    Ok(path.components().collect())
}
