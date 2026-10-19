//! Retain the logical versions confirmed by a first native Knowledge publication.
//! Inventory metadata is historical data, never authority or presence.
//!
//! ```
//! use graphit_daemon::inventory::KnowledgeInventoryError;
//! fn completed_retention(error: &KnowledgeInventoryError) -> usize {
//!     error.retained.len()
//! }
//! ```
#![cfg(target_os = "linux")]
use graphit_auth::{AuthError, ProjectWriteAdmission, ProjectWriteOperation};
use graphit_core::{
    auth_document::AuthDocumentError,
    file_io::SnapshotContext,
    git_checkout::GitCheckout,
    installation_bootstrap::PrivateInstallation,
    native_document_file::NativeDocumentFileLimits,
    project_inventory::{CapturedProjectInventory, GlobalProjectInventory},
    project_paths::ProjectPaths,
};
use graphit_data::{
    knowledge_document_projection::PreparedKnowledgeDocumentRows,
    knowledge_index::{
        KnowledgeFirstPublication, KnowledgeGeneration, KnowledgeIndexError,
        KnowledgeIndexWriteLimits,
    },
};
use std::{error::Error, fmt, io, sync::Arc};

pub struct KnowledgeInventoryInput<'a> {
    pub project: &'a ProjectPaths,
    pub checkout: &'a GitCheckout,
    pub installation: PrivateInstallation,
}

pub struct KnowledgeInventoryLimits {
    pub native: NativeDocumentFileLimits,
    pub inventory_bytes: usize,
    pub data: KnowledgeIndexWriteLimits,
}

/// The outcome of an actual Core mutation, not an authorization decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VersionRetention {
    Retained,
    Unchanged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedVersion {
    pub version: String,
    pub change: VersionRetention,
}

pub struct KnowledgeInventoryPublication {
    pub publication: KnowledgeFirstPublication,
    pub retained: Vec<RetainedVersion>,
}
impl fmt::Debug for KnowledgeInventoryPublication {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KnowledgeInventoryPublication")
            .field("default", &self.publication.default)
            .field("named", &self.publication.named)
            .field("retained", &self.retained)
            .finish()
    }
}

#[derive(Debug)]
pub enum KnowledgeInventoryCause {
    Request(io::Error),
    Installation(AuthDocumentError),
    Admission(AuthError),
    Capture(io::Error),
    Inventory(io::Error),
    ResultBinding(io::Error),
    Data(KnowledgeIndexError),
}
impl fmt::Display for KnowledgeInventoryCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Request(_) => "Knowledge inventory request failed",
            Self::Installation(_) => "Knowledge inventory installation failed",
            Self::Admission(_) => "Knowledge inventory admission failed",
            Self::Capture(_) => "Knowledge inventory capture failed",
            Self::Inventory(_) => "Knowledge inventory retention failed",
            Self::ResultBinding(_) => "Knowledge publication result binding failed",
            Self::Data(_) => "Knowledge publication failed",
        })
    }
}
impl Error for KnowledgeInventoryCause {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Request(e) | Self::Capture(e) | Self::Inventory(e) | Self::ResultBinding(e) => e,
            Self::Installation(e) => e,
            Self::Admission(e) => e,
            Self::Data(e) => e,
        })
    }
}

/// Preserve confirmed SDK effects and earlier inventory commits on failure.
/// For PartialDefault, the original Data cause contains the confirmed default;
/// `publication` stays absent because the named target was not confirmed.
pub struct KnowledgeInventoryError {
    pub cause: KnowledgeInventoryCause,
    pub publication: Option<KnowledgeFirstPublication>,
    pub retained: Vec<RetainedVersion>,
    pub retention_failure: Option<KnowledgeInventoryCause>,
}
impl fmt::Debug for KnowledgeInventoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KnowledgeInventoryError")
            .field("cause", &self.cause)
            .field("default", &self.publication.as_ref().map(|p| &p.default))
            .field(
                "named",
                &self.publication.as_ref().and_then(|p| p.named.as_ref()),
            )
            .field("retained", &self.retained)
            .field("retention_failure", &self.retention_failure)
            .finish()
    }
}
impl fmt::Display for KnowledgeInventoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.cause.fmt(f)
    }
}
impl Error for KnowledgeInventoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.cause)
    }
}
impl From<KnowledgeInventoryCause> for Box<KnowledgeInventoryError> {
    fn from(cause: KnowledgeInventoryCause) -> Self {
        Box::new(KnowledgeInventoryError {
            cause,
            publication: None,
            retained: Vec::new(),
            retention_failure: None,
        })
    }
}

fn check_admission(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
) -> Result<(), KnowledgeInventoryCause> {
    original.check().map_err(KnowledgeInventoryCause::Request)?;
    if admission.operation() != ProjectWriteOperation::Knowledge {
        return Err(KnowledgeInventoryCause::Data(
            KnowledgeIndexError::WrongAdmission,
        ));
    }
    let request = admission
        .request()
        .map_err(KnowledgeInventoryCause::Admission)?;
    if !Arc::ptr_eq(original, request.snapshot()) {
        return Err(KnowledgeInventoryCause::Data(
            KnowledgeIndexError::DifferentRequest,
        ));
    }
    admission
        .recheck()
        .map_err(KnowledgeInventoryCause::Admission)?;
    Ok(())
}

struct RetentionFailure {
    cause: KnowledgeInventoryCause,
    retained: Vec<RetainedVersion>,
}

fn check_default(generation: &KnowledgeGeneration) -> Result<(), KnowledgeInventoryCause> {
    if generation.version == 0
        || generation.corpus_uri.is_empty()
        || generation.physical_branch.is_some()
        || generation.inherited_target.is_some()
    {
        return Err(KnowledgeInventoryCause::ResultBinding(
            io::ErrorKind::InvalidData.into(),
        ));
    }
    Ok(())
}

fn check_publication(
    publication: &KnowledgeFirstPublication,
    selected_branch: &str,
) -> Result<(), KnowledgeInventoryCause> {
    check_default(&publication.default)?;
    match (&publication.named, selected_branch.is_empty()) {
        (None, true) => Ok(()),
        (Some(named), false)
            if named.physical_branch.as_deref() == Some(selected_branch)
                && named.inherited_target.is_none()
                && named.corpus_uri == publication.default.corpus_uri
                && named.version == publication.default.version =>
        {
            Ok(())
        }
        _ => Err(KnowledgeInventoryCause::ResultBinding(
            io::ErrorKind::InvalidData.into(),
        )),
    }
}

fn retain_confirmed(
    store: &GlobalProjectInventory,
    installation: PrivateInstallation,
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    capture: &CapturedProjectInventory,
    versions: &[&str],
) -> Result<Vec<RetainedVersion>, RetentionFailure> {
    let mut retained = Vec::with_capacity(versions.len());
    let result = (|| {
        check_admission(original, admission)?;
        let mut writer = store
            .writer(installation, original)
            .map_err(KnowledgeInventoryCause::Inventory)?;
        for &version in versions {
            check_admission(original, admission)?;
            let changed = writer
                .retain_version(capture, version)
                .map_err(KnowledgeInventoryCause::Inventory)?;
            retained.push(RetainedVersion {
                version: version.to_owned(),
                change: if changed {
                    VersionRetention::Retained
                } else {
                    VersionRetention::Unchanged
                },
            });
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(retained),
        Err(cause) => Err(RetentionFailure { cause, retained }),
    }
}

/// Publish through the real Data SDK and retain only its confirmed targets.
/// No inventory lock crosses the SDK await. Each retention is a separate Core
/// commit; an error preserves earlier effects without cross-store rollback.
#[cfg(target_os = "linux")]
pub async fn publish_knowledge_and_retain(
    input: KnowledgeInventoryInput<'_>,
    limits: KnowledgeInventoryLimits,
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    rows: &PreparedKnowledgeDocumentRows<'_, '_>,
) -> Result<KnowledgeInventoryPublication, Box<KnowledgeInventoryError>> {
    use KnowledgeInventoryCause as Cause;
    check_admission(original, admission)?;
    input
        .installation
        .revalidate(original)
        .map_err(Cause::Installation)?;
    if input.installation.paths().brand().key() != input.project.brand().key() {
        return Err(Cause::Capture(io::ErrorKind::InvalidInput.into()).into());
    }
    let project = admission.project().map_err(Cause::Admission)?;
    let selection = project.version;
    let capture = CapturedProjectInventory::capture_in_scope(
        input.project,
        input.checkout,
        selection,
        project.scope.live,
        limits.native,
        original,
    )
    .map_err(Cause::Capture)?;
    let store = GlobalProjectInventory::at(input.installation.paths(), limits.inventory_bytes)
        .map_err(Cause::Inventory)?;
    store.load(original).map_err(Cause::Inventory)?;
    check_admission(original, admission)?;
    match graphit_data::knowledge_index::publish_native_authored(
        original,
        admission,
        rows,
        limits.data,
    )
    .await
    {
        Ok(publication) => {
            if let Err(cause) = check_publication(&publication, &selection.lance_branch()) {
                return Err(Box::new(KnowledgeInventoryError {
                    cause,
                    publication: Some(publication),
                    retained: Vec::new(),
                    retention_failure: None,
                }));
            }
            let mut versions = vec![selection.default_version()];
            if publication.named.is_some() && selection.version() != selection.default_version() {
                versions.push(selection.version());
            }
            match retain_confirmed(
                &store,
                input.installation,
                original,
                admission,
                &capture,
                &versions,
            ) {
                Ok(retained) => Ok(KnowledgeInventoryPublication {
                    publication,
                    retained,
                }),
                Err(failure) => Err(Box::new(KnowledgeInventoryError {
                    cause: failure.cause,
                    publication: Some(publication),
                    retained: failure.retained,
                    retention_failure: None,
                })),
            }
        }
        Err(KnowledgeIndexError::PartialDefault { generation, cause }) => {
            let retention = check_default(&generation)
                .map_err(|cause| RetentionFailure {
                    cause,
                    retained: Vec::new(),
                })
                .and_then(|()| {
                    retain_confirmed(
                        &store,
                        input.installation,
                        original,
                        admission,
                        &capture,
                        &[selection.default_version()],
                    )
                });
            let (retained, retention_failure) = match retention {
                Ok(retained) => (retained, None),
                Err(failure) => (failure.retained, Some(failure.cause)),
            };
            Err(Box::new(KnowledgeInventoryError {
                cause: Cause::Data(KnowledgeIndexError::PartialDefault { generation, cause }),
                publication: None,
                retained,
                retention_failure,
            }))
        }
        Err(error) => Err(Cause::Data(error).into()),
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
