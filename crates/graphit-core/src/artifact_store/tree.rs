//! Logical payload contents and conventional file access. No directory identity protocol.
use super::{codec::Kind, *};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
};

pub(super) const BUFFER_BYTES: usize = 64 * 1024;
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "PathBuf", into = "PathBuf")]
pub(super) struct RelativePath(pub PathBuf);
impl TryFrom<PathBuf> for RelativePath {
    type Error = io::Error;
    fn try_from(path: PathBuf) -> io::Result<Self> {
        snapshot::validate_relative(&path)?;
        Ok(Self(path))
    }
}
impl From<RelativePath> for PathBuf {
    fn from(path: RelativePath) -> Self {
        path.0
    }
}

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
        match self {
            #[cfg(unix)]
            Self::Unix(bits) if bits <= 0o777 => Ok(()),
            #[cfg(windows)]
            Self::Windows(_) => Ok(()),
            _ => Err(io::ErrorKind::InvalidInput.into()),
        }
    }
    /// Apply package attributes only to newly created output, never to gain access to an existing file.
    pub fn set(self, path: &Path) -> io::Result<()> {
        self.validate()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Self::Unix(bits) = self {
                return fs::set_permissions(path, fs::Permissions::from_mode(bits));
            }
        }
        #[cfg(windows)]
        {
            if let Self::Windows(readonly) = self {
                let mut p = fs::metadata(path)?.permissions();
                p.set_readonly(readonly);
                return fs::set_permissions(path, p);
            }
        }
        Err(io::ErrorKind::Unsupported.into())
    }
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
    pub fn bytes(&self) -> u64 {
        match self {
            Self::Directory => 0,
            Self::File { bytes, .. } => *bytes,
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
pub(super) struct SourceNode {
    pub relative: RelativePath,
    pub mode: Mode,
    pub content: Content,
}
pub(super) struct SourcePlan {
    source: PathBuf,
    nodes: Vec<SourceNode>,
}

pub(super) struct Budget {
    limits: CopyLimits,
    nodes: usize,
    bytes: u64,
}
impl Budget {
    pub fn new(limits: CopyLimits) -> Self {
        Self {
            limits,
            nodes: 0,
            bytes: 0,
        }
    }
    pub fn add(&mut self, bytes: u64) -> io::Result<()> {
        self.nodes = self
            .nodes
            .checked_add(1)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if self.nodes > self.limits.max_tree_nodes || self.bytes > self.limits.max_total_file_bytes
        {
            return Err(io::ErrorKind::FileTooLarge.into());
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
fn digest_file(
    file: &mut fs::File,
    expected: u64,
    context: Option<&SnapshotContext>,
    cancellation: &Cancellation,
) -> io::Result<(u64, [u8; 32])> {
    let mut digest = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = vec![0; BUFFER_BYTES];
    loop {
        check_cancelled(cancellation)?;
        if let Some(c) = context {
            c.check()?;
        }
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes = bytes
            .checked_add(n as u64)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if bytes > expected {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "source file grew while reading",
            ));
        }
        digest.update(&buffer[..n]);
    }
    if bytes != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "source file length changed while reading",
        ));
    }
    Ok((bytes, digest.finalize().into()))
}
impl SourcePlan {
    pub fn from_nodes(source: PathBuf, mut nodes: Vec<SourceNode>) -> Self {
        nodes.sort_by(|a, b| a.relative.cmp(&b.relative));
        Self { source, nodes }
    }
    pub fn source(&self) -> &Path {
        &self.source
    }
    pub fn nodes(&self) -> impl ExactSizeIterator<Item = &SourceNode> {
        self.nodes.iter()
    }
    pub fn node(&self, relative: &Path) -> Option<(usize, &SourceNode)> {
        self.nodes
            .binary_search_by(|n| n.relative.0.as_path().cmp(relative))
            .ok()
            .map(|i| (i, &self.nodes[i]))
    }
    pub fn indexed_node(&self, index: usize) -> io::Result<&SourceNode> {
        self.nodes
            .get(index)
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }
    pub fn open_file(&self, node: &SourceNode) -> io::Result<fs::File> {
        fs::File::open(node_path(&self.source, &node.relative.0))
    }
    pub fn fingerprint(&self) -> [u8; 32] {
        fingerprint(
            self.nodes
                .iter()
                .map(|n| (n.relative.0.as_path(), n.mode, &n.content)),
        )
    }
    pub fn capture_with_context(
        source: &Path,
        kind: Kind,
        links: CopyPolicy,
        limits: CopyLimits,
        context: &SnapshotContext,
    ) -> io::Result<Self> {
        Self::read(
            source,
            kind,
            links,
            Some(limits),
            context.cancellation(),
            Some(context),
            None,
        )
        .map(|p| p.0)
    }
    pub fn capture_snapshot(
        source: &Path,
        kind: Kind,
        policy: &SnapshotPolicy,
        context: &SnapshotContext,
    ) -> io::Result<(Self, snapshot::RetainedSources)> {
        Self::read(
            source,
            kind,
            policy.links,
            Some(policy.copy),
            context.cancellation(),
            Some(context),
            Some(policy),
        )
    }
    pub fn capture_generated(
        source: &Path,
        kind: Kind,
        context: &SnapshotContext,
    ) -> io::Result<Self> {
        Self::read(
            source,
            kind,
            CopyPolicy::FollowLinks,
            None,
            context.cancellation(),
            Some(context),
            None,
        )
        .map(|p| p.0)
    }
    pub fn capture_generated_metadata(
        source: &Path,
        files: &[PathBuf],
        context: &SnapshotContext,
    ) -> io::Result<Self> {
        let mut paths = BTreeSet::from([PathBuf::new()]);
        for relative in files {
            snapshot::validate_relative(relative)?;
            paths.insert(relative.to_owned());
        }
        let mut nodes = Vec::new();
        for relative in paths {
            context.check()?;
            let metadata = fs::metadata(node_path(source, &relative))?;
            let content = if metadata.is_dir() {
                Content::Directory
            } else if metadata.is_file() {
                Content::File {
                    bytes: metadata.len(),
                    digest: [0; 32],
                }
            } else {
                return Err(io::ErrorKind::InvalidInput.into());
            };
            nodes.push(SourceNode {
                relative: RelativePath(relative),
                mode: Mode::observe(&metadata),
                content,
            });
        }
        Ok(Self {
            source: source.to_owned(),
            nodes,
        })
    }
    fn read(
        source: &Path,
        kind: Kind,
        links: CopyPolicy,
        limits: Option<CopyLimits>,
        cancel: &Cancellation,
        context: Option<&SnapshotContext>,
        policy: Option<&SnapshotPolicy>,
    ) -> io::Result<(Self, snapshot::RetainedSources)> {
        let root = fs::metadata(source)?;
        if root.is_dir() != (kind == Kind::Directory) || (!root.is_dir() && !root.is_file()) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let mut budget = limits.map(Budget::new);
        let mut nodes = Vec::new();
        let mut retained = snapshot::RetainedSources::new();
        let mut retained_bytes = 0usize;
        // Follow configured source roots and file links. Directory recursion remains cycle-safe through WalkDir.
        for entry in walkdir::WalkDir::new(source)
            .follow_root_links(true)
            .follow_links(links == CopyPolicy::FollowLinks)
            .sort_by_file_name()
        {
            check_cancelled(cancel)?;
            if let Some(c) = context {
                c.check()?;
            }
            let entry = entry.map_err(io::Error::other)?;
            let relative = entry
                .path()
                .strip_prefix(source)
                .map_err(io::Error::other)?
                .to_owned();
            snapshot::validate_relative(&relative)?;
            let metadata = fs::metadata(entry.path())?;
            if entry.path_is_symlink()
                && entry.depth() != 0
                && links == CopyPolicy::RejectEntryLinks
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "payload entry is a link not admitted by its installation policy",
                ));
            }
            let content = if metadata.is_dir() {
                Content::Directory
            } else if metadata.is_file() {
                if let Some(b) = &mut budget {
                    b.add(metadata.len())?;
                }
                let mut file = fs::File::open(entry.path())?;
                if let Some(policy) = policy.filter(|p| p.selected.paths.contains(&relative)) {
                    let remaining = policy
                        .retention
                        .bytes
                        .checked_sub(retained_bytes)
                        .ok_or(io::ErrorKind::FileTooLarge)?;
                    let mut bytes = Vec::new();
                    file.take(remaining as u64 + 1).read_to_end(&mut bytes)?;
                    if bytes.len() > remaining || retained.len() >= policy.retention.files {
                        return Err(io::ErrorKind::FileTooLarge.into());
                    }
                    if bytes.len() as u64 != metadata.len() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "source file length changed while reading",
                        ));
                    }
                    retained_bytes += bytes.len();
                    let content = Content::File {
                        bytes: bytes.len() as u64,
                        digest: Sha256::digest(&bytes).into(),
                    };
                    retained.insert(relative.clone(), bytes);
                    content
                } else {
                    let (bytes, digest) = digest_file(&mut file, metadata.len(), context, cancel)?;
                    Content::File { bytes, digest }
                }
            } else {
                return Err(io::ErrorKind::InvalidInput.into());
            };
            if content.directory()
                && let Some(b) = &mut budget
            {
                b.add(0)?;
            }
            nodes.push(SourceNode {
                relative: RelativePath(relative),
                mode: Mode::observe(&metadata),
                content,
            });
        }
        nodes.sort_by(|a, b| a.relative.cmp(&b.relative));
        Ok((
            Self {
                source: source.to_owned(),
                nodes,
            },
            retained,
        ))
    }
    pub fn write_to(&self, root: &Path, context: &SnapshotContext) -> io::Result<()> {
        for node in &self.nodes {
            context.check()?;
            let destination = node_path(root, &node.relative.0);
            if node.content.directory() {
                fs::create_dir(&destination)?;
            } else {
                let mut input = self.open_file(node)?;
                let mut output = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&destination)?;
                let mut digest = Sha256::new();
                let mut count = 0u64;
                let mut buffer = vec![0; BUFFER_BYTES];
                loop {
                    context.check()?;
                    let n = input.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    count = count
                        .checked_add(n as u64)
                        .ok_or(io::ErrorKind::FileTooLarge)?;
                    if count > node.content.bytes() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "source file grew while copying",
                        ));
                    }
                    output.write_all(&buffer[..n])?;
                    digest.update(&buffer[..n]);
                }
                if (Content::File {
                    bytes: count,
                    digest: digest.finalize().into(),
                }) != node.content
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "source content changed during copying",
                    ));
                }
                output.sync_all()?;
            }
        }
        for node in self.nodes.iter().rev() {
            node.mode.set(&node_path(root, &node.relative.0))?;
        }
        Ok(())
    }
}
/// Content-only hash: benign mode/owner changes do not imply a user content edit.
pub(super) fn fingerprint<'a>(
    nodes: impl ExactSizeIterator<Item = (&'a Path, Mode, &'a Content)>,
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"graphit/payload-content/v2\0");
    digest.update((nodes.len() as u64).to_le_bytes());
    for (relative, _mode, content) in nodes {
        let relative = relative.as_os_str().as_encoded_bytes();
        digest.update((relative.len() as u64).to_le_bytes());
        digest.update(relative);
        match content {
            Content::Directory => digest.update(b"d"),
            Content::File {
                bytes,
                digest: hash,
            } => {
                digest.update(b"f");
                digest.update(bytes.to_le_bytes());
                digest.update(hash);
            }
        }
    }
    digest.finalize().into()
}
