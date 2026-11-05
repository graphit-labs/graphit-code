//! Authenticated local Source inspection using existing Auth, index and FILE owners.
//! The caller composes issuer and installation from the same DaemonAuth and
//! supplies the already resolved local project scope/version.

use crate::{
    indexed_source::IndexedSourceError,
    source_index_locator::{SourceIndexError, SourceIndexPresence, open_local_source_index},
    source_inspection::{
        IndexedSource, PhysicalSource, SourceInspection, SourceInspectionError, inspect_source,
    },
};
use graphit_auth::{
    AuthError, AuthenticatedCaller, Permission, RoleIssuer, ScopeInput, VerifiedRoleGrant,
};
use graphit_core::{
    artifact_store::{
        CapturedSourceReader, RetainedBytesLimits, RetainedFileSet, SnapshotPolicy,
        SourceReadLimits,
    },
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    text_slice::Request,
};
use graphit_fs::NodeKind;
use graphit_hub::{native::RoleCredentialReference, policy::RequestContext};
use std::{error::Error, fmt, io, path::Path};

#[derive(Debug)]
pub enum SourceHandlerError {
    Authentication(AuthError),
    MissingProject,
    InvalidPath,
    Operation(io::Error),
    Index(SourceIndexError),
    Indexed(IndexedSourceError),
    Inspection(SourceInspectionError),
}
impl fmt::Display for SourceHandlerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authentication(error) => write!(f, "Source authentication: {error}"),
            Self::MissingProject => f.write_str("Source requires a resolved local project"),
            Self::InvalidPath => f.write_str("invalid Source file path"),
            Self::Operation(error) => write!(f, "Source operation: {error}"),
            Self::Index(error) => write!(f, "Source index: {error}"),
            Self::Indexed(error) => write!(f, "Source indexed text: {error}"),
            Self::Inspection(error) => write!(f, "Source inspection: {error}"),
        }
    }
}
impl Error for SourceHandlerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Authentication(error) => Some(error),
            Self::Operation(error) => Some(error),
            Self::Index(error) => Some(error),
            Self::Indexed(error) => Some(error),
            Self::Inspection(error) => Some(error),
            Self::MissingProject | Self::InvalidPath => None,
        }
    }
}
impl From<io::Error> for SourceHandlerError {
    fn from(error: io::Error) -> Self {
        Self::Operation(error)
    }
}

enum Authority<'a> {
    Caller(Box<AuthenticatedCaller<'a>>),
    Role(Box<VerifiedRoleGrant<'a>>),
}
impl Authority<'_> {
    fn authorize(&self) -> Result<(), SourceHandlerError> {
        let permission = Permission::Read(Module::Ast);
        match self {
            Self::Caller(caller) => caller.authorize(permission),
            Self::Role(grant) => grant.authorize(permission),
        }
        .map(|_| ())
        .map_err(SourceHandlerError::Authentication)
    }
}
pub struct CapturedSourceInput<'a, 'r> {
    pub scope: ScopeInput<'a>,
    pub requested_relative: &'r Path,
    pub selection: &'r Request,
    pub file_policy: &'r SnapshotPolicy,
    pub read_limits: SourceReadLimits,
    pub path_limits: RetainedBytesLimits,
    pub index_limits: NativeDocumentFileLimits,
    pub original: &'r std::sync::Arc<graphit_core::file_io::SnapshotContext>,
    pub entity: Option<&'r crate::project_entity_source::ProjectEntitySource>,
}

async fn read_authorized(
    authority: Authority<'_>,
    input: CapturedSourceInput<'_, '_>,
) -> Result<SourceInspection, SourceHandlerError> {
    authority.authorize()?;
    let original = input.original;
    let result = inspect_captured_source(input).await;
    original.check()?;
    authority.authorize()?;
    original.check()?;
    result
}

/// Inspect Source in actual caller-captured scopes. No selector/capture grants
/// authorization; the consumer retains its real admission and checks delivery.
pub async fn inspect_captured_source(
    input: CapturedSourceInput<'_, '_>,
) -> Result<SourceInspection, SourceHandlerError> {
    let original = input.original;
    original.check()?;
    let project = input
        .scope
        .project
        .ok_or(SourceHandlerError::MissingProject)?;
    if input.requested_relative.as_os_str().is_empty() {
        return Err(SourceHandlerError::InvalidPath);
    }
    let _validated = RetainedFileSet::new(
        std::iter::once(input.requested_relative.to_path_buf()),
        input.path_limits,
    )?;
    let key = input
        .requested_relative
        .to_str()
        .ok_or(SourceHandlerError::InvalidPath)?;
    original.check()?;
    let index = match open_local_source_index(
        input.scope.installation,
        project,
        original,
        input.index_limits,
    )
    .await
    .map_err(SourceHandlerError::Index)?
    {
        SourceIndexPresence::Absent => None,
        SourceIndexPresence::Available(index) => Some(index),
    };
    let indexed = match &index {
        Some(index) => Some(
            index
                .read(key, original)
                .await
                .map_err(SourceHandlerError::Index)?,
        ),
        None => None,
    };
    if input.entity.is_some()
        && indexed.as_ref().is_none_or(|source| {
            source.missing_reason() == Some(crate::indexed_source::MissingSourceReason::FileAbsent)
        })
    {
        return Err(SourceHandlerError::Inspection(
            SourceInspectionError::Association,
        ));
    }
    let source = match &indexed {
        Some(indexed) => indexed
            .source(original)
            .map_err(SourceHandlerError::Indexed)?,
        None => IndexedSource::Missing,
    };
    let physical = if matches!(source, IndexedSource::Missing) {
        original.check()?;
        Some(CapturedSourceReader::capture_project(
            project.scope.live,
            input.requested_relative,
            NodeKind::File,
            input.file_policy,
            input.read_limits,
            original,
        )?)
    } else {
        None
    };
    let physical_source = physical.as_ref().map(|reader| PhysicalSource {
        reader,
        relative: Path::new(""),
    });
    let result = if let Some(entity) = input.entity {
        crate::source_inspection::inspect_project_entity_source(
            source,
            physical_source,
            input.selection,
            entity,
            original,
        )
    } else {
        inspect_source(source, physical_source, input.selection, original)
    };
    if let Some(index) = &index {
        index
            .revalidate(original)
            .map_err(SourceHandlerError::Index)?;
    }
    if let Some(reader) = &physical {
        reader.revalidate_content()?;
        reader.check_request(original)?;
    }
    original.check()?;
    // All observed owners remain alive until the delivery checks above complete.
    drop((indexed, index, physical));
    result.map_err(SourceHandlerError::Inspection)
}

/// Runtime authentication before acquisition and again before delivery.
#[expect(
    clippy::too_many_arguments,
    reason = "Explicit composition of distinct owning APIs and operator policies"
)]
pub async fn inspect_source_runtime<'a>(
    issuer: &'a RoleIssuer,
    raw: &str,
    scope: ScopeInput<'a>,
    requested_relative: &Path,
    selection: &Request,
    file_policy: &SnapshotPolicy,
    read_limits: SourceReadLimits,
    path_limits: RetainedBytesLimits,
    index_limits: NativeDocumentFileLimits,
    request: &'a RequestContext<'a>,
) -> Result<SourceInspection, SourceHandlerError> {
    let caller = issuer
        .authenticate_runtime(raw, request)
        .map_err(SourceHandlerError::Authentication)?;
    read_authorized(
        Authority::Caller(Box::new(caller)),
        CapturedSourceInput {
            scope,
            requested_relative,
            selection,
            file_policy,
            read_limits,
            path_limits,
            index_limits,
            original: request.snapshot(),
            entity: None,
        },
    )
    .await
}

/// Local authentication before acquisition and again before delivery.
#[expect(
    clippy::too_many_arguments,
    reason = "Explicit composition of distinct owning APIs and operator policies"
)]
pub async fn inspect_source_local<'a>(
    issuer: &'a RoleIssuer,
    raw: &str,
    scope: ScopeInput<'a>,
    requested_relative: &Path,
    selection: &Request,
    file_policy: &SnapshotPolicy,
    read_limits: SourceReadLimits,
    path_limits: RetainedBytesLimits,
    index_limits: NativeDocumentFileLimits,
    request: &'a RequestContext<'a>,
) -> Result<SourceInspection, SourceHandlerError> {
    let caller = issuer
        .authenticate_local(raw, request)
        .map_err(SourceHandlerError::Authentication)?;
    read_authorized(
        Authority::Caller(Box::new(caller)),
        CapturedSourceInput {
            scope,
            requested_relative,
            selection,
            file_policy,
            read_limits,
            path_limits,
            index_limits,
            original: request.snapshot(),
            entity: None,
        },
    )
    .await
}

/// Verify the actual role credential and captured scope before acquisition.
#[expect(
    clippy::too_many_arguments,
    reason = "Explicit composition of role proof, distinct owning APIs and operator policies"
)]
pub async fn inspect_source_role<'a>(
    issuer: &'a RoleIssuer,
    raw: &str,
    reference: &RoleCredentialReference,
    scope: ScopeInput<'a>,
    requested_relative: &Path,
    selection: &Request,
    file_policy: &SnapshotPolicy,
    read_limits: SourceReadLimits,
    path_limits: RetainedBytesLimits,
    index_limits: NativeDocumentFileLimits,
    request: &'a RequestContext<'a>,
) -> Result<SourceInspection, SourceHandlerError> {
    let grant = issuer
        .verify(raw, reference, scope, request)
        .map_err(SourceHandlerError::Authentication)?;
    read_authorized(
        Authority::Role(Box::new(grant)),
        CapturedSourceInput {
            scope,
            requested_relative,
            selection,
            file_policy,
            read_limits,
            path_limits,
            index_limits,
            original: request.snapshot(),
            entity: None,
        },
    )
    .await
}
