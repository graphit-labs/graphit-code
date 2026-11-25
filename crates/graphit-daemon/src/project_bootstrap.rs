//! Observe a real checkout and register its selected version in the Core inventory.
//! The result contains data, never filesystem or supervisor authority.
//!
//! ```
//! use graphit_core::version_context::VersionContext;
//! fn selected(result: &VersionContext) -> &str { result.version() }
//! ```
use graphit_core::{
    file_io::SnapshotContext,
    git_checkout::GitCheckout,
    project_identity::ProjectIdentityStore,
    project_inventory::{GlobalProjectInventory, ProjectInventoryObservation},
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

/// Preserve both the failing phase and its original source for local diagnostics.
#[derive(Debug)]
pub enum ProjectBootstrapError {
    Request(io::Error),
    Checkout(io::Error),
    Selection(io::Error),
    Identity(io::Error),
    Inventory(io::Error),
}
impl fmt::Display for ProjectBootstrapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(error) => write!(formatter, "project bootstrap request: {error}"),
            Self::Checkout(error) => write!(formatter, "project bootstrap checkout: {error}"),
            Self::Selection(error) => write!(formatter, "project bootstrap selection: {error}"),
            Self::Identity(error) => write!(formatter, "project bootstrap identity: {error}"),
            Self::Inventory(error) => write!(formatter, "project bootstrap inventory: {error}"),
        }
    }
}
impl Error for ProjectBootstrapError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Request(error)
            | Self::Checkout(error)
            | Self::Selection(error)
            | Self::Identity(error)
            | Self::Inventory(error) => error,
        })
    }
}

/// Observe an existing project identity and register its selected version.
/// This observes an already changed checkout/selection; it does not switch Git.
pub fn bootstrap_project(
    input: ProjectBootstrapInput<'_>,
    store: &GlobalProjectInventory,
    request: &Arc<SnapshotContext>,
) -> Result<VersionContext, ProjectBootstrapError> {
    use ProjectBootstrapError as Failure;
    let operation_deadline = request.required_deadline().map_err(Failure::Request)?;
    let expected = ProjectIdentityStore::at(input.project)
        .load()
        .map_err(Failure::Identity)?
        .ok_or_else(|| Failure::Identity(io::ErrorKind::NotFound.into()))?
        .id();
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
    let observation =
        ProjectInventoryObservation::observe(input.project, expected, &checkout, &version, request)
            .map_err(Failure::Identity)?;
    {
        let mut writer = store.writer(request).map_err(Failure::Inventory)?;
        writer
            .register_checkout(&observation)
            .map_err(Failure::Inventory)?;
    }
    // Core checks before its atomic commit. Do not turn a confirmed commit into
    // an ambiguous failure with a fresh request check after publication.
    Ok(version)
}

#[cfg(test)]
mod tests;
