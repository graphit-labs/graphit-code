//! Local maintained projects and versions. A snapshot is history, never online presence.
//!
//! ```
//! use graphit_core::{project_inventory::ProjectInventorySnapshot, project_identity::ProjectId};
//! fn versions(snapshot: &ProjectInventorySnapshot, id: ProjectId) -> Vec<&str> {
//!     snapshot.get(id).map(|p| p.versions().collect()).unwrap_or_default()
//! }
//! ```
use crate::{
    artifact_store::{CapturedPublicationScope, ScopeLimits, StoreOwner},
    auth_document::AuthDocumentError,
    file_io::SnapshotContext,
    file_lock::FileLock,
    git_checkout::GitCheckout,
    installation_bootstrap::PrivateInstallation,
    local_files::{
        atomic_replace_with, directory_exists, open_private_directory, read_regular,
        regular_metadata,
    },
    native_document_file::{CapturedProjectDocumentDirectory, NativeDocumentFileLimits},
    native_path::NativePath,
    project_identity::{ProjectId, ProjectIdentity, ProjectIdentityStore},
    project_paths::{InstallationPaths, ProjectPaths},
    project_version,
    version_context::{Checkout, VersionContext, VersionSource},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

const DOCUMENT: &str = "project-inventory.json";
const LOCK: &str = ".project-inventory.lock";

/// Pure historical data; decoded paths confer no filesystem access.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectInventorySnapshot {
    schema: u8,
    brand: String,
    projects: Vec<ProjectInventoryEntry>,
}
impl ProjectInventorySnapshot {
    fn empty(brand: &str) -> Self {
        Self {
            schema: 1,
            brand: brand.to_owned(),
            projects: Vec::new(),
        }
    }
    pub fn projects(&self) -> impl ExactSizeIterator<Item = &ProjectInventoryEntry> {
        self.projects.iter()
    }
    pub fn get(&self, id: ProjectId) -> Option<&ProjectInventoryEntry> {
        self.projects.iter().find(|p| p.identity.id() == id)
    }
    fn validate(&self) -> io::Result<()> {
        if self.schema != 1 {
            return Err(invalid_data());
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut roots = std::collections::BTreeSet::new();
        for project in &self.projects {
            if !project.identity.allows_persistent_stores()
                || !ids.insert(project.identity.id())
                || (project.checkouts.is_empty() && project.versions.is_empty())
            {
                return Err(invalid_data());
            }
            let mut versions = std::collections::BTreeSet::new();
            for v in &project.versions {
                canonical_version(v).map_err(|_| invalid_data())?;
                if !versions.insert(v) {
                    return Err(invalid_data());
                }
            }
            for checkout in &project.checkouts {
                canonical_version(&checkout.selected_version).map_err(|_| invalid_data())?;
                canonical_version(&checkout.default_version).map_err(|_| invalid_data())?;
                if !versions.contains(&checkout.selected_version)
                    || !roots.insert(checkout.path.as_path())
                {
                    return Err(invalid_data());
                }
                match checkout.source {
                    VersionSource::Checkout if !matches!(&checkout.checkout, CheckoutState::Branch { name } if name == &checkout.selected_version) =>
                    {
                        return Err(invalid_data());
                    }
                    VersionSource::DetachedCommit
                        if !matches!(checkout.checkout, CheckoutState::Detached)
                            || !checkout.selected_version.strip_prefix("git:").is_some_and(
                                |commit| {
                                    crate::git_record_event::validate_commit_id(commit).is_ok()
                                },
                            ) =>
                    {
                        return Err(invalid_data());
                    }
                    VersionSource::Default
                        if checkout.selected_version != checkout.default_version
                            || matches!(checkout.checkout, CheckoutState::Branch { .. }) =>
                    {
                        return Err(invalid_data());
                    }
                    _ => (),
                }
                if let CheckoutState::Branch { name } = &checkout.checkout {
                    canonical_version(name).map_err(|_| invalid_data())?;
                }
            }
        }
        Ok(())
    }
    fn sort(&mut self) {
        self.projects.sort_by_key(|p| p.identity.id());
        for project in &mut self.projects {
            project
                .checkouts
                .sort_by(|a, b| a.path.as_path().cmp(b.path.as_path()));
            project.versions.sort();
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectInventoryEntry {
    identity: ProjectIdentity,
    versions: Vec<String>,
    checkouts: Vec<CheckoutInventory>,
}
impl ProjectInventoryEntry {
    pub fn identity(&self) -> &ProjectIdentity {
        &self.identity
    }
    pub fn versions(&self) -> impl ExactSizeIterator<Item = &str> {
        self.versions.iter().map(String::as_str)
    }
    pub fn checkouts(&self) -> impl ExactSizeIterator<Item = &CheckoutInventory> {
        self.checkouts.iter()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum CheckoutState {
    NonGit,
    Branch { name: String },
    Detached,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckoutInventory {
    path: NativePath,
    selected_version: String,
    default_version: String,
    source: VersionSource,
    checkout: CheckoutState,
}
impl CheckoutInventory {
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
    pub fn selected_version(&self) -> &str {
        &self.selected_version
    }
    pub fn default_version(&self) -> &str {
        &self.default_version
    }
    pub fn source(&self) -> VersionSource {
        self.source
    }
    pub fn checkout(&self) -> &CheckoutState {
        &self.checkout
    }
}

/// A short-lived original-request observation. No decoding restores this capture.
pub struct CapturedProjectInventory {
    project: ProjectPaths,
    identity: ProjectIdentity,
    checkout: CheckoutInventory,
    directory: CapturedProjectDocumentDirectory,
}
impl CapturedProjectInventory {
    pub fn capture(
        project: &ProjectPaths,
        checkout: &GitCheckout,
        selection: &VersionContext,
        scope_limits: ScopeLimits,
        native_limits: NativeDocumentFileLimits,
        request: &Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        request.check()?;
        if !selection.belongs_to(project) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let identity = ProjectIdentityStore::at(project)
            .load()?
            .ok_or(io::ErrorKind::NotFound)?;
        if !identity.allows_persistent_stores() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let scope =
            CapturedPublicationScope::project(project, identity.id(), scope_limits, request)?;
        let directory = CapturedProjectDocumentDirectory::capture(
            &scope,
            Path::new(""),
            native_limits,
            request,
        )?;
        Self::finish_capture(project, checkout, selection, identity, directory, request)
    }

    /// Observe an inventory identity through an already admitted PROJECT scope.
    ///
    /// The scope and request must be the original objects. The loaded identity is
    /// checked against the scope owner; it never supplies a replacement scope.
    /// This observation does not make dataset and inventory writes atomic.
    ///
    /// ```
    /// use graphit_core::{
    ///     artifact_store::CapturedPublicationScope, file_io::SnapshotContext,
    ///     git_checkout::GitCheckout, native_document_file::NativeDocumentFileLimits,
    ///     project_inventory::CapturedProjectInventory, project_paths::ProjectPaths,
    ///     version_context::VersionContext,
    /// };
    /// use std::{io, sync::Arc};
    /// fn capture(
    ///     project: &ProjectPaths, checkout: &GitCheckout, selection: &VersionContext,
    ///     scope: &CapturedPublicationScope, limits: NativeDocumentFileLimits,
    ///     original: &Arc<SnapshotContext>,
    /// ) -> io::Result<CapturedProjectInventory> {
    ///     CapturedProjectInventory::capture_in_scope(
    ///         project, checkout, selection, scope, limits, original,
    ///     )
    /// }
    /// ```
    pub fn capture_in_scope(
        project: &ProjectPaths,
        checkout: &GitCheckout,
        selection: &VersionContext,
        scope: &CapturedPublicationScope,
        native_limits: NativeDocumentFileLimits,
        request: &Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        request.check()?;
        let StoreOwner::Project { project_id } = scope.owner() else {
            return Err(io::ErrorKind::InvalidInput.into());
        };
        if !selection.belongs_to(project)
            || scope.owner_root() != project.root()
            || scope.brand().key() != project.brand().key()
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let directory = CapturedProjectDocumentDirectory::capture(
            scope,
            Path::new(""),
            native_limits,
            request,
        )?;
        let identity = ProjectIdentityStore::at(project)
            .load()?
            .ok_or(io::ErrorKind::NotFound)?;
        if identity.id() != *project_id {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if !identity.allows_persistent_stores() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Self::finish_capture(project, checkout, selection, identity, directory, request)
    }

    fn finish_capture(
        project: &ProjectPaths,
        checkout: &GitCheckout,
        selection: &VersionContext,
        identity: ProjectIdentity,
        directory: CapturedProjectDocumentDirectory,
        request: &SnapshotContext,
    ) -> io::Result<Self> {
        let (state, default) = match checkout.as_checkout() {
            Checkout::NonGit => (CheckoutState::NonGit, project_version::DEFAULT_NAME),
            Checkout::Branch { name, default } => (
                CheckoutState::Branch {
                    name: name.to_owned(),
                },
                default,
            ),
            Checkout::Detached { default, .. } => (CheckoutState::Detached, default),
        };
        if default != selection.default_version()
            || selection.is_git() != !matches!(state, CheckoutState::NonGit)
            || selection.is_detached() != matches!(state, CheckoutState::Detached)
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let captured = Self {
            project: project.clone(),
            identity,
            checkout: CheckoutInventory {
                path: NativePath::new(project.root().to_owned())?,
                selected_version: canonical_version(selection.version())?.to_owned(),
                default_version: canonical_version(default)?.to_owned(),
                source: selection.source(),
                checkout: state,
            },
            directory,
        };
        captured.revalidate(request)?;
        Ok(captured)
    }
    fn revalidate(&self, request: &SnapshotContext) -> io::Result<()> {
        self.directory.revalidate(request)?;
        let current = ProjectIdentityStore::at(&self.project)
            .load()?
            .ok_or(io::ErrorKind::NotFound)?;
        if current != self.identity {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.directory.revalidate(request)
    }
}

/// Explicit installation namespace. Reads never create directories or identities.
#[derive(Clone)]
pub struct GlobalProjectInventory {
    installation: InstallationPaths,
    max_document_bytes: usize,
}
impl GlobalProjectInventory {
    pub fn at(installation: &InstallationPaths, max_document_bytes: usize) -> io::Result<Self> {
        max_document_bytes
            .checked_add(1)
            .filter(|n| *n > 1)
            .and_then(|n| u64::try_from(n).ok())
            .ok_or(io::ErrorKind::InvalidInput)?;
        Ok(Self {
            installation: installation.clone(),
            max_document_bytes,
        })
    }
    fn path(&self) -> PathBuf {
        self.installation.root().join(DOCUMENT)
    }
    pub fn load(&self, request: &SnapshotContext) -> io::Result<ProjectInventorySnapshot> {
        request.check()?;
        if !directory_exists(self.installation.root())? {
            return Ok(ProjectInventorySnapshot::empty(
                self.installation.brand().key(),
            ));
        }
        let _root = open_private_directory(self.installation.root())?;
        let Some((bytes, permissions)) = read_regular(&self.path(), self.max_document_bytes)?
        else {
            request.check()?;
            return Ok(ProjectInventorySnapshot::empty(
                self.installation.brand().key(),
            ));
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if permissions.mode() & 0o077 != 0 {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
        }
        #[cfg(not(unix))]
        let _ = permissions;
        let mut snapshot: ProjectInventorySnapshot =
            serde_json::from_slice(&bytes).map_err(|_| invalid_data())?;
        if snapshot.brand != self.installation.brand().key() {
            return Err(invalid_data());
        }
        snapshot.validate()?;
        snapshot.sort();
        request.check()?;
        Ok(snapshot)
    }
    pub fn writer(
        &self,
        installation: PrivateInstallation,
        request: &Arc<SnapshotContext>,
    ) -> io::Result<ProjectInventoryWriter> {
        request.check()?;
        if installation.paths().root() != self.installation.root()
            || installation.paths().brand().key() != self.installation.brand().key()
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        installation
            .revalidate(request)
            .map_err(installation_error)?;
        request.check()?;
        regular_metadata(&self.installation.root().join(LOCK))?;
        let lock = FileLock::acquire_in_existing_parent_until(
            &self.installation.root().join(LOCK),
            request.deadline(),
            request.cancellation(),
        )?;
        installation
            .revalidate(request)
            .map_err(installation_error)?;
        Ok(ProjectInventoryWriter {
            store: self.clone(),
            installation,
            request: request.clone(),
            _lock: lock,
        })
    }
}

/// Cooperative mutations are serialized by a stable kernel lock for this lifetime.
pub struct ProjectInventoryWriter {
    store: GlobalProjectInventory,
    installation: PrivateInstallation,
    request: Arc<SnapshotContext>,
    _lock: FileLock,
}
impl ProjectInventoryWriter {
    fn check(&self) -> io::Result<()> {
        self.request.check()?;
        self.installation
            .revalidate(&self.request)
            .map_err(installation_error)
    }
    fn check_capture(&self, capture: &CapturedProjectInventory) -> io::Result<()> {
        if capture.project.brand().key() != self.store.installation.brand().key() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        capture.revalidate(&self.request)
    }
    pub fn register_checkout(&mut self, capture: &CapturedProjectInventory) -> io::Result<bool> {
        self.check_capture(capture)?;
        self.mutate(
            |snapshot| {
                if snapshot.projects.iter().any(|p| {
                    p.identity.id() != capture.identity.id()
                        && p.checkouts.iter().any(|c| c.path == capture.checkout.path)
                }) {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                let (project, mut changed) = ensure_project(snapshot, &capture.identity);
                if let Some(existing) = project
                    .checkouts
                    .iter_mut()
                    .find(|c| c.path == capture.checkout.path)
                {
                    if existing != &capture.checkout {
                        *existing = capture.checkout.clone();
                        changed = true;
                    }
                } else {
                    project.checkouts.push(capture.checkout.clone());
                    changed = true;
                }
                Ok(retain(project, &capture.checkout.selected_version) || changed)
            },
            || capture.revalidate(&self.request),
        )
    }
    pub fn retain_version(
        &mut self,
        capture: &CapturedProjectInventory,
        version: &str,
    ) -> io::Result<bool> {
        let version = canonical_version(version)?;
        if version.len() > self.store.max_document_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        self.check_capture(capture)?;
        self.mutate(
            |snapshot| {
                let (project, changed) = ensure_project(snapshot, &capture.identity);
                Ok(retain(project, version) || changed)
            },
            || capture.revalidate(&self.request),
        )
    }
    pub fn remove_checkout(&mut self, expected: ProjectId, checkout: &Path) -> io::Result<bool> {
        let path = NativePath::new(checkout.to_owned())?;
        self.mutate(
            |snapshot| {
                let Some(project) = snapshot
                    .projects
                    .iter_mut()
                    .find(|p| p.identity.id() == expected)
                else {
                    return Ok(false);
                };
                let before = project.checkouts.len();
                project.checkouts.retain(|c| c.path != path);
                let changed = before != project.checkouts.len();
                prune(snapshot);
                Ok(changed)
            },
            || Ok(()),
        )
    }
    pub fn remove_version(&mut self, expected: ProjectId, version: &str) -> io::Result<bool> {
        let version = canonical_version(version)?;
        self.mutate(
            |snapshot| {
                let Some(project) = snapshot
                    .projects
                    .iter_mut()
                    .find(|p| p.identity.id() == expected)
                else {
                    return Ok(false);
                };
                if project
                    .checkouts
                    .iter()
                    .any(|c| c.selected_version == version)
                {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                let before = project.versions.len();
                project.versions.retain(|v| v != version);
                let changed = before != project.versions.len();
                prune(snapshot);
                Ok(changed)
            },
            || Ok(()),
        )
    }
    fn mutate(
        &self,
        mutate: impl FnOnce(&mut ProjectInventorySnapshot) -> io::Result<bool>,
        before_publish: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<bool> {
        self.check()?;
        let mut desired = self.store.load(&self.request)?;
        if !mutate(&mut desired)? {
            return Ok(false);
        }
        desired.sort();
        desired.validate()?;
        before_publish()?;
        self.check()?;
        #[cfg(unix)]
        let permissions = {
            use std::os::unix::fs::PermissionsExt;
            Some(fs::Permissions::from_mode(0o600))
        };
        #[cfg(not(unix))]
        let permissions = None;
        atomic_replace_with(
            &self.store.path(),
            permissions,
            self.request.cancellation(),
            |file| {
                self.check()?;
                let mut output = BoundedOutput {
                    file,
                    remaining: self.store.max_document_bytes,
                };
                serde_json::to_writer(&mut output, &desired).map_err(|e| {
                    io::Error::new(e.io_error_kind().unwrap_or(io::ErrorKind::InvalidData), e)
                })?;
                self.check()
            },
        )?;
        Ok(true)
    }
}
fn ensure_project<'a>(
    snapshot: &'a mut ProjectInventorySnapshot,
    identity: &ProjectIdentity,
) -> (&'a mut ProjectInventoryEntry, bool) {
    if let Some(index) = snapshot
        .projects
        .iter()
        .position(|p| p.identity.id() == identity.id())
    {
        let project = &mut snapshot.projects[index];
        let changed = &project.identity != identity;
        if changed {
            project.identity = identity.clone();
        }
        return (project, changed);
    }
    snapshot.projects.push(ProjectInventoryEntry {
        identity: identity.clone(),
        versions: Vec::new(),
        checkouts: Vec::new(),
    });
    (
        snapshot.projects.last_mut().expect("just inserted project"),
        true,
    )
}
fn retain(project: &mut ProjectInventoryEntry, version: &str) -> bool {
    if project.versions.iter().any(|v| v == version) {
        return false;
    }
    project.versions.push(version.to_owned());
    true
}
fn prune(snapshot: &mut ProjectInventorySnapshot) {
    snapshot
        .projects
        .retain(|p| !p.checkouts.is_empty() || !p.versions.is_empty());
}
fn canonical_version(version: &str) -> io::Result<&str> {
    let normalized = project_version::validate(version).map_err(|_| io::ErrorKind::InvalidInput)?;
    if normalized.is_empty() || normalized != version {
        Err(io::ErrorKind::InvalidInput.into())
    } else {
        Ok(version)
    }
}
fn invalid_data() -> io::Error {
    io::ErrorKind::InvalidData.into()
}
fn installation_error(error: AuthDocumentError) -> io::Error {
    use AuthDocumentError::*;
    io::Error::new(
        match error {
            InvalidPolicy => io::ErrorKind::InvalidInput,
            Denied => io::ErrorKind::PermissionDenied,
            Conflict => io::ErrorKind::InvalidData,
            Limit => io::ErrorKind::FileTooLarge,
            Cancelled => io::ErrorKind::Interrupted,
            Deadline => io::ErrorKind::TimedOut,
            Unavailable => io::ErrorKind::NotFound,
            Resource => io::ErrorKind::OutOfMemory,
            Io => io::ErrorKind::Other,
        },
        error,
    )
}
struct BoundedOutput<'a> {
    file: &'a mut fs::File,
    remaining: usize,
}
impl Write for BoundedOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.remaining {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let written = self.file.write(bytes)?;
        self.remaining -= written;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
#[cfg(test)]
mod tests;
