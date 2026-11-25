//! Owned historical inventory facts from the running installation.
//!
//! This reader confers no authority and does not report online presence or selection.
//! ```
//! # #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
//! # fn example(service: &graphit_daemon::auth_startup::DaemonAuth,
//! # original: &graphit_core::file_io::SnapshotContext) -> std::io::Result<()> {
//! use graphit_daemon::maintained_inventory::read_maintained_inventory;
//! let snapshot = read_maintained_inventory(service, original, 65536)?;
//! for project in snapshot.projects {
//!     let _historical_versions = (project.project_id, project.versions);
//! }
//! # Ok(())
//! # }
//! ```

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use crate::auth_startup::DaemonAuth;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use graphit_core::{
    artifact_store::StoreOwner, file_io::SnapshotContext, installation_identity::InstallationId,
    project_identity::ProjectId, project_inventory::GlobalProjectInventory,
    project_paths::InstallationPaths,
};
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use std::io;

/// The live service's logical installation identity and maintained project facts.
/// The identity is not reloaded from disk and the result retains no service resources.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaintainedInventorySnapshot {
    pub installation_id: InstallationId,
    pub projects: Vec<MaintainedProjectVersions>,
}

/// Maintained versions are historical facts, not checkout selection or activity.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaintainedProjectVersions {
    pub project_id: ProjectId,
    pub versions: Vec<String>,
}

/// Read existing Core inventory at the installation configured by `service`.
///
/// Exact inventory absence returns empty projects.
/// All request, document and I/O failures remain errors. Synchronous
/// filesystem I/O is checked cooperatively; this is not an atomic filesystem view.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub fn read_maintained_inventory(
    service: &DaemonAuth,
    original: &SnapshotContext,
    max_document_bytes: usize,
) -> io::Result<MaintainedInventorySnapshot> {
    original.check()?;
    let scope = service.installation();
    let StoreOwner::Installation { installation_id } = scope.owner() else {
        return Err(io::ErrorKind::InvalidData.into());
    };
    let installation_id = installation_id.clone();
    let projects = {
        let root = scope.owner_root();
        let paths = InstallationPaths::resolve(scope.brand().clone(), root, None, Some(root))?;
        let snapshot = GlobalProjectInventory::at(&paths, max_document_bytes)?.load(original)?;
        let mut projects = Vec::with_capacity(snapshot.projects().len());
        for entry in snapshot.projects() {
            original.check()?;
            let mut versions = Vec::with_capacity(entry.versions().len());
            for version in entry.versions() {
                original.check()?;
                versions.push(version.to_owned());
            }
            projects.push(MaintainedProjectVersions {
                project_id: entry.identity().id(),
                versions,
            });
        }
        projects
    };
    original.check()?;
    Ok(MaintainedInventorySnapshot {
        installation_id,
        projects,
    })
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
