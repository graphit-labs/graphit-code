//! Explicit local checkout retirement after its actual supervisors have joined.
//! Retained versions and project files are untouched; UI/HTTP routing is separate.
#![cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use graphit_auth::{AuthError, AuthenticatedCaller, Obligation, Permission};
use graphit_core::{
    artifact_store::ScopeContext, auth_document::AuthDocumentError,
    installation_bootstrap::PrivateInstallation, project_identity::ProjectId,
    project_inventory::GlobalProjectInventory,
};
use graphit_daemon::{auth_startup::DaemonAuth, supervisor::ProjectSupervisors};
use graphit_hub::policy::RequestContext;
use std::{error::Error, fmt, io, path::Path};

/// Incoming credentials are authenticated on the operation's Original.
/// Selecting an origin is not authority, and bearer values are never formatted.
pub enum CheckoutCredential<'a> {
    Runtime(&'a str),
    Local(&'a str),
}

pub struct CheckoutRetirementInput<'a> {
    pub credential: CheckoutCredential<'a>,
    pub installation: PrivateInstallation,
    pub expected: ProjectId,
    /// Exact historical native path; it may no longer exist physically.
    pub checkout: &'a Path,
}

/// The failing phase preserves its cause; stopped owners are never rolled back.
#[derive(Debug)]
pub enum CheckoutRetirementError {
    Admission(AuthError),
    Request(io::Error),
    Installation(AuthDocumentError),
    Scope(io::Error),
    Stop(io::Error),
    Inventory(io::Error),
}
impl fmt::Display for CheckoutRetirementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Admission(_) => "checkout retirement admission failed",
            Self::Request(_) => "checkout retirement request failed",
            Self::Installation(_) => "checkout retirement installation failed",
            Self::Scope(_) => "checkout retirement namespace failed",
            Self::Stop(_) => "checkout retirement cleanup failed",
            Self::Inventory(_) => "checkout retirement inventory failed",
        })
    }
}
impl Error for CheckoutRetirementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Admission(error) => error,
            Self::Installation(error) => error,
            Self::Request(error)
            | Self::Scope(error)
            | Self::Stop(error)
            | Self::Inventory(error) => error,
        })
    }
}

fn authorize(caller: &AuthenticatedCaller<'_>) -> Result<(), CheckoutRetirementError> {
    if caller
        .authorize(Permission::Configure)
        .map_err(CheckoutRetirementError::Admission)?
        != Obligation::Unconditional
    {
        return Err(CheckoutRetirementError::Admission(AuthError::Denied));
    }
    Ok(())
}

fn recheck_installation(
    service: &DaemonAuth,
    installation: &PrivateInstallation,
    original: &RequestContext<'_>,
) -> Result<(), CheckoutRetirementError> {
    use CheckoutRetirementError as Failure;
    let request = original.snapshot();
    request.check().map_err(Failure::Request)?;
    installation
        .revalidate(request)
        .map_err(Failure::Installation)?;
    let capture = service.installation();
    if installation.paths().root() != capture.live.owner_root()
        || installation.paths().brand().key() != capture.live.brand().key()
    {
        return Err(Failure::Scope(io::ErrorKind::InvalidInput.into()));
    }
    let context = ScopeContext::from_snapshot(request);
    capture
        .live
        .revalidate_installation(&context)
        .map_err(Failure::Scope)?;
    if !capture
        .record
        .matches_scope_bounded(capture.live, &context)
        .map_err(Failure::Scope)?
    {
        return Err(Failure::Scope(io::ErrorKind::InvalidData.into()));
    }
    request.check().map_err(Failure::Request)
}

/// Forget only this explicit historical checkout after all matching owners join.
///
/// Configure must be explicitly allowed for the authenticated Runtime/Local origin.
/// Missing metadata returns false without creating a checkout or identity. A missing
/// physical checkout can still be removed using its recorded native path. Parking,
/// switching versions or service shutdown do not call this operation.
///
/// Cancellation or expiry during cleanup does not abort joins. The subsequent
/// Original check prevents metadata publication. A stop or inventory error does
/// not restart stopped owners; no transaction spans supervision and the registry.
/// Await this future to acknowledge cleanup. File locks are acquired only after joins.
pub async fn retire_checkout(
    service: &DaemonAuth,
    input: CheckoutRetirementInput<'_>,
    supervisors: &mut ProjectSupervisors,
    inventory_bytes: usize,
    original: &RequestContext<'_>,
) -> Result<bool, CheckoutRetirementError> {
    use CheckoutRetirementError as Failure;
    original.snapshot().check().map_err(Failure::Request)?;
    let caller = match input.credential {
        CheckoutCredential::Runtime(raw) => service.issuer().authenticate_runtime(raw, original),
        CheckoutCredential::Local(raw) => service.issuer().authenticate_local(raw, original),
    }
    .map_err(Failure::Admission)?;
    authorize(&caller)?;
    recheck_installation(service, &input.installation, original)?;
    let store = GlobalProjectInventory::at(input.installation.paths(), inventory_bytes)
        .map_err(Failure::Inventory)?;
    let snapshot = store
        .load(original.snapshot())
        .map_err(Failure::Inventory)?;
    let exists = snapshot.get(input.expected).is_some_and(|project| {
        project
            .checkouts()
            .any(|checkout| checkout.path() == input.checkout)
    });
    if !exists {
        authorize(&caller)?;
        recheck_installation(service, &input.installation, original)?;
        return Ok(false);
    }
    // The snapshot is selection data, never a lock or a resource retained across join.
    drop(snapshot);
    supervisors
        .stop_checkout(input.expected, input.checkout)
        .await
        .map_err(Failure::Stop)?;
    original.snapshot().check().map_err(Failure::Request)?;
    authorize(&caller)?;
    recheck_installation(service, &input.installation, original)?;
    let mut writer = store
        .writer(input.installation, original.snapshot())
        .map_err(Failure::Inventory)?;
    // Core's atomic publication determines changed/unchanged; no fresh post-commit fence.
    writer
        .remove_checkout(input.expected, input.checkout)
        .map_err(Failure::Inventory)
}
