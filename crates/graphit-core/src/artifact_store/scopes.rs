//! Captured publication boundaries. Restored bindings remain data, never authority.
use super::native_source::NativeBirthAnchor;
use super::{snapshot::path_units, tree, *};

use crate::{
    brand::Brand, installation_identity::InstallationId, project_paths::InstallationPaths,
};
mod owner_root;
mod user;
pub use owner_root::{
    OwnerRootBindingLimits, OwnerRootBindingRecord, OwnerRootIdentity, OwnerRootMode,
};
pub use user::UserIdentity;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::{
    collections::BTreeMap,
    marker::PhantomData,
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum StoreOwner {
    User {
        #[serde(with = "installation_id")]
        installation_id: InstallationId,
        identity: UserIdentity,
    },
    Project {
        project_id: ProjectId,
    },
    Installation {
        #[serde(with = "installation_id")]
        installation_id: InstallationId,
    },
}
mod installation_id {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        id: &InstallationId,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(id.as_str())
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<InstallationId, D::Error> {
        struct IdVisitor;
        impl<'de> Visitor<'de> for IdVisitor {
            type Value = InstallationId;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a validated installation identifier")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                InstallationId::new(value).map_err(E::custom)
            }
        }
        deserializer.deserialize_str(IdVisitor)
    }
}

/// Bounds on allowed roots, each native path, captured directory entries and
/// path depth. No product defaults or accounting of inspection steps.
#[derive(Clone, Copy, Debug)]
pub struct ScopeLimits {
    roots: usize,
    path_bytes: usize,
    directory_entries: usize,
}
impl ScopeLimits {
    pub fn new(roots: usize, path_bytes: usize, directory_entries: usize) -> io::Result<Self> {
        if [roots, path_bytes, directory_entries].contains(&0) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self {
            roots,
            path_bytes,
            directory_entries,
        })
    }
}

#[derive(Clone)]
pub struct ScopeContext {
    deadline: Option<Instant>,
    cancellation: Cancellation,
    #[cfg(any(test, feature = "test-util"))]
    test_clock: Option<Arc<Mutex<Instant>>>,
}
impl ScopeContext {
    pub fn new(deadline: Instant, cancellation: Cancellation) -> Self {
        Self {
            deadline: Some(deadline),
            cancellation,
            #[cfg(any(test, feature = "test-util"))]
            test_clock: None,
        }
    }
    pub fn from_snapshot(request: &crate::file_io::SnapshotContext) -> Self {
        Self {
            deadline: request.deadline(),
            cancellation: request.cancellation().clone(),
            #[cfg(any(test, feature = "test-util"))]
            test_clock: request.test_clock().cloned(),
        }
    }
    pub(super) fn check(&self) -> io::Result<()> {
        check_cancelled(&self.cancellation)?;
        if let Some(deadline) = self.deadline {
            #[cfg(any(test, feature = "test-util"))]
            let now = crate::file_io::test_now(self.test_clock.as_ref())?;
            #[cfg(not(any(test, feature = "test-util")))]
            let now = Instant::now();
            if now >= deadline {
                return Err(io::ErrorKind::TimedOut.into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectoryBinding {
    path: NativePath,
    id: tree::PhysicalId,
    mode: tree::Mode,
}

/// Strict serializable observations. Cannot reconstruct CapturedPublicationScope.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct ScopeBindingRecord {
    owner: StoreOwner,
    brand: String,
    owner_root: NativePath,
    storage_root: NativePath,
    runtime_root: NativePath,
    allowed_roots: Vec<NativePath>,
    directories: Vec<DirectoryBinding>,
}
impl ScopeBindingRecord {
    pub fn pure_restore(
        bytes: &[u8],
        limits: ScopeLimits,
        max_document_bytes: usize,
    ) -> io::Result<Self> {
        if max_document_bytes == 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if bytes.len() > max_document_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let mut decoder = serde_json::Deserializer::from_slice(bytes);
        let result = BindingSeed(limits)
            .deserialize(&mut decoder)
            .map_err(|_| io::ErrorKind::InvalidData)?;
        decoder.end().map_err(|_| io::ErrorKind::InvalidData)?;
        Ok(result)
    }
    pub fn owner(&self) -> &StoreOwner {
        &self.owner
    }
    pub fn runtime_root(&self) -> &Path {
        &self.runtime_root.0
    }
    pub fn allowed_roots(&self) -> impl ExactSizeIterator<Item = &Path> {
        self.allowed_roots.iter().map(|p| p.0.as_path())
    }
    pub fn directory_count(&self) -> usize {
        self.directories.len()
    }
    /// Catalogue identity is shared by PROJECT worktrees. Checkout observations
    /// remain operation data and never extend the live scope's allowed roots.
    pub(super) fn matches_catalog(&self, scope: &CapturedPublicationScope) -> bool {
        if !matches!(self.owner, StoreOwner::Project { .. }) {
            return self.matches_scope(scope);
        }
        if self.owner != scope.inner.owner
            || self.brand != scope.inner.brand.key()
            || self.storage_root.0 != scope.inner.storage_root
            || self.runtime_root.0 != scope.inner.runtime_root
        {
            return false;
        }
        let directories = scope
            .inner
            .directories
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        self.directories
            .iter()
            .filter(|d| self.runtime_root.0.starts_with(&d.path.0))
            .all(|d| {
                directories
                    .get(&d.path.0)
                    .is_some_and(|live| binding_directory_matches(d, live))
            })
    }
    /// Validate persisted operation data against its recorded roots. This does
    /// not authorize IO; callers must also check the freshly captured scope.
    pub(super) fn destination(&self, path: &Path) -> io::Result<()> {
        validate_destination(
            path,
            &self.owner_root.0,
            &self.runtime_root.0,
            self.allowed_roots.iter().map(|root| root.0.as_path()),
        )
    }
    /// Data comparison only; revalidate the opaque scope separately before use.
    pub fn matches_scope(&self, scope: &CapturedPublicationScope) -> bool {
        if self.owner != scope.inner.owner
            || self.brand != scope.inner.brand.key()
            || self.owner_root.0 != scope.inner.owner_root
            || self.storage_root.0 != scope.inner.storage_root
            || self.runtime_root.0 != scope.inner.runtime_root
            || self
                .allowed_roots
                .iter()
                .map(|p| &p.0)
                .ne(scope.inner.allowed_roots.iter())
        {
            return false;
        }
        let directories = scope
            .inner
            .directories
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        self.directories.iter().all(|d| {
            directories
                .get(&d.path.0)
                .is_some_and(|live| binding_directory_matches(d, live))
        })
    }
    /// Context-bounded data comparison, without filesystem observations or grants.
    /// Revalidate the opaque scope separately before using the matched binding.
    /// Waits for concurrent observations; a poisoned scope returns `Other`.
    pub fn matches_scope_bounded(
        &self,
        scope: &CapturedPublicationScope,
        context: &ScopeContext,
    ) -> io::Result<bool> {
        context.check()?;
        if self.allowed_roots.is_empty()
            || self.allowed_roots.len() > scope.inner.limits.roots
            || self.directories.is_empty()
            || self.directories.len() > scope.inner.limits.directory_entries
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        // Records have already passed capture/restore validation. Admit their
        // retained native paths against this capture's policy without cloning
        // the record or rebuilding its physical-identity index.
        check_path(&self.owner_root.0, scope.inner.limits)?;
        context.check()?;
        check_path(&self.runtime_root.0, scope.inner.limits)?;
        check_path(&self.storage_root.0, scope.inner.limits)?;
        for root in &self.allowed_roots {
            check_comparison(context)?;
            check_path(&root.0, scope.inner.limits)?;
        }
        for directory in &self.directories {
            check_comparison(context)?;
            check_path(&directory.path.0, scope.inner.limits)?;
        }
        context.check()?;
        if self.owner != scope.inner.owner
            || self.brand != scope.inner.brand.key()
            || self.owner_root.0 != scope.inner.owner_root
            || self.storage_root.0 != scope.inner.storage_root
            || self.runtime_root.0 != scope.inner.runtime_root
            || self.allowed_roots.len() != scope.inner.allowed_roots.len()
        {
            context.check()?;
            return Ok(false);
        }
        for (recorded, live) in self.allowed_roots.iter().zip(&scope.inner.allowed_roots) {
            check_comparison(context)?;
            if recorded.0 != *live {
                context.check()?;
                return Ok(false);
            }
        }
        let directories = scope.lock_directories(context)?;
        context.check()?;
        for directory in &self.directories {
            check_comparison(context)?;
            if !directories
                .get(&directory.path.0)
                .is_some_and(|live| binding_directory_matches(directory, live))
            {
                context.check()?;
                return Ok(false);
            }
        }
        context.check()?;
        Ok(true)
    }
    fn validate(&self, limits: ScopeLimits) -> io::Result<()> {
        check_path(&self.storage_root.0, limits)?;
        if Brand::new(&self.brand)?.key() != self.brand {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let expected_runtime = match &self.owner {
            StoreOwner::Project { project_id } => self
                .storage_root
                .0
                .join("projects")
                .join(project_id.to_string()),
            StoreOwner::Installation { .. } => self.owner_root.0.join("runtime"),
            StoreOwner::User { identity, .. } => {
                let root = self
                    .owner_root
                    .0
                    .join("users")
                    .join(identity.namespace_key());
                if self.allowed_roots.len() != 1 || self.allowed_roots[0].0 != root {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                identity.validate(limits.path_bytes)?;
                root.join("runtime")
            }
        };
        if !matches!(self.owner, StoreOwner::Project { .. }) && self.storage_root != self.owner_root
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if self.runtime_root.0 != expected_runtime
            && !(matches!(&self.owner, StoreOwner::Installation { .. })
                && ["models", "native-instructions"]
                    .into_iter()
                    .any(|namespace| self.runtime_root.0 == expected_runtime.join(namespace)))
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        validate_layout(
            &self.owner_root.0,
            &self.runtime_root.0,
            self.allowed_roots.iter().map(|root| root.0.as_path()),
            self.directories
                .iter()
                .map(|binding| (binding.path.0.as_path(), &binding.id, binding.mode)),
            limits,
        )
    }
}

// A persisted observation may precede permission hardening of the same directory.
// The live capture still performs its own exact physical and policy revalidation.
fn binding_directory_matches(recorded: &DirectoryBinding, live: &tree::NodeReceipt) -> bool {
    recorded.id == live.id
        && live.relative.0.as_os_str().is_empty()
        && live.content.directory()
        && match (recorded.mode, live.mode) {
            (tree::Mode::Unix(recorded), tree::Mode::Unix(live)) => live & !recorded == 0,
            (tree::Mode::Windows(recorded), tree::Mode::Windows(live)) => !recorded || live,
            _ => false,
        }
}

fn check_comparison(context: &ScopeContext) -> io::Result<()> {
    #[cfg(test)]
    COMPARISON_STEP.with(|slot| {
        let hook = {
            let mut step = slot.borrow_mut();
            if let Some((remaining, _)) = step.as_mut() {
                if *remaining == 0 {
                    step.take().map(|(_, hook)| hook)
                } else {
                    *remaining -= 1;
                    None
                }
            } else {
                None
            }
        };
        if let Some(hook) = hook {
            hook();
        }
    });
    context.check()
}

fn validate_layout<'a>(
    owner_root: &'a Path,
    runtime_root: &'a Path,
    roots: impl ExactSizeIterator<Item = &'a Path> + Clone,
    directories: impl ExactSizeIterator<Item = (&'a Path, &'a tree::PhysicalId, tree::Mode)>,
    limits: ScopeLimits,
) -> io::Result<()> {
    if roots.len() == 0
        || roots.len() > limits.roots
        || directories.len() == 0
        || directories.len() > limits.directory_entries
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    check_path(owner_root, limits)?;
    check_path(runtime_root, limits)?;
    let mut previous = None;
    for root in roots.clone() {
        check_path(root, limits)?;
        if previous.is_some_and(|p| p >= root) || root.starts_with(runtime_root) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        previous = Some(root);
    }
    let mut previous = None;
    let mut identities = std::collections::HashSet::new();
    let mut has_owner_root = false;
    for (path, id, mode) in directories {
        check_path(path, limits)?;
        if previous.is_some_and(|p| p >= path)
            || ![owner_root, runtime_root]
                .into_iter()
                .chain(roots.clone())
                .any(|root| root.starts_with(path))
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        id.validate()?;
        if !identities.insert(id) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        mode.validate()?;
        has_owner_root |= path == owner_root;
        previous = Some(path);
    }
    if !has_owner_root {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(())
}

/// A real read-only capture of owner/runtime/destination boundaries.
///
/// ```compile_fail
/// use graphit_core::artifact_store::CapturedPublicationScope;
/// let _: CapturedPublicationScope = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone)]
pub struct CapturedPublicationScope {
    inner: Arc<CapturedPublicationScopeInner>,
}
struct CapturedPublicationScopeInner {
    owner: StoreOwner,
    publication_slot: PublicationSlot,
    brand: Brand,
    owner_root: PathBuf,
    storage_root: PathBuf,
    runtime_root: PathBuf,
    runtime_anchor_root: PathBuf,
    runtime_anchor: Arc<NativeBirthAnchor>,
    allowed_roots: Vec<PathBuf>,
    limits: ScopeLimits,
    directories: Arc<Mutex<BTreeMap<PathBuf, tree::NodeReceipt>>>,
    native_anchor: Arc<NativeBirthAnchor>,
}
impl fmt::Debug for CapturedPublicationScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CapturedPublicationScope { .. }")
    }
}
struct CapturePaths<'a, Roots> {
    owner_root: &'a Path,
    storage_root: &'a Path,
    runtime_root: &'a Path,
    allowed: Roots,
}

impl CapturedPublicationScope {
    #[cfg(test)]
    pub(crate) fn test_inspection_step(hook: Box<dyn FnOnce()>) {
        INSPECTION_STEP.with(|slot| *slot.borrow_mut() = Some(hook));
    }
    #[cfg(test)]
    pub(crate) fn test_before_lock(hook: Box<dyn FnOnce()>) {
        BEFORE_SCOPE_LOCK.with(|slot| *slot.borrow_mut() = Some(hook));
    }
    pub fn project(
        project: &ProjectPaths,
        project_id: ProjectId,
        limits: ScopeLimits,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Self> {
        request.check()?;
        let anchor = NativeBirthAnchor::capture(
            project.root(),
            request,
            limits.path_bytes,
            limits.directory_entries,
        )?;
        Self::capture(
            StoreOwner::Project { project_id },
            project.brand().clone(),
            CapturePaths {
                owner_root: project.root(),
                storage_root: project.global_root(),
                runtime_root: &project.project_runtime(project_id, &[])?,
                allowed: [project.root().to_owned()],
            },
            limits,
            request,
            anchor,
        )
    }
    pub fn installation(
        installation: &InstallationPaths,
        installation_id: InstallationId,
        allowed_roots: impl IntoIterator<Item = PathBuf>,
        limits: ScopeLimits,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Self> {
        Self::installation_scope(
            installation,
            installation_id,
            allowed_roots,
            limits,
            request,
            installation.global_path(&["runtime"])?,
        )
    }
    /// Tool-owned native agent/skill files share the existing writer and a
    /// dedicated control namespace, separate from cached artifacts and models.
    /// Destinations remain only the explicitly captured native directories.
    pub fn installation_instructions(
        installation: &InstallationPaths,
        installation_id: InstallationId,
        allowed_roots: impl IntoIterator<Item = PathBuf>,
        limits: ScopeLimits,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Self> {
        Self::installation_scope(
            installation,
            installation_id,
            allowed_roots,
            limits,
            request,
            installation.global_path(&["runtime", "native-instructions"])?,
        )
    }
    /// Model bundles have their own concrete control namespace so a native FILE
    /// cache or instruction publication cannot replace their captured root set.
    pub fn installation_models(
        installation: &InstallationPaths,
        installation_id: InstallationId,
        allowed_roots: impl IntoIterator<Item = PathBuf>,
        limits: ScopeLimits,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Self> {
        Self::installation_scope(
            installation,
            installation_id,
            allowed_roots,
            limits,
            request,
            installation.global_path(&["runtime", "models"])?,
        )
    }
    fn installation_scope(
        installation: &InstallationPaths,
        installation_id: InstallationId,
        allowed_roots: impl IntoIterator<Item = PathBuf>,
        limits: ScopeLimits,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
        runtime_root: PathBuf,
    ) -> io::Result<Self> {
        request.check()?;
        let anchor = NativeBirthAnchor::capture(
            installation.root(),
            request,
            limits.path_bytes,
            limits.directory_entries,
        )?;
        Self::capture(
            StoreOwner::Installation { installation_id },
            installation.brand().clone(),
            CapturePaths {
                owner_root: installation.root(),
                storage_root: installation.root(),
                runtime_root: &runtime_root,
                allowed: allowed_roots,
            },
            limits,
            request,
            anchor,
        )
    }
    fn capture(
        owner: StoreOwner,
        brand: Brand,
        paths: CapturePaths<'_, impl IntoIterator<Item = PathBuf>>,
        limits: ScopeLimits,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
        native_anchor: Arc<NativeBirthAnchor>,
    ) -> io::Result<Self> {
        let CapturePaths {
            owner_root,
            storage_root,
            runtime_root,
            allowed: roots,
        } = paths;
        let context = ScopeContext::from_snapshot(request);
        context.check()?;
        check_path(owner_root, limits)?;
        check_path(runtime_root, limits)?;
        check_path(storage_root, limits)?;
        let mut allowed_roots = Vec::new();
        for root in roots {
            if allowed_roots.len() >= limits.roots {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            check_path(&root, limits)?;
            allowed_roots.push(root);
        }
        if allowed_roots.is_empty() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let mut inspection = Inspection::new(limits, Some(&context));
        let owner_root = inspection.root(owner_root)?;
        if !inspection.directories.contains_key(&owner_root) {
            return Err(io::ErrorKind::NotFound.into());
        }
        let storage_root = inspection.root(storage_root)?;
        let runtime_root = inspection.root(runtime_root)?;
        for root in &mut allowed_roots {
            *root = inspection.root(root)?;
        }
        allowed_roots.sort();
        if allowed_roots.windows(2).any(|r| r[0] == r[1])
            || allowed_roots
                .iter()
                .any(|root| root.starts_with(&runtime_root))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        inspection.finish()?;
        let runtime_anchor_root = if matches!(owner, StoreOwner::Project { .. }) {
            storage_root
                .ancestors()
                .find(|path| inspection.directories.contains_key(*path))
                .ok_or(io::ErrorKind::NotFound)?
                .to_owned()
        } else {
            owner_root.clone()
        };
        let runtime_anchor = if runtime_anchor_root == owner_root {
            Arc::clone(&native_anchor)
        } else {
            NativeBirthAnchor::capture(
                &runtime_anchor_root,
                request,
                limits.path_bytes,
                limits.directory_entries,
            )?
        };
        let publication_slot = PublicationSlot::capture(
            &owner,
            &inspection
                .directories
                .get(&owner_root)
                .ok_or(io::ErrorKind::InvalidData)?
                .id,
        );
        let result = Self {
            inner: Arc::new(CapturedPublicationScopeInner {
                owner,
                publication_slot,
                brand,
                owner_root,
                storage_root,
                runtime_root,
                runtime_anchor_root,
                runtime_anchor,
                allowed_roots,
                limits,
                directories: Arc::new(Mutex::new(inspection.directories)),
                native_anchor,
            }),
        };
        {
            let captured = result.lock_directories(&context)?;
            let receipt = captured
                .get(&result.inner.owner_root)
                .ok_or(io::ErrorKind::InvalidData)?;
            result
                .inner
                .native_anchor
                .matches_receipt(receipt, request)?;
            let receipt = captured
                .get(&result.inner.runtime_anchor_root)
                .ok_or(io::ErrorKind::InvalidData)?;
            result
                .inner
                .runtime_anchor
                .matches_receipt(receipt, request)?;
        }
        result.binding()?.validate(limits)?;
        context.check()?;
        Ok(result)
    }
    pub(crate) fn require_project(&self) -> io::Result<()> {
        if !matches!(self.inner.owner, StoreOwner::Project { .. }) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(())
    }
    pub(super) fn native_anchor(&self) -> &Arc<NativeBirthAnchor> {
        &self.inner.native_anchor
    }
    pub(super) fn runtime_anchor(&self) -> &Arc<NativeBirthAnchor> {
        &self.inner.runtime_anchor
    }
    pub(crate) fn runtime_anchor_root(&self) -> &Path {
        &self.inner.runtime_anchor_root
    }
    pub(crate) fn runtime_selection(&self, relative: &Path) -> io::Result<PathBuf> {
        crate::file_io::validate_relative(relative, true)?;
        let root = self
            .runtime_root()
            .strip_prefix(self.runtime_anchor_root())
            .map_err(|_| io::ErrorKind::InvalidData)?;
        Ok(if relative.as_os_str().is_empty() {
            root.to_path_buf()
        } else {
            root.join(relative)
        })
    }
    pub(super) fn revalidate_native(
        &self,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<()> {
        request.check()?;

        let context = ScopeContext::from_snapshot(request);
        self.revalidate_bounded(&context)?;
        let directories = self.lock_directories(&context)?;
        let node = directories
            .get(&self.inner.owner_root)
            .ok_or(io::ErrorKind::InvalidData)?;
        self.native_anchor().matches_receipt(node, request)?;
        let node = directories
            .get(&self.inner.runtime_anchor_root)
            .ok_or(io::ErrorKind::InvalidData)?;
        self.runtime_anchor().matches_receipt(node, request)?;
        request.check()
    }
    pub fn owner(&self) -> &StoreOwner {
        &self.inner.owner
    }
    pub fn brand(&self) -> &Brand {
        &self.inner.brand
    }
    pub fn owner_root(&self) -> &Path {
        &self.inner.owner_root
    }
    /// Publication selection DATA captured once with the real owner root.
    /// No authority, permission mode, path or Git version is encoded in the slot.
    pub fn publication_slot(&self) -> PublicationSlot {
        self.inner.publication_slot
    }
    pub fn runtime_root(&self) -> &Path {
        &self.inner.runtime_root
    }
    pub fn allowed_roots(&self) -> impl ExactSizeIterator<Item = &Path> {
        self.inner.allowed_roots.iter().map(PathBuf::as_path)
    }
    pub fn binding(&self) -> io::Result<ScopeBindingRecord> {
        let directories = self
            .inner
            .directories
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let record = ScopeBindingRecord {
            owner: self.inner.owner.clone(),
            brand: self.inner.brand.key().into(),
            owner_root: NativePath(self.inner.owner_root.clone()),
            storage_root: NativePath(self.inner.storage_root.clone()),
            runtime_root: NativePath(self.inner.runtime_root.clone()),
            allowed_roots: self
                .inner
                .allowed_roots
                .iter()
                .cloned()
                .map(NativePath)
                .collect(),
            directories: directories
                .iter()
                .map(|(path, node)| DirectoryBinding {
                    path: NativePath(path.clone()),
                    id: node.id.clone(),
                    mode: node.mode,
                })
                .collect(),
        };
        record.validate(self.inner.limits)?;
        Ok(record)
    }
    pub fn revalidate(&self, context: &ScopeContext) -> io::Result<()> {
        self.check(Some(context))
    }
    fn lock_directories(
        &self,
        context: &ScopeContext,
    ) -> io::Result<std::sync::MutexGuard<'_, BTreeMap<PathBuf, tree::NodeReceipt>>> {
        context.check()?;
        #[cfg(test)]
        BEFORE_SCOPE_LOCK.with(|slot| {
            if let Some(hook) = slot.borrow_mut().take() {
                hook();
            }
        });
        let captured = self
            .inner
            .directories
            .lock()
            .map_err(|_| io::ErrorKind::Other)?;
        context.check()?;
        Ok(captured)
    }
    /// Revalidate under the original context after waiting for concurrent observations.
    /// A poisoned scope remains an error.
    /// `revalidate` uses a blocking lock and recovers its recorded observations after poison.
    pub fn revalidate_bounded(&self, context: &ScopeContext) -> io::Result<()> {
        context.check()?;
        let mut captured = self.lock_directories(context)?;
        self.check_locked(&mut captured, Some(context))?;
        context.check()
    }
    /// Revalidate the installation namespace without requiring unrelated project roots.
    /// This does not validate or authorize a project capture.
    pub fn revalidate_installation(&self, context: &ScopeContext) -> io::Result<()> {
        context.check()?;
        let mut captured = self.lock_directories(context)?;
        self.check_installation_locked(&mut captured, context)?;
        context.check()
    }
    fn check_installation_locked(
        &self,
        captured: &mut BTreeMap<PathBuf, tree::NodeReceipt>,
        context: &ScopeContext,
    ) -> io::Result<()> {
        if !matches!(self.inner.owner, StoreOwner::Installation { .. }) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let mut inspection = Inspection::new(self.inner.limits, Some(context));
        for root in [&self.inner.owner_root, &self.inner.runtime_root] {
            if inspection.root(root)? != *root {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        for (path, node) in captured.iter().filter(|(path, _)| {
            self.inner.owner_root.starts_with(path.as_path())
                || self.inner.runtime_root.starts_with(path.as_path())
        }) {
            if inspection.directories.get(path) != Some(node) {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        inspection.finish()?;
        validate_layout(
            &self.inner.owner_root,
            &self.inner.runtime_root,
            std::iter::once(self.inner.owner_root.as_path()),
            inspection
                .directories
                .iter()
                .map(|(path, node)| (path.as_path(), &node.id, node.mode)),
            self.inner.limits,
        )?;
        context.check()?;
        // Keep project observations for operations that use their full captures.
        captured.extend(inspection.directories);
        Ok(())
    }
    pub(crate) fn recheck_runtime_installation(
        &self,
        root: &Path,
        root_handle: &std::fs::File,
        brand: &Brand,
        context: &ScopeContext,
    ) -> io::Result<()> {
        context.check()?;
        let mut captured = self.lock_directories(context)?;
        if !matches!(self.inner.owner, StoreOwner::Installation { .. })
            || self.inner.brand.key() != brand.key()
            || self.inner.owner_root != root
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.check_installation_locked(&mut captured, context)?;
        let node = captured.get(root).ok_or(io::ErrorKind::InvalidData)?;
        if node.id != crate::native_file_identity::handle_id(root_handle)? {
            return Err(io::ErrorKind::InvalidData.into());
        }
        context.check()
    }
    pub(super) fn check(&self, context: Option<&ScopeContext>) -> io::Result<()> {
        let mut captured = self
            .inner
            .directories
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        self.check_locked(&mut captured, context)
    }
    fn check_locked(
        &self,
        captured: &mut BTreeMap<PathBuf, tree::NodeReceipt>,
        context: Option<&ScopeContext>,
    ) -> io::Result<()> {
        let mut inspection = Inspection::new(self.inner.limits, context);
        for root in [&self.inner.owner_root, &self.inner.runtime_root]
            .into_iter()
            .chain(self.inner.allowed_roots.iter())
        {
            if inspection.root(root)? != *root {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        for (path, node) in captured.iter() {
            if inspection.directories.get(path) != Some(node) {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        inspection.finish()?;
        validate_layout(
            &self.inner.owner_root,
            &self.inner.runtime_root,
            self.inner.allowed_roots.iter().map(PathBuf::as_path),
            inspection
                .directories
                .iter()
                .map(|(path, node)| (path.as_path(), &node.id, node.mode)),
            self.inner.limits,
        )?;
        if let Some(context) = context {
            context.check()?;
        }
        // Newly appearing layout directories are observations, never payload receipts.
        *captured = inspection.directories;
        Ok(())
    }
    /// Captured policy for bounded data restoration.
    pub fn limits(&self) -> ScopeLimits {
        self.inner.limits
    }
    /// Admit a finite provider layout, including future observations of roots.
    /// Planned paths are not directory receipts or permission to recreate an owner.
    pub(crate) fn admit_native_document_layout(
        &self,
        parent: &Path,
        births: &[PathBuf],
        context: &ScopeContext,
    ) -> io::Result<()> {
        self.revalidate(context)?;
        let admit = |path: &Path| -> io::Result<()> {
            valid_path(path)?;
            if path_units(path)? > self.inner.limits.path_bytes {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            if !self
                .inner
                .allowed_roots
                .iter()
                .any(|root| path.starts_with(root))
                || path.starts_with(&self.inner.runtime_root)
            {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            Ok(())
        };
        admit(parent)?;
        for path in births {
            admit(path)?;
            if self.inner.runtime_root.starts_with(path) || *path == self.inner.owner_root {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        }
        let roots = || {
            [&self.inner.owner_root, &self.inner.runtime_root]
                .into_iter()
                .chain(self.inner.allowed_roots.iter())
        };
        let mut inspection = Inspection::new(self.inner.limits, Some(context));
        for root in roots() {
            if inspection.root(root)? != *root {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        inspection.finish()?;
        context.check()
    }
    /// The control namespace is an exact runtime destination derived by the
    /// opaque scope factory; it is deliberately separate from publication roots.
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(super) fn admit_native_control_layout(
        &self,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<()> {
        let context = ScopeContext::from_snapshot(request);
        self.revalidate_bounded(&context)?;
        let relative = self
            .runtime_root()
            .strip_prefix(self.runtime_anchor_root())
            .map_err(|_| io::ErrorKind::InvalidInput)?;
        crate::file_io::validate_relative(relative, false)?;
        let mut inspection = Inspection::new(self.inner.limits, Some(&context));
        for root in [&self.inner.owner_root, &self.inner.runtime_root]
            .into_iter()
            .chain(self.inner.allowed_roots.iter())
        {
            if inspection.root(root)? != *root {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        inspection.finish()?;
        request.check()
    }

    pub(super) fn destination(&self, path: &Path) -> io::Result<()> {
        validate_destination(
            path,
            &self.inner.owner_root,
            &self.inner.runtime_root,
            self.inner.allowed_roots.iter().map(PathBuf::as_path),
        )
    }
}

fn validate_destination<'a>(
    path: &Path,
    owner_root: &Path,
    runtime_root: &Path,
    roots: impl Iterator<Item = &'a Path> + Clone,
) -> io::Result<()> {
    valid_path(path)?;
    if roots.clone().any(|root| root.starts_with(path))
        || !roots.clone().any(|root| path.starts_with(root))
        || path.starts_with(runtime_root)
        || runtime_root.starts_with(path)
        || path == owner_root
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}

fn check_path(path: &Path, limits: ScopeLimits) -> io::Result<()> {
    valid_path(path)?;
    if path_units(path)? > limits.path_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    Ok(())
}
struct Inspection<'a> {
    limits: ScopeLimits,
    context: Option<&'a ScopeContext>,
    directories: BTreeMap<PathBuf, tree::NodeReceipt>,
    examined: BTreeMap<PathBuf, PathBuf>,
    missing: std::collections::BTreeSet<PathBuf>,
}
impl<'a> Inspection<'a> {
    fn new(limits: ScopeLimits, context: Option<&'a ScopeContext>) -> Self {
        Self {
            limits,
            context,
            directories: BTreeMap::new(),
            examined: BTreeMap::new(),
            missing: Default::default(),
        }
    }
    fn step(&self) -> io::Result<()> {
        if let Some(context) = self.context {
            context.check()?;
        }
        #[cfg(test)]
        INSPECTION_STEP.with(|hook| {
            if let Some(hook) = hook.borrow_mut().take() {
                hook();
            }
        });
        Ok(())
    }
    fn root(&mut self, path: &Path) -> io::Result<PathBuf> {
        valid_path(path)?;
        if path_units(path)? > self.limits.path_bytes
            || path.components().count() > self.limits.directory_entries
        {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let mut chain = Vec::new();
        for ancestor in path.ancestors() {
            check_path(ancestor, self.limits)?;
            chain.push(ancestor);
        }
        let mut anchor = None;
        for ancestor in chain.into_iter().rev() {
            if self.missing.contains(ancestor) {
                break;
            }
            if !self.examined.contains_key(ancestor) {
                self.step()?;
                self.context.map(|context| context.check()).transpose()?;
                match fs::symlink_metadata(ancestor) {
                    Ok(metadata) => {
                        if !metadata.is_dir()
                            || metadata.file_type().is_symlink()
                            || reparse(&metadata)
                        {
                            return Err(io::ErrorKind::InvalidData.into());
                        }
                        self.context.map(|context| context.check()).transpose()?;
                        let canonical = fs::canonicalize(ancestor)?;
                        self.context.map(|context| context.check()).transpose()?;
                        let node = tree::TreeReceipt::directory(ancestor)?;
                        if !self.directories.contains_key(&canonical)
                            && self.directories.len() >= self.limits.directory_entries
                        {
                            return Err(io::ErrorKind::FileTooLarge.into());
                        }
                        self.directories.insert(canonical.clone(), node);
                        self.examined.insert(ancestor.to_owned(), canonical);
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        self.missing.insert(ancestor.to_owned());
                        break;
                    }
                    Err(error) => return Err(error),
                }
            }
            anchor = Some(ancestor);
        }
        let anchor = anchor.ok_or(io::ErrorKind::NotFound)?;
        let canonical = self
            .examined
            .get(anchor)
            .ok_or(io::ErrorKind::InvalidData)?;
        Ok(canonical.join(
            path.strip_prefix(anchor)
                .map_err(|_| io::ErrorKind::InvalidData)?,
        ))
    }
    fn finish(&self) -> io::Result<()> {
        for (path, node) in &self.directories {
            self.step()?;
            self.context.map(|context| context.check()).transpose()?;
            tree::checked_node(path, node, None, false)?;
            self.context.map(|context| context.check()).transpose()?;
            if reparse(&fs::symlink_metadata(path)?) {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        if let Some(context) = self.context {
            context.check()?;
        }
        Ok(())
    }
}
fn reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

pub(super) struct BindingSeed(pub ScopeLimits);
impl<'de> DeserializeSeed<'de> for BindingSeed {
    type Value = ScopeBindingRecord;
    fn deserialize<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<Self::Value, D::Error> {
        decoder.deserialize_map(BindingVisitor(self.0))
    }
}
struct BindingVisitor(ScopeLimits);
impl<'de> Visitor<'de> for BindingVisitor {
    type Value = ScopeBindingRecord;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a closed scope binding")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let (
            mut owner,
            mut brand,
            mut owner_root,
            mut storage_root,
            mut runtime_root,
            mut roots,
            mut directories,
        ) = (None, None, None, None, None, None, None);
        let mut seen = std::collections::HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(de::Error::custom("duplicate scope field"));
            }
            match key.as_str() {
                "owner" => owner = Some(map.next_value()?),
                "brand" => brand = Some(map.next_value()?),
                "owner_root" => {
                    let path: NativePath = map.next_value()?;
                    check_path(&path.0, self.0).map_err(de::Error::custom)?;
                    owner_root = Some(path);
                }
                "storage_root" => {
                    let path: NativePath = map.next_value()?;
                    check_path(&path.0, self.0).map_err(de::Error::custom)?;
                    storage_root = Some(path);
                }
                "runtime_root" => {
                    let path: NativePath = map.next_value()?;
                    check_path(&path.0, self.0).map_err(de::Error::custom)?;
                    runtime_root = Some(path);
                }
                "allowed_roots" => {
                    roots = Some(map.next_value_seed(ListSeed::<NativePath> {
                        max: self.0.roots,
                        path_bytes: self.0.path_bytes,
                        marker: PhantomData,
                    })?)
                }
                "directories" => {
                    directories = Some(map.next_value_seed(ListSeed::<DirectoryBinding> {
                        max: self.0.directory_entries,
                        path_bytes: self.0.path_bytes,
                        marker: PhantomData,
                    })?)
                }
                _ => return Err(de::Error::custom("unknown scope field")),
            }
        }
        let record = ScopeBindingRecord {
            owner: owner.ok_or_else(|| de::Error::missing_field("owner"))?,
            brand: brand.ok_or_else(|| de::Error::missing_field("brand"))?,
            owner_root: owner_root.ok_or_else(|| de::Error::missing_field("owner_root"))?,
            storage_root: storage_root.ok_or_else(|| de::Error::missing_field("storage_root"))?,
            runtime_root: runtime_root.ok_or_else(|| de::Error::missing_field("runtime_root"))?,
            allowed_roots: roots.ok_or_else(|| de::Error::missing_field("allowed_roots"))?,
            directories: directories.ok_or_else(|| de::Error::missing_field("directories"))?,
        };
        record.validate(self.0).map_err(de::Error::custom)?;
        Ok(record)
    }
}
trait BindingPath {
    fn path(&self) -> &Path;
}
impl BindingPath for NativePath {
    fn path(&self) -> &Path {
        &self.0
    }
}
impl BindingPath for DirectoryBinding {
    fn path(&self) -> &Path {
        &self.path.0
    }
}
struct ListSeed<T> {
    max: usize,
    path_bytes: usize,
    marker: PhantomData<T>,
}
impl<'de, T: Deserialize<'de> + BindingPath> DeserializeSeed<'de> for ListSeed<T> {
    type Value = Vec<T>;
    fn deserialize<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<Vec<T>, D::Error> {
        decoder.deserialize_seq(self)
    }
}
impl<'de, T: Deserialize<'de> + BindingPath> Visitor<'de> for ListSeed<T> {
    type Value = Vec<T>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded native scope paths")
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut sequence: S) -> Result<Vec<T>, S::Error> {
        let mut values = Vec::new();
        while values.len() < self.max {
            let Some(value) = sequence.next_element::<T>()? else {
                return Ok(values);
            };
            valid_path(value.path()).map_err(de::Error::custom)?;
            if path_units(value.path()).map_err(de::Error::custom)? > self.path_bytes {
                return Err(de::Error::custom("scope path exceeds its bound"));
            }
            values.push(value);
        }
        sequence.next_element_seed(RejectExtra)?;
        Ok(values)
    }
}
struct RejectExtra;
impl<'de> DeserializeSeed<'de> for RejectExtra {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, _: D) -> Result<(), D::Error> {
        Err(de::Error::custom("scope array exceeds its bound"))
    }
}

#[cfg(test)]
type ComparisonStep = (usize, Box<dyn FnOnce()>);

#[cfg(test)]
thread_local! {
    static BEFORE_SCOPE_LOCK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
    static INSPECTION_STEP: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
    static COMPARISON_STEP: std::cell::RefCell<Option<ComparisonStep>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
// Scope birth performs metadata work only; payload requests keep their own original context.
fn native_capture_request() -> std::sync::Arc<crate::file_io::SnapshotContext> {
    std::sync::Arc::new(crate::file_io::SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scope_context_shares_snapshot_test_time_and_cancellation() {
        let deadline = Instant::now();
        let clock = Arc::new(Mutex::new(deadline - Duration::from_secs(1)));
        let original = Arc::new(crate::file_io::SnapshotContext::new(
            deadline,
            Cancellation::default(),
        ));
        original.set_test_clock(Arc::clone(&clock)).unwrap();
        let scope = ScopeContext::from_snapshot(&original);
        let cloned_scope = scope.clone();
        original.check().unwrap();
        scope.check().unwrap();
        cloned_scope.check().unwrap();
        assert_eq!(scope.deadline, original.deadline());
        *clock.lock().unwrap() = deadline;
        for context in [&scope, &cloned_scope] {
            assert_eq!(context.check().unwrap_err().kind(), io::ErrorKind::TimedOut);
        }
        assert_eq!(
            original.check().unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        original.cancellation().cancel();
        for context in [&scope, &cloned_scope] {
            assert_eq!(
                context.check().unwrap_err().kind(),
                io::ErrorKind::Interrupted
            );
        }
    }
    #[test]
    fn independently_created_scope_context_keeps_real_time() {
        let deadline = Instant::now();
        let original = crate::file_io::SnapshotContext::new(deadline, Cancellation::default());
        original
            .set_test_clock(Arc::new(Mutex::new(deadline - Duration::from_secs(1))))
            .unwrap();
        ScopeContext::from_snapshot(&original).check().unwrap();
        let fresh = ScopeContext::new(deadline, original.cancellation().clone());
        assert_eq!(fresh.check().unwrap_err().kind(), io::ErrorKind::TimedOut);
    }
    fn runtime_scope_fixture() -> (tempfile::TempDir, CapturedPublicationScope, std::fs::File) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("installation");
        let handle = graphit_fs::create_private_directory(&root).unwrap();
        let paths = InstallationPaths::resolve(
            Brand::new("graphit").unwrap(),
            temp.path(),
            None,
            Some(&root),
        )
        .unwrap();
        let scope = CapturedPublicationScope::installation(
            &paths,
            InstallationId::new("fixture").unwrap(),
            [root.clone(), root.join("data")],
            ScopeLimits::new(8, 65536, 256).unwrap(),
            &native_capture_request(),
        )
        .unwrap();
        (temp, scope, handle)
    }
    fn bounded_context() -> ScopeContext {
        ScopeContext::new(
            Instant::now() + std::time::Duration::from_secs(10),
            Cancellation::default(),
        )
    }
    #[test]
    fn installation_scope_ignores_removed_project_but_full_scope_still_rejects() {
        let (temp, original, handle) = runtime_scope_fixture();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let paths = InstallationPaths::resolve(
            Brand::new("graphit").unwrap(),
            temp.path(),
            None,
            Some(original.owner_root()),
        )
        .unwrap();
        let scope = CapturedPublicationScope::installation(
            &paths,
            InstallationId::new("fixture").unwrap(),
            [paths.root().to_owned(), project.clone()],
            original.limits(),
            &native_capture_request(),
        )
        .unwrap();
        let record = scope.binding().unwrap();
        fs::remove_dir(&project).unwrap();
        let context = bounded_context();
        scope.revalidate_installation(&context).unwrap();
        scope
            .recheck_runtime_installation(scope.owner_root(), &handle, scope.brand(), &context)
            .unwrap();
        assert!(scope.revalidate_bounded(&context).is_err());
        assert!(record.matches_scope_bounded(&scope, &context).unwrap());
        let cancelled = bounded_context();
        cancelled.cancellation.cancel();
        assert_eq!(
            scope
                .revalidate_installation(&cancelled)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
        let expired = ScopeContext::new(Instant::now(), Cancellation::default());
        assert_eq!(
            scope.revalidate_installation(&expired).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
    }
    #[test]
    fn installation_scope_rejects_replaced_runtime_receipt() {
        let (temp, scope, _handle) = runtime_scope_fixture();
        fs::create_dir_all(scope.runtime_root()).unwrap();
        let context = bounded_context();
        scope.revalidate_installation(&context).unwrap();
        fs::rename(scope.runtime_root(), temp.path().join("held-runtime")).unwrap();
        fs::create_dir(scope.runtime_root()).unwrap();
        assert_eq!(
            scope.revalidate_installation(&context).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    #[test]
    fn binding_comparison_preserves_shape_and_complete_receipts() {
        let (_temp, scope, _handle) = runtime_scope_fixture();
        let context = bounded_context();
        let record = scope.binding().unwrap();
        assert!(record.matches_scope_bounded(&scope, &context).unwrap());
        for namespace in ["models", "native-instructions"] {
            let mut known = record.clone();
            known.runtime_root.0.push(namespace);
            known.validate(scope.limits()).unwrap();
            // A valid recorded namespace is still not the live default scope.
            assert!(!known.matches_scope_bounded(&scope, &context).unwrap());
        }
        let mut mismatches = Vec::new();
        let mut changed = record.clone();
        changed.owner = StoreOwner::Installation {
            installation_id: InstallationId::new("other").unwrap(),
        };
        mismatches.push(changed);
        let mut changed = record.clone();
        changed.owner = StoreOwner::Project {
            project_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
        };
        mismatches.push(changed);
        let mut changed = record.clone();
        changed.brand = "other".into();
        mismatches.push(changed);
        let mut changed = record.clone();
        changed.owner_root.0.push("other");
        mismatches.push(changed);
        let mut changed = record.clone();
        changed.runtime_root.0.push("other");
        assert_eq!(
            changed.validate(scope.limits()).unwrap_err().kind(),
            io::ErrorKind::InvalidData,
        );
        mismatches.push(changed);
        let mut changed = record.clone();
        changed.allowed_roots.reverse();
        mismatches.push(changed);
        let mut changed = record.clone();
        changed.allowed_roots[0].0.push("other");
        mismatches.push(changed);
        let mut changed = record.clone();
        changed.allowed_roots.pop();
        mismatches.push(changed);
        let mut changed = record.clone();
        changed.directories[0].path.0.push("other");
        mismatches.push(changed);
        let mut changed = record.clone();
        changed.directories[0].id = record.directories[1].id.clone();
        mismatches.push(changed);
        for changed in mismatches {
            assert!(!changed.matches_scope_bounded(&scope, &context).unwrap());
        }
        assert!(record.matches_scope_bounded(&scope, &context).unwrap());
    }
    #[test]
    fn persisted_directory_bindings_keep_only_path_identity_and_mode() {
        let (_temp, scope, _handle) = runtime_scope_fixture();
        let record = scope.binding().unwrap();
        let value = serde_json::to_value(&record).unwrap();
        for directory in value["directories"].as_array().unwrap() {
            let fields = directory.as_object().unwrap();
            assert_eq!(fields.len(), 3);
            assert!(
                fields.contains_key("path")
                    && fields.contains_key("id")
                    && fields.contains_key("mode")
            );
        }
        let bytes = serde_json::to_vec(&record).unwrap();
        assert!(
            ScopeBindingRecord::pure_restore(&bytes, scope.limits(), bytes.len()).unwrap()
                == record
        );
    }

    #[test]
    fn persisted_directory_mode_accepts_only_hardening() {
        let (_temp, scope, _handle) = runtime_scope_fixture();
        let record = scope.binding().unwrap();
        let mut recorded = record.directories[0].clone();
        let mut live = tree::TreeReceipt::directory(&recorded.path.0).unwrap();
        for (old, current, matches) in [
            (tree::Mode::Unix(0o755), tree::Mode::Unix(0o700), true),
            (tree::Mode::Unix(0o755), tree::Mode::Unix(0o750), true),
            (tree::Mode::Unix(0o700), tree::Mode::Unix(0o700), true),
            (tree::Mode::Unix(0o700), tree::Mode::Unix(0o755), false),
            (tree::Mode::Unix(0o755), tree::Mode::Unix(0o775), false),
            (tree::Mode::Windows(false), tree::Mode::Windows(true), true),
            (tree::Mode::Windows(true), tree::Mode::Windows(false), false),
            (tree::Mode::Windows(true), tree::Mode::Windows(true), true),
            (tree::Mode::Unix(0o700), tree::Mode::Windows(true), false),
        ] {
            recorded.mode = old;
            live.mode = current;
            assert_eq!(binding_directory_matches(&recorded, &live), matches);
        }
    }
    #[test]
    fn binding_comparison_accepts_extra_observations_and_does_no_filesystem_io() {
        let (temp, scope, _handle) = runtime_scope_fixture();
        let context = bounded_context();
        let record = scope.binding().unwrap();
        drop(graphit_fs::create_private_directory(scope.runtime_root()).unwrap());
        scope.revalidate_bounded(&context).unwrap();
        assert!(scope.binding().unwrap().directory_count() > record.directory_count());
        assert!(record.matches_scope_bounded(&scope, &context).unwrap());
        fs::rename(scope.owner_root(), temp.path().join("moved")).unwrap();
        // A data match is deliberately not fresh physical revalidation.
        assert!(record.matches_scope_bounded(&scope, &context).unwrap());
        assert!(scope.revalidate_bounded(&context).is_err());
    }
    #[test]
    fn binding_comparison_denies_poison_without_recovery() {
        let (_temp, scope, _handle) = runtime_scope_fixture();
        let context = bounded_context();
        let record = scope.binding().unwrap();
        assert!(record.matches_scope_bounded(&scope, &context).unwrap());
        let _ = std::panic::catch_unwind(|| {
            let _guard = scope.inner.directories.lock().unwrap();
            panic!("binding comparison fixture poison");
        });
        assert_eq!(
            record
                .matches_scope_bounded(&scope, &context)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Other
        );
        assert!(record.matches_scope(&scope));
        assert_eq!(
            record
                .matches_scope_bounded(&scope, &context)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Other
        );
    }
    #[test]
    fn binding_comparison_preserves_cancellation_and_deadline_through_loops() {
        let (_temp, scope, _handle) = runtime_scope_fixture();
        let record = scope.binding().unwrap();
        let mut mismatch = record.clone();
        mismatch.brand = "other".into();
        let cancelled = bounded_context();
        cancelled.cancellation.cancel();
        let expired = ScopeContext::new(Instant::now(), Cancellation::default());
        for candidate in [&record, &mismatch] {
            assert_eq!(
                candidate
                    .matches_scope_bounded(&scope, &cancelled)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::Interrupted
            );
            assert_eq!(
                candidate
                    .matches_scope_bounded(&scope, &expired)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::TimedOut
            );
        }
        // Interrupt root admission, directory admission, and the final receipt loop.
        for step in [
            0,
            record.allowed_roots.len(),
            2 * record.allowed_roots.len() + record.directories.len(),
        ] {
            let context = bounded_context();
            let cancellation = context.cancellation.clone();
            COMPARISON_STEP.with(|slot| {
                *slot.borrow_mut() = Some((step, Box::new(move || cancellation.cancel())));
            });
            assert_eq!(
                record
                    .matches_scope_bounded(&scope, &context)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::Interrupted
            );
            COMPARISON_STEP.with(|slot| assert!(slot.borrow().is_none()));
        }
        assert!(
            record
                .matches_scope_bounded(&scope, &bounded_context())
                .unwrap()
        );
    }
    #[test]
    fn binding_comparison_admits_exact_native_caps_and_rejects_one_under() {
        let (_temp, mut scope, _handle) = runtime_scope_fixture();
        let context = bounded_context();
        let record = scope.binding().unwrap();
        let paths = [&record.owner_root.0, &record.runtime_root.0]
            .into_iter()
            .chain(record.allowed_roots.iter().map(|root| &root.0))
            .chain(record.directories.iter().map(|directory| &directory.path.0));
        let units: Vec<_> = paths.map(|path| path_units(path).unwrap()).collect();
        let maximum = *units.iter().max().unwrap();
        let limits = ScopeLimits::new(
            record.allowed_roots.len(),
            maximum,
            record.directories.len(),
        )
        .unwrap();
        Arc::get_mut(&mut scope.inner)
            .expect("unique scope fixture")
            .limits = limits;
        assert!(record.matches_scope_bounded(&scope, &context).unwrap());
        for field in 0..3 {
            Arc::get_mut(&mut scope.inner)
                .expect("unique scope fixture")
                .limits = limits;
            let kind = match field {
                0 => {
                    Arc::get_mut(&mut scope.inner)
                        .expect("unique scope fixture")
                        .limits
                        .roots -= 1;
                    io::ErrorKind::InvalidData
                }
                1 => {
                    Arc::get_mut(&mut scope.inner)
                        .expect("unique scope fixture")
                        .limits
                        .directory_entries -= 1;
                    io::ErrorKind::InvalidData
                }
                _ => {
                    Arc::get_mut(&mut scope.inner)
                        .expect("unique scope fixture")
                        .limits
                        .path_bytes -= 1;
                    io::ErrorKind::FileTooLarge
                }
            };
            assert_eq!(
                record
                    .matches_scope_bounded(&scope, &context)
                    .unwrap_err()
                    .kind(),
                kind
            );
        }
        Arc::get_mut(&mut scope.inner)
            .expect("unique scope fixture")
            .limits = limits;
        assert!(record.matches_scope_bounded(&scope, &context).unwrap());
        assert!(scope.binding().unwrap() == record);
    }
    #[cfg(unix)]
    #[test]
    fn binding_comparison_preserves_distinct_unicode_paths() {
        let (temp, old, _handle) = runtime_scope_fixture();
        let root = temp.path().join("installation-é");
        fs::rename(old.owner_root(), &root).unwrap();
        let paths = InstallationPaths::resolve(
            Brand::new("graphit").unwrap(),
            temp.path(),
            None,
            Some(&root),
        )
        .unwrap();
        let context = bounded_context();
        let scope = CapturedPublicationScope::installation(
            &paths,
            InstallationId::new("fixture").unwrap(),
            [root.clone()],
            old.limits(),
            &native_capture_request(),
        )
        .unwrap();
        let record = scope.binding().unwrap();
        let bytes = serde_json::to_vec(&record).unwrap();
        let restored =
            ScopeBindingRecord::pure_restore(&bytes, scope.limits(), bytes.len()).unwrap();
        assert!(restored.matches_scope_bounded(&scope, &context).unwrap());
        let mut changed = restored;
        changed.owner_root.0 = temp.path().join("installation-ê");
        assert_ne!(changed.owner_root.as_path(), root);
        assert!(!changed.matches_scope_bounded(&scope, &context).unwrap());
    }
    #[test]
    fn runtime_scope_bounded_poison_preserves_explicit_recovery() {
        let (_temp, scope, handle) = runtime_scope_fixture();
        let context = bounded_context();
        let dirs = Arc::clone(&scope.inner.directories);
        let _ = std::thread::spawn(move || {
            let _guard = dirs.lock().unwrap();
            panic!("scope fixture poison");
        })
        .join();
        assert_eq!(
            scope.revalidate_bounded(&context).unwrap_err().kind(),
            io::ErrorKind::Other
        );
        assert_eq!(
            scope
                .recheck_runtime_installation(scope.owner_root(), &handle, scope.brand(), &context)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Other
        );
        assert_eq!(
            scope.revalidate_installation(&context).unwrap_err().kind(),
            io::ErrorKind::Other
        );
        scope.revalidate(&context).unwrap();
        // Explicit recovery intentionally does not clear the poison flag.
        assert_eq!(
            scope.revalidate_bounded(&context).unwrap_err().kind(),
            io::ErrorKind::Other
        );
    }
    #[test]
    fn bounded_scope_waits_for_observations_and_rechecks_original() {
        let (_temp, scope, handle) = runtime_scope_fixture();
        let record = scope.binding().unwrap();
        for operation in 0..4 {
            for cancelled in [false, true] {
                let context = bounded_context();
                let before = scope.binding().unwrap();
                let guard = scope.inner.directories.lock().unwrap();
                std::thread::scope(|threads| {
                    let (entered, waiting) = std::sync::mpsc::channel();
                    let (scope, handle, record, context) = (&scope, &handle, &record, &context);
                    let worker = threads.spawn(move || {
                        BEFORE_SCOPE_LOCK.with(|slot| {
                            *slot.borrow_mut() = Some(Box::new(move || entered.send(()).unwrap()));
                        });
                        match operation {
                            0 => record
                                .matches_scope_bounded(scope, context)
                                .map(|matched| assert!(matched)),
                            1 => scope.revalidate_bounded(context),
                            2 => scope.revalidate_installation(context),
                            _ => scope.recheck_runtime_installation(
                                scope.owner_root(),
                                handle,
                                scope.brand(),
                                context,
                            ),
                        }
                    });
                    waiting.recv().unwrap();
                    if cancelled {
                        context.cancellation.cancel();
                    }
                    drop(guard);
                    let result = worker.join().unwrap();
                    if cancelled {
                        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
                    } else {
                        result.unwrap();
                    }
                });
                assert!(scope.binding().unwrap() == before);
            }
        }
    }
    #[test]
    fn runtime_scope_rechecks_deadline_after_waiting_for_observations() {
        let (_temp, scope, handle) = runtime_scope_fixture();
        let before = scope.binding().unwrap();
        let context = ScopeContext::new(
            Instant::now() + Duration::from_millis(200),
            Cancellation::default(),
        );
        let guard = scope.inner.directories.lock().unwrap();
        std::thread::scope(|threads| {
            let (entered, waiting) = std::sync::mpsc::channel();
            let (scope, handle, context) = (&scope, &handle, &context);
            let worker = threads.spawn(move || {
                BEFORE_SCOPE_LOCK.with(|slot| {
                    *slot.borrow_mut() = Some(Box::new(move || entered.send(()).unwrap()));
                });
                scope.recheck_runtime_installation(
                    scope.owner_root(),
                    handle,
                    scope.brand(),
                    context,
                )
            });
            waiting.recv().unwrap();
            std::thread::sleep(
                context
                    .deadline
                    .unwrap()
                    .saturating_duration_since(Instant::now()),
            );
            drop(guard);
            assert_eq!(
                worker.join().unwrap().unwrap_err().kind(),
                io::ErrorKind::TimedOut
            );
        });
        assert!(scope.binding().unwrap() == before);
    }
    #[test]
    fn runtime_scope_denies_project_owner_and_different_physical_handle() {
        let (temp, scope, handle) = runtime_scope_fixture();
        let context = bounded_context();
        scope
            .recheck_runtime_installation(scope.owner_root(), &handle, scope.brand(), &context)
            .unwrap();
        let other = graphit_fs::create_private_directory(&temp.path().join("other")).unwrap();
        assert_eq!(
            scope
                .recheck_runtime_installation(scope.owner_root(), &other, scope.brand(), &context)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        let project_root = temp.path().join("project");
        let project_handle = graphit_fs::create_private_directory(&project_root).unwrap();
        drop(graphit_fs::create_private_directory(&project_root.join(".graphit")).unwrap());
        let installation = InstallationPaths::resolve(
            Brand::new("graphit").unwrap(),
            temp.path(),
            None,
            Some(scope.owner_root()),
        )
        .unwrap();
        let project = ProjectPaths::at(&project_root, &installation).unwrap();
        let project_scope = CapturedPublicationScope::project(
            &project,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            scope.inner.limits,
            &native_capture_request(),
        )
        .unwrap();
        assert_eq!(
            project_scope
                .recheck_runtime_installation(
                    project_scope.owner_root(),
                    &project_handle,
                    project_scope.brand(),
                    &context
                )
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
    #[test]
    fn runtime_scope_caps_and_cancel_during_inspection_leave_captured_records() {
        let (_temp, mut scope, handle) = runtime_scope_fixture();
        let context = bounded_context();
        let old = scope.binding().unwrap();
        let original = scope.inner.limits;
        for limits in [
            ScopeLimits::new(8, 1, 256).unwrap(),
            ScopeLimits::new(8, 65536, 1).unwrap(),
        ] {
            Arc::get_mut(&mut scope.inner)
                .expect("unique scope fixture")
                .limits = limits;
            assert_eq!(
                scope.revalidate_bounded(&context).unwrap_err().kind(),
                io::ErrorKind::FileTooLarge
            );
        }
        Arc::get_mut(&mut scope.inner)
            .expect("unique scope fixture")
            .limits = original;
        Arc::get_mut(&mut scope.inner)
            .expect("unique scope fixture")
            .limits
            .roots = 1;
        assert_eq!(
            scope.revalidate_bounded(&context).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        Arc::get_mut(&mut scope.inner)
            .expect("unique scope fixture")
            .limits = original;
        assert!(old.matches_scope(&scope));
        let cancelled = bounded_context();
        let cancellation = cancelled.cancellation.clone();
        INSPECTION_STEP
            .with(|hook| *hook.borrow_mut() = Some(Box::new(move || cancellation.cancel())));
        assert_eq!(
            scope
                .recheck_runtime_installation(
                    scope.owner_root(),
                    &handle,
                    scope.brand(),
                    &cancelled
                )
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
        assert!(old.matches_scope(&scope));
        let expired = ScopeContext::new(Instant::now(), Cancellation::default());
        assert_eq!(
            scope.revalidate_bounded(&expired).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        scope.revalidate_bounded(&context).unwrap();
    }
}
