//! Observe a real checkout and register its selected version in the Core inventory.
//! The result contains data, never filesystem or supervisor authority.
//!
//! ```
//! use graphit_daemon::project_bootstrap::{BootstrappedProject, InventoryRegistration};
//! fn selected(result: &BootstrappedProject) -> (&str, InventoryRegistration) {
//!     (result.version().version(), result.disposition())
//! }
//! ```
use graphit_core::{
    artifact_store::ScopeLimits,
    auth_document::AuthDocumentError,
    file_io::SnapshotContext,
    git_checkout::GitCheckout,
    installation_bootstrap::PrivateInstallation,
    native_document_file::NativeDocumentFileLimits,
    project_inventory::{CapturedProjectInventory, GlobalProjectInventory},
    project_paths::ProjectPaths,
    version_context::{ScopedPin, VersionContext},
};
use std::{error::Error, fmt, io, sync::Arc, time::Instant};

pub struct ProjectBootstrapInput<'a> {
    pub project: &'a ProjectPaths,
    pub explicit_version: &'a str,
    pub pin: Option<&'a ScopedPin>,
    pub detached_base: &'a str,
}

pub struct ProjectBootstrapLimits {
    pub scope: ScopeLimits,
    pub native: NativeDocumentFileLimits,
}

/// The actual Core mutation outcome, not a permission or presence signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InventoryRegistration {
    Registered,
    Unchanged,
}

pub struct BootstrappedProject {
    version: VersionContext,
    checkout: GitCheckout,
    disposition: InventoryRegistration,
}
impl BootstrappedProject {
    pub fn version(&self) -> &VersionContext {
        &self.version
    }
    pub fn checkout(&self) -> &GitCheckout {
        &self.checkout
    }
    pub fn disposition(&self) -> InventoryRegistration {
        self.disposition
    }
}

/// Preserve both the failing phase and its original source for local diagnostics.
#[derive(Debug)]
pub enum ProjectBootstrapError {
    Request(io::Error),
    Installation(AuthDocumentError),
    Checkout(io::Error),
    Selection(io::Error),
    Capture(io::Error),
    Inventory(io::Error),
}
impl fmt::Display for ProjectBootstrapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(error) => write!(formatter, "project bootstrap request: {error}"),
            Self::Installation(error) => {
                write!(formatter, "project bootstrap installation: {error}")
            }
            Self::Checkout(error) => write!(formatter, "project bootstrap checkout: {error}"),
            Self::Selection(error) => write!(formatter, "project bootstrap selection: {error}"),
            Self::Capture(error) => write!(formatter, "project bootstrap capture: {error}"),
            Self::Inventory(error) => write!(formatter, "project bootstrap inventory: {error}"),
        }
    }
}
impl Error for ProjectBootstrapError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Installation(error) => error,
            Self::Request(error)
            | Self::Checkout(error)
            | Self::Selection(error)
            | Self::Capture(error)
            | Self::Inventory(error) => error,
        })
    }
}

/// Use an existing identity and prepared installation bound to this exact request.
/// This observes an already changed checkout/selection; it does not switch Git.
pub fn bootstrap_project(
    input: ProjectBootstrapInput<'_>,
    limits: ProjectBootstrapLimits,
    store: &GlobalProjectInventory,
    installation: PrivateInstallation,
    request: &Arc<SnapshotContext>,
) -> Result<BootstrappedProject, ProjectBootstrapError> {
    use ProjectBootstrapError as Failure;
    let operation_deadline = request.required_deadline().map_err(Failure::Request)?;
    installation
        .revalidate(request)
        .map_err(Failure::Installation)?;
    if installation.paths().brand().key() != input.project.brand().key() {
        return Err(Failure::Capture(io::Error::new(
            io::ErrorKind::InvalidInput,
            "installation and project brands differ",
        )));
    }
    request.check().map_err(Failure::Request)?;
    let remaining = operation_deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(|| Failure::Request(io::ErrorKind::TimedOut.into()))?;
    let observed = GitCheckout::inspect(input.project.root(), remaining, request.cancellation());
    // Fence the original request even when the relative Git SDK budget fails.
    request.check().map_err(Failure::Request)?;
    let checkout = observed.map_err(Failure::Checkout)?;
    let version = VersionContext::select(
        input.project,
        checkout.as_checkout(),
        input.explicit_version,
        input.pin,
        input.detached_base,
    )
    .map_err(Failure::Selection)?;
    let capture = CapturedProjectInventory::capture(
        input.project,
        &checkout,
        &version,
        limits.scope,
        limits.native,
        request,
    )
    .map_err(Failure::Capture)?;
    let changed = {
        let mut writer = store
            .writer(installation, request)
            .map_err(Failure::Inventory)?;
        writer
            .register_checkout(&capture)
            .map_err(Failure::Inventory)?
    };
    // Core checks before its atomic commit. Do not turn a confirmed commit into
    // an ambiguous failure with a fresh request check after publication.
    Ok(BootstrappedProject {
        version,
        checkout,
        disposition: if changed {
            InventoryRegistration::Registered
        } else {
            InventoryRegistration::Unchanged
        },
    })
}

#[cfg(test)]
mod tests;
