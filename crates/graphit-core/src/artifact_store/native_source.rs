//! Native scope capture backing, shared by Project and Installation readers.
//! Scope holds the native root from birth. Records remain observations only.
use super::{
    CapturedPublicationScope, CopyPolicy, SnapshotContext, SnapshotPolicy,
    snapshot::{RetainedSources, path_units},
    tree::{self, Content, SourceNode, SourcePlan},
};
use crate::file_io::native_name_bytes;
use graphit_fs::{NodeKind, read_directory};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
    sync::{Arc, RwLock, RwLockWriteGuard},
};

pub(crate) mod chain;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub(super) use chain::NativeControlChain;
mod selection;
pub use selection::{
    ProjectIgnoreCriteria, ProjectSelectionCriteria, ProjectSelectionLimits,
    selected_project_changed_paths, selected_project_paths,
};

pub(crate) struct NativeHandle {
    pub(super) file: Arc<RwLock<File>>,
}
impl NativeHandle {
    fn open_root(
        path: &Path,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Arc<Self>> {
        crate::native_path::valid_path(path)?;
        let file = read_directory::open_root_readonly(path, &mut || request.check())?;
        read_directory::inspect(&file, Some(NodeKind::Directory), &mut || request.check())?;
        request.check()?;
        Ok(Arc::new(Self {
            file: Arc::new(RwLock::new(file)),
        }))
    }
    fn open_child(
        parent: &File,
        name: &std::ffi::OsStr,
        kind: Option<NodeKind>,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Arc<Self>> {
        let file = if kind == Some(NodeKind::File) {
            read_directory::open_relative_source_readonly(
                parent,
                name,
                tree::BUFFER_BYTES,
                &mut || request.check(),
            )?
        } else {
            read_directory::open_relative_readonly(
                parent,
                name,
                kind,
                tree::BUFFER_BYTES,
                &mut || request.check(),
            )?
        };
        request.check()?;
        Ok(Arc::new(Self {
            file: Arc::new(RwLock::new(file)),
        }))
    }
    // A short-lived payload read keeps the existing strict file sharing profile.
    fn open_read_child(
        parent: &File,
        name: &std::ffi::OsStr,
        request: &Arc<SnapshotContext>,
    ) -> io::Result<Arc<Self>> {
        let file = read_directory::open_relative_readonly(
            parent,
            name,
            Some(NodeKind::File),
            tree::BUFFER_BYTES,
            &mut || request.check(),
        )?;
        request.check()?;
        Ok(Arc::new(Self {
            file: Arc::new(RwLock::new(file)),
        }))
    }
    pub(crate) fn lock(&self) -> io::Result<RwLockWriteGuard<'_, File>> {
        self.file
            .write()
            .map_err(|_| io::Error::other("native handle lock poisoned"))
    }
    fn observe(
        &self,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<tree::Observation> {
        let file = self.lock()?;
        request.check()?;
        tree::Observation::from_file(&file)
    }
}

pub(super) struct NativeBirthAnchor {
    root: Arc<NativeHandle>,
    // Windows holds no-delete ancestor handles, preventing namespace detach.
    _ancestors: Vec<Arc<NativeHandle>>,
    identity: tree::PhysicalId,
}
impl NativeBirthAnchor {
    pub(super) fn capture(
        root: &Path,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
        max_path_bytes: usize,
        max_components: usize,
    ) -> io::Result<Arc<Self>> {
        request.check()?;
        if !root.is_absolute() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if native_name_bytes(root.as_os_str())? > max_path_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        request.check()?;
        let count = root.components().count();
        if count > max_components {
            return Err(io::ErrorKind::FileTooLarge.into());
        }

        #[cfg(windows)]
        let mut ancestors = Vec::new();
        #[cfg(not(windows))]
        let ancestors = Vec::new();
        #[cfg(windows)]
        {
            ancestors
                .try_reserve_exact(count)
                .map_err(|_| io::ErrorKind::OutOfMemory)?;
            for ancestor in root.ancestors().skip(1) {
                request.check()?;
                ancestors.push(NativeHandle::open_root(ancestor, request)?);
            }
        }
        let handle = NativeHandle::open_root(root, request)?;
        let identity = {
            let file = handle.lock()?;
            request.check()?;
            tree::handle_id(&file)?
        };
        identity.validate()?;
        request.check()?;
        Ok(Arc::new(Self {
            root: handle,
            _ancestors: ancestors,
            identity,
        }))
    }
    pub(super) fn matches_receipt(
        &self,
        receipt: &tree::NodeReceipt,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<()> {
        request.check()?;
        let file = self.root.lock()?;
        read_directory::inspect(&file, Some(NodeKind::Directory), &mut || request.check())?;
        request.check()?;
        if receipt.id != self.identity || tree::handle_id(&file)? != self.identity {
            return Err(io::ErrorKind::InvalidData.into());
        }
        request.check()
    }
}

/// Shared root, scope and operation retained by readers and their opened files.
pub(super) struct RootReadAdmission {
    anchor: Arc<NativeBirthAnchor>,
    scope: CapturedPublicationScope,
    request: std::sync::Arc<crate::file_io::SnapshotContext>,
}

/// Birth missing layout parents through the retained storage anchor. This
/// ordinary writer keeps its existing ScopeContext; it creates no Original.
pub(super) fn ensure_control_parent(
    scope: &CapturedPublicationScope,
    context: &super::ScopeContext,
) -> io::Result<()> {
    scope.revalidate(context)?;
    let parent_path = scope
        .runtime_root()
        .parent()
        .ok_or(io::ErrorKind::InvalidInput)?;
    let relative = parent_path
        .strip_prefix(scope.runtime_anchor_root())
        .map_err(|_| io::ErrorKind::InvalidInput)?;
    crate::file_io::validate_relative(relative, true)?;
    let mut parent = Arc::clone(&scope.runtime_anchor().root);
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(io::ErrorKind::InvalidInput.into());
        };
        scope.revalidate(context)?;
        let file = {
            let held = parent.lock()?;
            let open = || {
                read_directory::open_relative_readonly(
                    &held,
                    name,
                    Some(NodeKind::Directory),
                    tree::BUFFER_BYTES,
                    &mut || context.check(),
                )
            };
            match open() {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    match graphit_fs::write_relative::create_private_directory_relative(
                        &held,
                        name,
                        tree::BUFFER_BYTES,
                        &mut || context.check(),
                    ) {
                        Ok(file) => file,
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => open()?,
                        Err(error) => return Err(error),
                    }
                }
                Err(error) => return Err(error),
            }
        };
        parent = Arc::new(NativeHandle {
            file: Arc::new(RwLock::new(file)),
        });
        scope.revalidate(context)?;
    }
    scope.revalidate(context)
}
impl RootReadAdmission {
    pub(super) fn runtime(
        scope: &CapturedPublicationScope,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Self> {
        request.check()?;
        let value = Self {
            anchor: Arc::clone(scope.runtime_anchor()),
            scope: scope.clone(),
            request: request.clone(),
        };
        value.revalidate()?;
        Ok(value)
    }
    pub(super) fn new(
        scope: &CapturedPublicationScope,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Self> {
        request.check()?;
        let anchor = scope.native_anchor();

        let value = Self {
            anchor: Arc::clone(anchor),
            scope: scope.clone(),
            request: request.clone(),
        };
        value.revalidate()?;
        Ok(value)
    }
    fn revalidate(&self) -> io::Result<()> {
        self.request.check()?;
        self.scope.revalidate_native(&self.request)?;
        self.request.check()
    }
}
struct PrefixHandle {
    handle: Arc<NativeHandle>,
    observation: tree::Observation,
}
pub(super) struct NativeSource {
    selection: PathBuf,
    prefixes: Vec<PrefixHandle>,
    // Only the selected root is a content observation; its ancestors bind the namespace.
    ancestor_bindings: Vec<chain::DirectoryBinding>,
    kind: NodeKind,
    directories: DirectoryLatches,
    selected: Option<selection::SelectedBacking>,
    // Root admission is last: descendants close before the original root.
    admission: Arc<RootReadAdmission>,
}
// The leaf descriptor drops first, then ancestors drain deepest-to-root.
struct DirectoryLatch {
    handle: Arc<NativeHandle>,
    observation: tree::Observation,
}
struct DirectoryLatches(Vec<DirectoryLatch>);
impl Drop for DirectoryLatches {
    fn drop(&mut self) {
        while let Some(latch) = self.0.pop() {
            drop(latch);
        }
    }
}
struct AncestorHandles(Vec<Arc<NativeHandle>>);
impl Drop for AncestorHandles {
    fn drop(&mut self) {
        while let Some(handle) = self.0.pop() {
            drop(handle);
        }
    }
}
pub(super) struct NativeOpenedFile {
    pub(super) handle: Arc<NativeHandle>,
    _ancestors: AncestorHandles,
    _source: Arc<NativeSource>,
}
// Owns the freshly opened leaf and its held-root chain for one operation.
#[cfg(windows)]
pub(super) struct NativeReadFile {
    pub(super) file: File,
    _ancestors: AncestorHandles,
    _source: Arc<NativeSource>,
}
#[cfg(windows)]
impl NativeOpenedFile {
    pub(super) fn into_read_file(self) -> io::Result<NativeReadFile> {
        let Self {
            handle,
            _ancestors,
            _source,
        } = self;
        let handle = Arc::try_unwrap(handle)
            .map_err(|_| io::Error::other("active source handle unexpectedly shared"))?;
        let lock = Arc::try_unwrap(handle.file)
            .map_err(|_| io::Error::other("active source file unexpectedly shared"))?;
        let file = lock
            .into_inner()
            .map_err(|_| io::Error::other("source reader lock poisoned"))?;
        Ok(NativeReadFile {
            file,
            _ancestors,
            _source,
        })
    }
}
impl NativeSource {
    pub(super) fn capture_selected_changes_metadata(
        scope: &CapturedPublicationScope,
        criteria: ProjectSelectionCriteria<'_>,
        changes: &std::collections::BTreeSet<PathBuf>,
        limits: ProjectSelectionLimits,
        request: &Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<super::SourceSnapshot> {
        scope.require_project()?;
        selection::capture_changes_metadata(scope, criteria, changes, limits, request)
    }
    pub(super) fn covers_selected_path(&self, path: &Path) -> io::Result<bool> {
        selection::covers(self, path)
    }
    pub(super) fn capture_selected_metadata(
        scope: &CapturedPublicationScope,
        criteria: ProjectSelectionCriteria<'_>,
        limits: ProjectSelectionLimits,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<super::SourceSnapshot> {
        scope.require_project()?;
        selection::capture_metadata(scope, criteria, limits, request)
    }
    pub(super) fn capture_selected(
        scope: &CapturedPublicationScope,
        criteria: ProjectSelectionCriteria<'_>,
        limits: ProjectSelectionLimits,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<super::SourceSnapshot> {
        scope.require_project()?;
        selection::capture(scope, criteria, limits, request)
    }
    pub(super) fn capture(
        scope: &CapturedPublicationScope,
        relative: &Path,
        kind: NodeKind,
        policy: &SnapshotPolicy,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<(Arc<Self>, SourcePlan, RetainedSources)> {
        Self::capture_inner(scope, relative, kind, policy, request, None, None)
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(super) fn capture_verified_copy(
        scope: &CapturedPublicationScope,
        relative: &Path,
        kind: NodeKind,
        policy: &SnapshotPolicy,
        request: &Arc<crate::file_io::SnapshotContext>,
        receipt: &tree::TreeReceipt,
    ) -> io::Result<super::SourceSnapshot> {
        // Copy receipts retain metadata, never bodies. A retained-body request
        // uses the ordinary capture API rather than pretending bytes were read.
        if !policy.selected.paths.is_empty() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let (native, plan, retained) =
            Self::capture_inner(scope, relative, kind, policy, request, None, Some(receipt))?;
        super::SourceSnapshot::from_native_parts(
            native,
            plan,
            retained,
            kind,
            super::snapshot::CapturePolicy::Generic(policy.clone()),
        )
    }
    fn capture_inner(
        scope: &CapturedPublicationScope,
        relative: &Path,
        kind: NodeKind,
        policy: &SnapshotPolicy,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
        existing: Option<Arc<RootReadAdmission>>,
        verified: Option<&tree::TreeReceipt>,
    ) -> io::Result<(Arc<Self>, SourcePlan, RetainedSources)> {
        request.check()?;
        crate::native_path::valid_native(relative)?;
        crate::file_io::validate_relative(relative, false)?;
        if policy.links != CopyPolicy::RejectFileLinks {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if policy.selected.paths.len() > policy.retention.files {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let mut selected_units = 0usize;
        for path in &policy.selected.paths {
            crate::file_io::validate_relative(path, true)?;
            selected_units = selected_units
                .checked_add(path_units(path)?)
                .ok_or(io::ErrorKind::FileTooLarge)?;
            if selected_units > policy.retention.path_bytes {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            if path.as_os_str().is_empty() && kind == NodeKind::Directory {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        }
        let admission = match existing {
            Some(admission) => {
                if !admission.request.same_request(request) {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                admission.revalidate()?;
                admission
            }
            None => Arc::new(RootReadAdmission::new(scope, request)?),
        };
        #[cfg(test)]
        capture_step(CaptureStep::AfterAdmission);
        let selection = relative.to_owned();

        let prefixes = chain::open_prefixes(&admission, relative, kind)?;
        let (_, ancestors) = prefixes.split_last().ok_or(io::ErrorKind::InvalidInput)?;
        let ancestor_bindings = ancestors
            .iter()
            .map(|prefix| chain::DirectoryBinding::observe(&prefix.handle, request))
            .collect::<io::Result<Vec<_>>>()?;
        let mut source = Arc::new(Self {
            admission,
            selection,
            prefixes,
            ancestor_bindings,
            kind,
            selected: None,
            directories: DirectoryLatches(Vec::new()),
        });
        // Validate links, paths, tree limits and selected files before payload reads.
        let mut capture = Capture {
            source: &source,
            policy,
            verified,
            budget: tree::Budget::new(policy.copy),
            nodes: Vec::new(),
            directories: DirectoryLatches(Vec::new()),
            retained_bytes: 0,
        };
        capture.walk(Path::new(""), Arc::clone(source.selected_handle()?), 0)?;
        source.check_prefixes(None)?;
        source.check_fresh_prefixes()?;
        source.admission.revalidate()?;
        capture.nodes.sort_by(|a, b| a.relative.cmp(&b.relative));
        if capture.nodes.is_empty() {
            return Err(io::ErrorKind::NotFound.into());
        }
        let display = scope.owner_root().join(relative);
        let Capture {
            nodes, directories, ..
        } = capture;
        let mut plan = SourcePlan::from_native_nodes(display, nodes);
        if let Some(receipt) = verified {
            if receipt.nodes.len() != plan.nodes().len()
                || !plan
                    .nodes()
                    .zip(&receipt.nodes)
                    .all(|(node, receipt)| node.matches_verified_receipt(receipt))
            {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            for (index, node) in receipt.nodes.iter().enumerate() {
                if let Content::File { digest, .. } = &node.content {
                    plan.set_native_digest(index, *digest)?;
                }
            }
        }
        Arc::get_mut(&mut source)
            .ok_or(io::ErrorKind::InvalidData)?
            .directories = directories;
        source.check_directories()?;
        let mut total = 0u64;
        for node in plan.nodes() {
            total = total
                .checked_add(node.content.bytes())
                .ok_or(io::ErrorKind::FileTooLarge)?;
        }
        let mut retained = BTreeMap::new();
        for path in &policy.selected.paths {
            let (_, node) = plan.node(path).ok_or(io::ErrorKind::NotFound)?;
            if node.content.directory() {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let length =
                usize::try_from(node.content.bytes()).map_err(|_| io::ErrorKind::FileTooLarge)?;
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(length)
                .map_err(|_| io::ErrorKind::OutOfMemory)?;
            retained.insert(path.to_owned(), bytes);
        }

        // Validate every captured file binding before reading payload bytes.
        for (index, node) in plan.nodes().enumerate() {
            if !node.content.directory() {
                let opened = source.open_file(&node.relative.0, &plan)?;
                let file = opened.handle.lock()?;
                source.check_binding(index, Some(&file), &plan, request)?;
            }
        }
        request.check()?;
        // Phase two reads each captured file exactly once; the same file/ranges
        // reader seam performs the native identity/path checks.
        if verified.is_none() {
            let mut buffer = Vec::new();
            buffer
                .try_reserve_exact(tree::BUFFER_BYTES)
                .map_err(|_| io::ErrorKind::OutOfMemory)?;
            buffer.resize(tree::BUFFER_BYTES, 0);
            let node_count = plan.nodes().len();
            for index in 0..node_count {
                let node = plan.indexed_node(index)?;
                if node.content.directory() {
                    continue;
                }
                let opened = source.open_file(&node.relative.0, &plan)?;
                let mut file = opened.handle.lock()?;
                source.check_binding(index, Some(&file), &plan, request)?;
                let mut bytes = retained.get_mut(&node.relative.0);
                request.check()?;
                file.seek(SeekFrom::Start(0))?;
                let mut left = node.content.bytes();
                let mut digest = Sha256::new();
                while left != 0 {
                    request.check()?;
                    let wanted = usize::try_from(left.min(buffer.len() as u64))
                        .map_err(|_| io::ErrorKind::FileTooLarge)?;
                    let read = file.read(&mut buffer[..wanted])?;
                    if read == 0 {
                        return Err(io::ErrorKind::UnexpectedEof.into());
                    }
                    #[cfg(test)]
                    capture_step(CaptureStep::AfterPayloadRead);
                    digest.update(&buffer[..read]);
                    if let Some(bytes) = &mut bytes {
                        bytes.extend_from_slice(&buffer[..read]);
                    }
                    left -= read as u64;
                }
                source.check_binding(index, Some(&file), &plan, request)?;
                drop(file);
                drop(opened);
                plan.set_native_digest(index, digest.finalize().into())?;
            }
        }
        // No snapshot escapes without a final fence of every captured file and
        // all retained directory objects, even files hashed earlier in the plan.
        for (index, node) in plan.nodes().enumerate() {
            if !node.content.directory() {
                let opened = source.open_file(&node.relative.0, &plan)?;
                let file = opened.handle.lock()?;
                source.check_binding(index, Some(&file), &plan, request)?;
            }
        }
        source.check_directories()?;
        source.admission.revalidate()?;
        request.check()?;
        Ok((source, plan, retained))
    }
    pub(super) fn recapture_selected(&self) -> io::Result<super::SourceSnapshot> {
        selection::recapture(self)
    }
    pub(super) fn recapture(&self, policy: &SnapshotPolicy) -> io::Result<super::SourceSnapshot> {
        let (native, plan, retained) = Self::capture_inner(
            &self.admission.scope,
            &self.selection,
            self.kind,
            policy,
            self.request(),
            Some(Arc::clone(&self.admission)),
            None,
        )?;
        super::SourceSnapshot::from_native_parts(
            native,
            plan,
            retained,
            self.kind,
            super::snapshot::CapturePolicy::Generic(policy.clone()),
        )
    }
    fn selected_handle(&self) -> io::Result<&Arc<NativeHandle>> {
        self.prefixes
            .last()
            .map(|prefix| &prefix.handle)
            .or_else(|| {
                if self.selection.as_os_str().is_empty() && self.kind == NodeKind::Directory {
                    Some(&self.admission.anchor.root)
                } else {
                    None
                }
            })
            .ok_or_else(|| io::ErrorKind::InvalidData.into())
    }
    pub(super) fn request(&self) -> &std::sync::Arc<crate::file_io::SnapshotContext> {
        &self.admission.request
    }
    pub(super) fn validate_suffix(&self, path: &Path) -> io::Result<()> {
        crate::file_io::validate_relative(path, true)
    }
    fn check_directories(&self) -> io::Result<()> {
        for latch in &self.directories.0 {
            if latch.handle.observe(self.request())? != latch.observation {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        self.request().check()
    }
    fn check_prefixes(&self, root_file: Option<&File>) -> io::Result<()> {
        self.request().check()?;
        for (index, prefix) in self.prefixes.iter().enumerate() {
            if let Some(binding) = self.ancestor_bindings.get(index) {
                if chain::DirectoryBinding::observe(&prefix.handle, self.request())? != *binding {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                continue;
            }
            let observed = if index + 1 == self.prefixes.len() && self.kind == NodeKind::File {
                if let Some(file) = root_file {
                    self.request().check()?;
                    tree::Observation::from_file(file)?
                } else {
                    prefix.handle.observe(self.request())?
                }
            } else {
                prefix.handle.observe(self.request())?
            };
            if observed != prefix.observation {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        self.request().check()
    }
    fn check_fresh_prefixes(&self) -> io::Result<()> {
        let mut parent = Arc::clone(&self.admission.anchor.root);
        for (index, (component, captured)) in
            self.selection.components().zip(&self.prefixes).enumerate()
        {
            let Component::Normal(name) = component else {
                return Err(io::ErrorKind::InvalidData.into());
            };
            let next = {
                let file = parent.lock()?;
                NativeHandle::open_child(&file, name, None, self.request())?
            };
            if let Some(binding) = self.ancestor_bindings.get(index) {
                if chain::DirectoryBinding::observe(&next, self.request())? != *binding {
                    return Err(io::ErrorKind::InvalidData.into());
                }
            } else if next.observe(self.request())? != captured.observation {
                return Err(io::ErrorKind::InvalidData.into());
            }
            parent = next;
        }
        self.request().check()
    }
    pub(super) fn open_file(
        self: &Arc<Self>,
        relative: &Path,
        plan: &SourcePlan,
    ) -> io::Result<NativeOpenedFile> {
        self.open_file_with_profile(relative, plan, false)
    }
    pub(super) fn open_file_shared(
        self: &Arc<Self>,
        relative: &Path,
        plan: &SourcePlan,
    ) -> io::Result<NativeOpenedFile> {
        self.open_file_with_profile(relative, plan, true)
    }
    fn open_file_with_profile(
        self: &Arc<Self>,
        relative: &Path,
        plan: &SourcePlan,
        idle: bool,
    ) -> io::Result<NativeOpenedFile> {
        self.admission.revalidate()?;
        self.validate_suffix(relative)?;
        let (_, node) = plan.node(relative).ok_or(io::ErrorKind::NotFound)?;
        if node.content.directory() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if relative.as_os_str().is_empty() {
            if self.kind != NodeKind::File {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let handle = if idle {
                Arc::clone(self.selected_handle()?)
            } else {
                let parent = if self.prefixes.len() > 1 {
                    &self.prefixes[self.prefixes.len() - 2].handle
                } else {
                    &self.admission.anchor.root
                };
                let name = self
                    .selection
                    .file_name()
                    .ok_or(io::ErrorKind::InvalidData)?;
                let file = parent.lock()?;
                NativeHandle::open_read_child(&file, name, self.request())?
            };
            self.request().check()?;
            {
                let file = handle.lock()?;
                if !node.matches_native_file(&file)? {
                    return Err(io::ErrorKind::InvalidData.into());
                }
            }
            return Ok(NativeOpenedFile {
                handle,
                _ancestors: AncestorHandles(Vec::new()),
                _source: Arc::clone(self),
            });
        }
        let mut ancestors = AncestorHandles(Vec::new());
        ancestors
            .0
            .try_reserve_exact(relative.components().count())
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        let mut parent = Arc::clone(self.selected_handle()?);
        let mut prefix = PathBuf::new();
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(io::ErrorKind::InvalidInput.into());
            };
            prefix.push(name);
            self.validate_suffix(&prefix)?;
            let (_, expected) = plan.node(&prefix).ok_or(io::ErrorKind::InvalidData)?;
            let kind = if expected.content.directory() {
                NodeKind::Directory
            } else {
                NodeKind::File
            };
            let next = {
                let file = parent.lock()?;
                if kind == NodeKind::File && !idle {
                    NativeHandle::open_read_child(&file, name, self.request())?
                } else {
                    NativeHandle::open_child(&file, name, Some(kind), self.request())?
                }
            };
            let file = next.lock()?;
            self.request().check()?;
            let matches = if self.selected.is_some() && kind == NodeKind::Directory {
                expected.matches_native_directory(&file)?
            } else {
                expected.matches_native_file(&file)?
            };
            if !matches {
                return Err(io::ErrorKind::InvalidData.into());
            }
            drop(file);
            ancestors.0.push(parent);
            parent = next;
        }
        Ok(NativeOpenedFile {
            handle: parent,
            _ancestors: ancestors,
            _source: Arc::clone(self),
        })
    }
    pub(super) fn check_binding(
        self: &Arc<Self>,
        index: usize,
        file: Option<&File>,
        plan: &SourcePlan,
        context: &SnapshotContext,
    ) -> io::Result<()> {
        context.check()?;
        if !context.same_request(self.request()) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.admission.revalidate()?;
        self.check_directories()?;
        let node = plan.indexed_node(index)?;
        self.validate_suffix(&node.relative.0)?;
        selection::check_binding(self, &node.relative.0, node.content.directory())?;
        self.check_prefixes(if node.relative.0.as_os_str().is_empty() {
            file
        } else {
            None
        })?;
        self.check_fresh_prefixes()?;
        if let Some(file) = file {
            read_directory::inspect(file, Some(NodeKind::File), &mut || self.request().check())?;
            self.request().check()?;
            if !node.matches_native_file(file)? {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        // Fresh relative opens verify each captured pathname and ancestor.
        // Selected snapshots validate receipts and retain the active IO chain;
        // generic tree captures also retain their original directory handles.
        if !node.content.directory() && !node.relative.0.as_os_str().is_empty() {
            let fresh = self.open_file(&node.relative.0, plan)?;
            let observed = fresh.handle.lock()?;
            self.request().check()?;
            if !node.matches_native_file(&observed)? {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        self.request().check()
    }
    pub(super) fn revalidate_selection(&self) -> io::Result<()> {
        selection::check(self)
    }
}

struct Capture<'a> {
    source: &'a NativeSource,
    policy: &'a SnapshotPolicy,
    verified: Option<&'a tree::TreeReceipt>,
    budget: tree::Budget,
    nodes: Vec<SourceNode>,
    directories: DirectoryLatches,
    retained_bytes: usize,
}
impl Capture<'_> {
    fn walk(&mut self, relative: &Path, handle: Arc<NativeHandle>, depth: usize) -> io::Result<()> {
        let request = self.source.request();
        crate::native_path::valid_native(relative)?;
        crate::file_io::validate_relative(relative, true)?;
        if depth > self.policy.copy.max_depth {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let file = handle.lock()?;
        let observed = read_directory::inspect(&file, None, &mut || request.check())?;
        self.budget.add(
            relative,
            // A verified Copy observes metadata without reading payload. Keep
            // its physical lengths in SourceNode, but charge only bytes read.
            if observed.is_file() && self.verified.is_none() {
                observed.len()
            } else {
                0
            },
        )?;

        let retain = self.policy.selected.paths.contains(relative);
        if retain && !observed.is_file() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if retain {
            self.retained_bytes = self
                .retained_bytes
                .checked_add(
                    usize::try_from(observed.len()).map_err(|_| io::ErrorKind::FileTooLarge)?,
                )
                .ok_or(io::ErrorKind::FileTooLarge)?;
            if self.retained_bytes > self.policy.retention.bytes {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
        }
        let content = if observed.is_dir() {
            Content::Directory
        } else {
            Content::File {
                bytes: observed.len(),
                digest: [0; 32],
            }
        };
        request.check()?;
        let node = SourceNode::from_native_file(relative, content, &file)?;
        self.nodes
            .try_reserve(1)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        self.nodes.push(node);
        if observed.is_dir() {
            request.check()?;
            let observation = tree::Observation::from_file(&file)?;
            self.directories
                .0
                .try_reserve(1)
                .map_err(|_| io::ErrorKind::OutOfMemory)?;
            self.directories.0.push(DirectoryLatch {
                handle: Arc::clone(&handle),
                observation,
            });

            let mut entries =
                read_directory::DirectoryEntries::new(&file, tree::BUFFER_BYTES, &mut || {
                    request.check()
                })?;
            drop(file);
            loop {
                let name = entries.next_name(tree::BUFFER_BYTES, &mut || request.check())?;
                let Some(name) = name else {
                    break;
                };

                let child_path = relative.join(&name);
                crate::native_path::valid_native(&child_path)?;
                crate::file_io::validate_relative(&child_path, false)?;
                let child = {
                    let file = handle.lock()?;
                    NativeHandle::open_child(&file, &name, None, request)?
                };
                self.walk(
                    &child_path,
                    child,
                    depth.checked_add(1).ok_or(io::ErrorKind::FileTooLarge)?,
                )?;
            }
            drop(entries);
        }
        request.check()
    }
}

impl Drop for NativeSource {
    fn drop(&mut self) {
        // The selected handle has one retained owner in the last prefix. Close
        // it before its parent, then drain toward the birth root; admission is last.
        while let Some(latch) = self.directories.0.pop() {
            drop(latch);
        }
        while let Some(prefix) = self.prefixes.pop() {
            drop(prefix);
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum CaptureStep {
    AfterAdmission,
    AfterPayloadRead,
}
#[cfg(test)]
type CaptureHook = (CaptureStep, Box<dyn FnOnce()>);
#[cfg(test)]
thread_local! {
    static CAPTURE_STEP: std::cell::RefCell<Option<CaptureHook>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
fn capture_step(step: CaptureStep) {
    CAPTURE_STEP.with(|slot| {
        let action = {
            let mut slot = slot.borrow_mut();
            if slot.as_ref().is_some_and(|(expected, _)| *expected == step) {
                slot.take().map(|(_, action)| action)
            } else {
                None
            }
        };
        if let Some(action) = action {
            action();
        }
    });
}

#[cfg(test)]
mod tests;
