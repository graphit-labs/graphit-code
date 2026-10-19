//! Validated output data. Only Core's private tree builder can materialize it.
use super::{
    CopyLimits, PayloadMode, SnapshotContext, SnapshotNode, SourceSnapshot,
    snapshot::{canonical_relative, validate_relative},
    tree::{self, Budget, Content, Mode, TreeBuilder, TreeReceipt},
};
use graphit_fs::NodeKind;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy)]
pub struct PayloadPolicy {
    kind: NodeKind,
    root_mode: PayloadMode,
    copy: CopyLimits,
}
impl PayloadPolicy {
    pub fn new(kind: NodeKind, root_mode: PayloadMode, copy: CopyLimits) -> Self {
        Self {
            kind,
            root_mode,
            copy,
        }
    }
}

pub struct PayloadOverlay {
    original: PathBuf,
    output: PathBuf,
    bytes: Vec<u8>,
    mode: PayloadMode,
}
impl PayloadOverlay {
    pub fn new(
        original: PathBuf,
        output: PathBuf,
        bytes: Vec<u8>,
        mode: PayloadMode,
    ) -> io::Result<Self> {
        let original = canonical_relative(&original)?;
        let output = canonical_relative(&output)?;
        Ok(Self {
            original,
            output,
            bytes,
            mode,
        })
    }
}

pub struct GeneratedNode {
    relative: PathBuf,
    mode: PayloadMode,
    bytes: Option<Vec<u8>>,
}
impl GeneratedNode {
    pub fn directory(relative: PathBuf, mode: PayloadMode) -> io::Result<Self> {
        let relative = canonical_relative(&relative)?;
        Ok(Self {
            relative,
            mode,
            bytes: None,
        })
    }
    pub fn file(relative: PathBuf, bytes: Vec<u8>, mode: PayloadMode) -> io::Result<Self> {
        let relative = canonical_relative(&relative)?;
        Ok(Self {
            relative,
            mode,
            bytes: Some(bytes),
        })
    }
}

#[derive(Clone)]
enum Data {
    Directory,
    Source(usize),
    Inline {
        bytes: Arc<Vec<u8>>,
        original: Option<usize>,
    },
}
#[derive(Clone)]
struct Node {
    relative: PathBuf,
    mode: Mode,
    content: Content,
    data: Data,
}

/// An immutable logical tree, never a restored filesystem proof.
///
/// ```compile_fail
/// use graphit_core::artifact_store::ValidatedPayloadPlan;
/// let plan: ValidatedPayloadPlan = serde_json::from_str("{}").unwrap();
/// ```
///
/// ```compile_fail
/// use graphit_core::artifact_store::ValidatedPayloadPlan;
/// fn overwrite(plan: &ValidatedPayloadPlan) { plan.build_tree(); }
/// ```
pub struct ValidatedPayloadPlan {
    source: Option<SourceSnapshot>,
    nodes: Vec<Node>,
    policy: PayloadPolicy,
}
impl ValidatedPayloadPlan {
    /// The real capture backing this plan, when it was built from an origin.
    /// Generated plans have no origin; this does not restore or grant authority.
    pub fn source_snapshot(&self) -> Option<&SourceSnapshot> {
        self.source.as_ref()
    }

    /// Read-only admission of the complete output tree under current caller limits.
    pub fn validate_limits(&self, limits: CopyLimits, context: &SnapshotContext) -> io::Result<()> {
        context.check()?;
        let mut budget = Budget::new(limits);
        for node in &self.nodes {
            context.check()?;
            budget.add(&node.relative, node.content.bytes())?;
        }
        Ok(())
    }

    /// Read-only admission of the selected output subtree under a caller policy.
    pub fn validate_selection_limits(
        &self,
        selection: &super::PayloadSelection,
        limits: CopyLimits,
        context: &SnapshotContext,
    ) -> io::Result<()> {
        context.check()?;
        let mut found = false;
        let mut budget = Budget::new(limits);
        for node in &self.nodes {
            context.check()?;
            if node.relative == selection.relative() {
                if node.content.directory() != (selection.kind() == NodeKind::Directory) {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                found = true;
            }
            if let Ok(relative) = node.relative.strip_prefix(selection.relative()) {
                budget.add(relative, node.content.bytes())?;
            }
        }
        if !found {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(())
    }

    /// Rebase one validated output subtree without copying file contents.
    pub fn selected(
        &self,
        selection: &super::PayloadSelection,
        context: &SnapshotContext,
    ) -> io::Result<Self> {
        context.check()?;
        let root = self
            .nodes
            .iter()
            .find(|node| node.relative == selection.relative())
            .ok_or(io::ErrorKind::NotFound)?;
        if root.content.directory() != (selection.kind() == NodeKind::Directory) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let policy = PayloadPolicy::new(selection.kind(), PayloadMode(root.mode), self.policy.copy);
        let mut output = BTreeMap::new();
        for node in &self.nodes {
            context.check()?;
            if let Ok(relative) = node.relative.strip_prefix(selection.relative()) {
                let mut selected = node.clone();
                selected.relative = relative.to_owned();
                output.insert(selected.relative.clone(), selected);
            }
        }
        Ok(Self {
            source: self.source.clone(),
            nodes: validate_tree(output, &policy)?,
            policy,
        })
    }

    pub fn new(
        snapshot: SourceSnapshot,
        overlays: impl IntoIterator<Item = PayloadOverlay>,
        policy: &PayloadPolicy,
        context: &SnapshotContext,
    ) -> io::Result<Self> {
        context.check()?;
        let mut replacements = BTreeMap::new();
        for overlay in overlays {
            context.check()?;
            let (index, original) = snapshot
                .plan()
                .node(&overlay.original)
                .ok_or(io::ErrorKind::NotFound)?;
            if original.content.directory() {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            if replacements.contains_key(&overlay.original) {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            Budget::new(policy.copy).add(&overlay.output, overlay.bytes.len() as u64)?;
            let content = inline_content(&overlay.bytes, context)?;
            replacements.insert(
                overlay.original,
                Node {
                    relative: overlay.output,
                    mode: overlay.mode.0,
                    content,
                    data: Data::Inline {
                        bytes: Arc::new(overlay.bytes),
                        original: Some(index),
                    },
                },
            );
        }
        let mut budget = Budget::new(policy.copy);
        let mut output = BTreeMap::new();
        if policy.kind == NodeKind::Directory && snapshot.kind() == NodeKind::File {
            insert(
                &mut output,
                &mut budget,
                Node {
                    relative: PathBuf::new(),
                    mode: policy.root_mode.0,
                    content: Content::Directory,
                    data: Data::Directory,
                },
            )?;
        }
        for (index, node) in snapshot.plan().nodes().enumerate() {
            context.check()?;
            if node.relative.0.as_os_str().is_empty()
                && node.content.directory()
                && policy.kind == NodeKind::File
            {
                continue;
            }
            let mut planned = replacements
                .remove(&node.relative.0)
                .unwrap_or_else(|| Node {
                    relative: node.relative.0.clone(),
                    mode: node.mode,
                    content: node.content.clone(),
                    data: if node.content.directory() {
                        Data::Directory
                    } else {
                        Data::Source(index)
                    },
                });
            if planned.relative.as_os_str().is_empty() {
                // Two explicit inline/root modes may not disagree.
                if matches!(planned.data, Data::Inline { .. }) && planned.mode != policy.root_mode.0
                {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                planned.mode = policy.root_mode.0;
            }
            insert(&mut output, &mut budget, planned)?;
        }
        let nodes = validate_tree(output, policy)?;
        context.check()?;
        Ok(Self {
            source: Some(snapshot),
            nodes,
            policy: *policy,
        })
    }

    pub fn generated(
        nodes: impl IntoIterator<Item = GeneratedNode>,
        policy: &PayloadPolicy,
        context: &SnapshotContext,
    ) -> io::Result<Self> {
        context.check()?;
        let mut budget = Budget::new(policy.copy);
        let mut output = BTreeMap::new();
        for node in nodes {
            context.check()?;
            budget.add(
                &node.relative,
                node.bytes.as_ref().map_or(0, |bytes| bytes.len() as u64),
            )?;
            let (content, data) = match node.bytes {
                None => (Content::Directory, Data::Directory),
                Some(bytes) => (
                    inline_content(&bytes, context)?,
                    Data::Inline {
                        bytes: Arc::new(bytes),
                        original: None,
                    },
                ),
            };
            insert_prechecked(
                &mut output,
                Node {
                    relative: node.relative,
                    mode: node.mode.0,
                    content,
                    data,
                },
            )?;
        }
        let nodes = validate_tree(output, policy)?;
        context.check()?;
        Ok(Self {
            source: None,
            nodes,
            policy: *policy,
        })
    }

    pub fn kind(&self) -> NodeKind {
        self.policy.kind
    }
    pub fn fingerprint(&self) -> [u8; 32] {
        tree::fingerprint(
            self.nodes
                .iter()
                .map(|n| (n.relative.as_path(), n.mode, &n.content)),
        )
    }
    pub fn nodes(&self) -> impl ExactSizeIterator<Item = SnapshotNode<'_>> {
        self.nodes.iter().map(|n| SnapshotNode {
            relative: &n.relative,
            mode: PayloadMode(n.mode),
            content: &n.content,
            has_payload_digest: true,
        })
    }
    fn recheck_source(&self, context: &SnapshotContext) -> io::Result<()> {
        context.check()?;
        if let Some(snapshot) = &self.source {
            snapshot.revalidate_content(context)?;
        }
        context.check()
    }
    pub fn revalidate_content(&self, context: &SnapshotContext) -> io::Result<()> {
        context.check()?;
        if let Some(source) = &self.source {
            source.revalidate_content(context)?;
        }
        context.check()
    }
}

impl tree::BuildTree for ValidatedPayloadPlan {
    fn build_tree(
        &self,
        root: &Path,
        _cancellation: &crate::file_lock::Cancellation,
        context: Option<&SnapshotContext>,
        created: &mut TreeReceipt,
        after_effect: &mut dyn FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        let context = context.ok_or(io::ErrorKind::InvalidInput)?;
        context.check()?;
        if !created.nodes.is_empty() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.recheck_source(context)?;
        context.check()?;
        let mut builder = TreeBuilder::new(
            root,
            context.cancellation(),
            Some(context),
            created,
            after_effect,
        );
        let mut buffer = vec![0; tree::BUFFER_BYTES];
        for node in &self.nodes {
            builder.check()?;
            match &node.data {
                Data::Directory => builder.directory(&node.relative)?,
                Data::Source(index) => {
                    let source = self
                        .source
                        .as_ref()
                        .ok_or(io::ErrorKind::InvalidData)?
                        .plan();
                    let original = source.indexed_node(*index)?;
                    let mut input = source.open_verified(original)?;
                    let mut output = builder.file(&node.relative)?;
                    let mut digest = Sha256::new();
                    let mut written = 0u64;
                    let observed = context.consume_file(
                        &mut input,
                        original.content.bytes(),
                        &mut buffer,
                        |chunk| builder.write(&mut output, &mut digest, &mut written, chunk),
                    )?;
                    if (Content::File {
                        bytes: written,
                        digest: observed,
                    }) != node.content
                    {
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    source.verify_handle(original, &input)?;
                    output.sync_all()?;
                }
                Data::Inline { bytes, original } => {
                    if let Some(index) = original {
                        let source = self
                            .source
                            .as_ref()
                            .ok_or(io::ErrorKind::InvalidData)?
                            .plan();
                        let original = source.indexed_node(*index)?;
                        let mut input = source.open_verified(original)?;
                        let observed = context.consume_file(
                            &mut input,
                            original.content.bytes(),
                            &mut buffer,
                            |_| Ok(()),
                        )?;
                        if (Content::File {
                            bytes: original.content.bytes(),
                            digest: observed,
                        }) != original.content
                        {
                            return Err(io::ErrorKind::InvalidData.into());
                        }
                        source.verify_handle(original, &input)?;
                    }
                    let mut output = builder.file(&node.relative)?;
                    let mut digest = Sha256::new();
                    let mut written = 0u64;
                    for chunk in bytes.chunks(tree::BUFFER_BYTES) {
                        builder.write(&mut output, &mut digest, &mut written, chunk)?;
                    }
                    if (Content::File {
                        bytes: written,
                        digest: digest.finalize().into(),
                    }) != node.content
                    {
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    output.sync_all()?;
                }
            }
        }
        builder.finish(self.nodes.iter().map(|n| n.mode))?;
        self.recheck_source(context)
    }
}

fn inline_content(bytes: &[u8], context: &SnapshotContext) -> io::Result<Content> {
    let mut digest = Sha256::new();
    for chunk in bytes.chunks(tree::BUFFER_BYTES) {
        context.check()?;
        digest.update(chunk);
    }
    context.check()?;
    Ok(Content::File {
        bytes: bytes.len() as u64,
        digest: digest.finalize().into(),
    })
}
fn insert(output: &mut BTreeMap<PathBuf, Node>, budget: &mut Budget, node: Node) -> io::Result<()> {
    budget.add(&node.relative, node.content.bytes())?;
    insert_prechecked(output, node)
}
fn insert_prechecked(output: &mut BTreeMap<PathBuf, Node>, node: Node) -> io::Result<()> {
    validate_relative(&node.relative)?;
    node.mode.validate()?;
    if output.contains_key(&node.relative) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    output.insert(node.relative.clone(), node);
    Ok(())
}
fn validate_tree(output: BTreeMap<PathBuf, Node>, policy: &PayloadPolicy) -> io::Result<Vec<Node>> {
    let root = output
        .get(Path::new(""))
        .ok_or(io::ErrorKind::InvalidInput)?;
    if root.mode != policy.root_mode.0
        || root.content.directory() != (policy.kind == NodeKind::Directory)
        || (policy.kind == NodeKind::File && output.len() != 1)
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    for node in output.values() {
        if node.relative.as_os_str().is_empty() {
            continue;
        }
        let parent = node.relative.parent().ok_or(io::ErrorKind::InvalidInput)?;
        if !output.get(parent).is_some_and(|n| n.content.directory()) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
    }
    Ok(output.into_values().collect())
}

#[cfg(test)]
mod tests;
