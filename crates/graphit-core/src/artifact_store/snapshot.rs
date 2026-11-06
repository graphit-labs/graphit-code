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

/// Positive aggregate retention bound. Non-selected assets never enter RAM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetainedBytesLimits {
    pub(super) bytes: usize,
    pub(super) files: usize,
    pub(super) path_bytes: usize,
}

impl RetainedBytesLimits {
    pub fn new(bytes: usize, files: usize, path_bytes: usize) -> io::Result<Self> {
        if bytes == 0 || files == 0 || path_bytes == 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self {
            bytes,
            files,
            path_bytes,
        })
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
        let mut units = 0usize;
        for path in paths {
            let remaining = limits
                .path_bytes
                .checked_sub(units)
                .ok_or(io::ErrorKind::FileTooLarge)?;
            let admitted_units = native_selection_units(&path, remaining)?;
            validate_relative(&path)?;
            units = units
                .checked_add(admitted_units)
                .ok_or(io::ErrorKind::FileTooLarge)?;
            if selected.len() >= limits.files || units > limits.path_bytes {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            if !selected.insert(path) {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        }
        Ok(Self { paths: selected })
    }
}

fn native_selection_units(path: &Path, remaining: usize) -> io::Result<usize> {
    #[cfg(unix)]
    {
        let bytes = path.as_os_str().as_encoded_bytes().len();
        if bytes > remaining {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        Ok(bytes)
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let mut bytes = 0usize;
        for _ in path.as_os_str().encode_wide() {
            bytes = bytes.checked_add(2).ok_or(io::ErrorKind::FileTooLarge)?;
            if bytes > remaining {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
        }
        Ok(bytes)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (path, remaining);
        Err(io::ErrorKind::Unsupported.into())
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

pub(super) enum CapturePolicy {
    Generic(SnapshotPolicy),
    Generated,
    GeneratedMetadata,
    Selected,
}

struct CapturedSource {
    plan: SourcePlan,
    canonical_source: PathBuf,
    kind: NodeKind,
    policy: CapturePolicy,
    retained: RetainedSources,
    native: Option<Arc<super::native_source::NativeSource>>,
}

/// Read-only proof of the observed source, sharing captured files and metadata.
#[derive(Clone)]
pub struct SourceSnapshot(Arc<CapturedSource>);

impl SourceSnapshot {
    pub(super) fn plan(&self) -> &SourcePlan {
        &self.0.plan
    }
    pub(super) fn canonical_source(&self) -> &Path {
        &self.0.canonical_source
    }
    pub(super) fn link_policy(&self) -> CopyPolicy {
        match &self.0.policy {
            CapturePolicy::Generic(policy) => policy.links,
            CapturePolicy::Generated
            | CapturePolicy::GeneratedMetadata
            | CapturePolicy::Selected => CopyPolicy::RejectFileLinks,
        }
    }

    pub(super) fn check_reader_binding(
        &self,
        index: usize,
        file: Option<&std::fs::File>,
        context: &SnapshotContext,
    ) -> io::Result<()> {
        context.check()?;
        if let Some(native) = &self.0.native {
            return native.check_binding(index, file, &self.0.plan, context);
        }
        if std::fs::canonicalize(self.0.plan.source())? != self.0.canonical_source {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if matches!(self.0.policy, CapturePolicy::GeneratedMetadata) {
            self.0
                .plan
                .check_metadata_reader_node(index, file, context)?;
        } else {
            self.0.plan.check_reader_node(index, file, context)?;
        }
        if std::fs::canonicalize(self.0.plan.source())? != self.0.canonical_source {
            return Err(io::ErrorKind::InvalidData.into());
        }
        context.check()
    }

    pub fn binding(&self) -> io::Result<super::SnapshotBindingRecord> {
        super::SnapshotBindingRecord::from_snapshot(self)
    }

    pub(super) fn native(&self) -> Option<&Arc<super::native_source::NativeSource>> {
        self.0.native.as_ref()
    }
    pub(super) fn validate_reader_relative(&self, path: &Path) -> io::Result<()> {
        match &self.0.native {
            Some(native) => native.validate_suffix(path),
            None => validate_relative(path),
        }
    }
    pub(super) fn before_native_read(&self) -> io::Result<()> {
        match &self.0.native {
            Some(native) => native.request().check(),
            None => Ok(()),
        }
    }
    pub(super) fn reader_last_modified(
        &self,
        index: usize,
        file: &std::fs::File,
    ) -> io::Result<std::time::SystemTime> {
        if let Some(native) = &self.0.native {
            native.request().check()?;
        }
        self.0.plan.reader_last_modified(index, file)
    }
    pub(super) fn capture_native_scope(
        scope: &super::CapturedPublicationScope,
        relative: &Path,
        kind: NodeKind,
        policy: &SnapshotPolicy,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Self> {
        let (native, plan, retained) =
            super::native_source::NativeSource::capture(scope, relative, kind, policy, request)?;
        Self::from_native_parts(
            native,
            plan,
            retained,
            kind,
            CapturePolicy::Generic(policy.clone()),
        )
    }
    pub(super) fn from_native_parts(
        native: Arc<super::native_source::NativeSource>,
        plan: SourcePlan,
        retained: RetainedSources,
        kind: NodeKind,
        policy: CapturePolicy,
    ) -> io::Result<Self> {
        // The stored paths/policy are binding data only. The reader
        // dispatch never uses this path to reopen PROJECT payload.
        native.request().check()?;
        let result = Self(Arc::new(CapturedSource {
            canonical_source: plan.source().to_owned(),
            plan,
            kind,
            policy,
            retained,
            native: Some(native),
        }));
        result
            .native()
            .ok_or(io::ErrorKind::InvalidData)?
            .request()
            .check()?;
        Ok(result)
    }
    pub fn capture(
        source: &Path,
        kind: NodeKind,
        policy: &SnapshotPolicy,
        context: &SnapshotContext,
    ) -> io::Result<Self> {
        crate::native_path::valid_path(source)?;
        context.check()?;
        let canonical_source = std::fs::canonicalize(source)?;
        crate::native_path::valid_path(&canonical_source)?;
        context.check()?;
        let (plan, retained) = SourcePlan::capture_snapshot(source, kind.into(), policy, context)?;
        if std::fs::canonicalize(source)? != canonical_source {
            return Err(io::ErrorKind::InvalidData.into());
        }
        context.check()?;
        Ok(Self(Arc::new(CapturedSource {
            plan,
            canonical_source,
            kind,
            policy: CapturePolicy::Generic(policy.clone()),
            retained,
            native: None,
        })))
    }

    pub(super) fn capture_generated_metadata(
        source: &Path,
        files: &[PathBuf],
        context: &SnapshotContext,
    ) -> io::Result<Self> {
        crate::native_path::valid_path(source)?;
        context.check()?;
        let canonical_source = std::fs::canonicalize(source)?;
        let plan = SourcePlan::capture_generated_metadata(source, files, context)?;
        if std::fs::canonicalize(source)? != canonical_source {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(Self(Arc::new(CapturedSource {
            plan,
            canonical_source,
            kind: NodeKind::Directory,
            policy: CapturePolicy::GeneratedMetadata,
            retained: BTreeMap::new(),
            native: None,
        })))
    }
    pub(super) fn has_payload_digests(&self) -> bool {
        !matches!(self.0.policy, CapturePolicy::GeneratedMetadata)
    }
    /// Capture completed output produced locally under the caller's authority.
    /// Bodies are hashed and streamed, without corpus quotas or retained copies.
    pub fn capture_generated(
        source: &Path,
        kind: NodeKind,
        context: &SnapshotContext,
    ) -> io::Result<Self> {
        crate::native_path::valid_path(source)?;
        context.check()?;
        let canonical_source = std::fs::canonicalize(source)?;
        crate::native_path::valid_path(&canonical_source)?;
        let plan = SourcePlan::capture_generated(source, kind.into(), context)?;
        if std::fs::canonicalize(source)? != canonical_source {
            return Err(io::ErrorKind::InvalidData.into());
        }
        context.check()?;
        Ok(Self(Arc::new(CapturedSource {
            plan,
            canonical_source,
            kind,
            policy: CapturePolicy::Generated,
            retained: RetainedSources::new(),
            native: None,
        })))
    }

    pub fn retained(&self, relative: &Path) -> Option<&[u8]> {
        self.0.retained.get(relative).map(Vec::as_slice)
    }

    pub fn retained_bytes(&self) -> usize {
        self.0.retained.values().map(|v| v.len()).sum()
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

    /// Recheck captured paths, inventory and native observations without rereading bodies.
    pub(super) fn revalidate_bindings(&self, context: &SnapshotContext) -> io::Result<()> {
        context.check()?;
        self.check_reader_binding(0, None, context)?;
        match &self.0.policy {
            CapturePolicy::Generic(policy) => self
                .0
                .plan
                .recheck_bindings_with_context(policy.copy, context)?,
            CapturePolicy::Generated => self.0.plan.recheck_generated_bindings(context)?,
            CapturePolicy::GeneratedMetadata => self.0.plan.recheck_metadata_bindings(context)?,
            CapturePolicy::Selected => {
                // Unselected files are outside this capture. Native selection
                // controls and each selected binding still belong to its Original.
                let native = self.0.native.as_ref().ok_or(io::ErrorKind::InvalidData)?;
                native.revalidate_selection()?;
                for index in 0..self.0.plan.nodes().len() {
                    self.check_reader_binding(index, None, context)?;
                }
                native.revalidate_selection()?;
            }
        }
        self.check_reader_binding(0, None, context)?;
        context.check()
    }

    /// Re-capture and compare content plus native observations, without grants.
    pub fn revalidate_content(&self, context: &SnapshotContext) -> io::Result<()> {
        context.check()?;
        if let Some(native) = &self.0.native {
            native.request().check()?;
            if !context.same_request(native.request()) {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        }
        let fresh = match &self.0.policy {
            CapturePolicy::Generated => {
                Self::capture_generated(self.0.plan.source(), self.0.kind, context)?
            }
            CapturePolicy::GeneratedMetadata => return Err(io::ErrorKind::Unsupported.into()),
            CapturePolicy::Selected => self
                .0
                .native
                .as_ref()
                .ok_or(io::ErrorKind::InvalidData)?
                .recapture_selected()?,
            CapturePolicy::Generic(policy) => {
                let empty = SnapshotPolicy::new(
                    policy.links,
                    policy.copy,
                    RetainedFileSet::new([], policy.retention)?,
                    policy.retention,
                )?;
                match &self.0.native {
                    Some(native) => native.recapture(&empty)?,
                    None => Self::capture(self.0.plan.source(), self.0.kind, &empty, context)?,
                }
            }
        };
        let same = match &self.0.policy {
            CapturePolicy::Selected => self.0.plan.same_selected_source(&fresh.0.plan),
            CapturePolicy::Generic(_)
            | CapturePolicy::Generated
            | CapturePolicy::GeneratedMetadata => self.0.plan.same_source(&fresh.0.plan),
        };
        if self.0.canonical_source != fresh.0.canonical_source || !same {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if let Some(native) = &self.0.native {
            native.request().check()?;
        }
        context.check()
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

/// A bounded manifest view; physical identity and the private proof stay inside Core.
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
    native_selection_units(path, crate::native_path::MAX_PATH_BYTES)?;
    let bytes = path.as_os_str().as_encoded_bytes();
    if bytes.is_empty() {
        return Ok(());
    }
    let separator = if cfg!(windows) { b'\\' } else { b'/' };
    let mut start = 0;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if byte == 0 || (cfg!(windows) && matches!(byte, b'/' | b':')) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if byte == separator {
            if matches!(&bytes[start..index], b"" | b"." | b"..") {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            start = index + 1;
        }
    }
    if matches!(&bytes[start..], b"" | b"." | b"..")
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}

// Generated payload plans normalize their paths; native source selections use
// validate_relative instead and must already be canonical.
pub(super) fn canonical_relative(path: &Path) -> io::Result<PathBuf> {
    crate::native_path::valid_native(path)?;
    native_selection_units(path, crate::native_path::MAX_PATH_BYTES)?;
    let mut canonical = PathBuf::new();
    for part in path.components() {
        match part {
            Component::Normal(name) => canonical.push(name),
            _ => return Err(io::ErrorKind::InvalidInput.into()),
        }
    }
    Ok(canonical)
}

pub(super) fn path_units(path: &Path) -> io::Result<usize> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(path.as_os_str().as_bytes().len())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str()
            .encode_wide()
            .count()
            .checked_mul(2)
            .ok_or_else(|| io::ErrorKind::FileTooLarge.into())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        Err(io::ErrorKind::Unsupported.into())
    }
}
