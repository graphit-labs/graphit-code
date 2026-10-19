use super::{TaskStoreError, TaskStoreReadLimits, rows};
use graphit_auth::{
    ProjectReadAdmission, ProjectReadModule, ProjectWriteAdmission, ProjectWriteOperation,
};
use graphit_core::{
    artifact_store::CapturedPublicationScope,
    file_io::SnapshotContext,
    native_document_file::{CapturedProjectDocumentDirectory, DirectoryPresence},
};
use graphit_data::lance_branch_store::BranchSnapshot;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub(super) fn check_read(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
) -> Result<(), TaskStoreError> {
    original.check()?;
    if admission.module() != ProjectReadModule::Task {
        return Err(TaskStoreError::WrongAdmission);
    }
    if !Arc::ptr_eq(original, admission.request()?.snapshot()) {
        return Err(TaskStoreError::DifferentRequest);
    }
    Ok(())
}
pub(super) fn check_write(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
) -> Result<(), TaskStoreError> {
    original.check()?;
    if !matches!(
        admission.operation(),
        ProjectWriteOperation::Task
            | ProjectWriteOperation::OwnTask
            | ProjectWriteOperation::CoordinateTask
    ) {
        return Err(TaskStoreError::WrongAdmission);
    }
    if !Arc::ptr_eq(original, admission.request()?.snapshot()) {
        return Err(TaskStoreError::DifferentRequest);
    }
    Ok(())
}
pub(super) fn project_id(
    scope: &CapturedPublicationScope,
) -> Result<graphit_core::project_identity::ProjectId, TaskStoreError> {
    match scope.owner() {
        graphit_core::artifact_store::StoreOwner::Project { project_id } => Ok(*project_id),
        _ => Err(TaskStoreError::WrongAdmission),
    }
}
pub(super) fn capture(
    scope: &CapturedPublicationScope,
    selected: &Path,
    original: &Arc<SnapshotContext>,
    limits: TaskStoreReadLimits,
    index: bool,
    before: impl Fn() -> Result<(), TaskStoreError>,
) -> Result<Option<CapturedProjectDocumentDirectory>, TaskStoreError> {
    if selected != Path::new("task/index.lance") {
        return Err(TaskStoreError::Invalid("corpus path"));
    }
    before()?;
    if CapturedProjectDocumentDirectory::probe_runtime(
        scope,
        Path::new(""),
        limits.native,
        original,
    )? == DirectoryPresence::Absent
    {
        before()?;
        return Ok(None);
    }
    before()?;
    let mut relative = PathBuf::new();
    let mut guard = CapturedProjectDocumentDirectory::capture_runtime(
        scope,
        Path::new(""),
        limits.native,
        original,
    )?;
    let selected = if index {
        selected
    } else {
        selected
            .parent()
            .ok_or(TaskStoreError::Invalid("corpus parent"))?
    };
    for child in selected.components() {
        before()?;
        let presence = guard.probe_child_directory(child.as_os_str(), original)?;
        before()?;
        if presence == DirectoryPresence::Absent {
            return Ok(None);
        }
        relative.push(child.as_os_str());
        guard = CapturedProjectDocumentDirectory::capture_runtime(
            scope,
            &relative,
            limits.native,
            original,
        )?;
    }
    guard.revalidate(original)?;
    before()?;
    Ok(Some(guard))
}
pub(super) fn uri(
    scope: &CapturedPublicationScope,
    relative: &Path,
) -> Result<String, TaskStoreError> {
    scope
        .runtime_root()
        .join(relative)
        .into_os_string()
        .into_string()
        .map_err(|_| TaskStoreError::Invalid("non-Unicode corpus path"))
}
/// One admitted selection, reused by URI binding, capture and publication.
pub(super) enum Selection {
    Local { relative: PathBuf, uri: String },
    Remote(graphit_auth::RemoteRecordStorage),
}
impl Selection {
    pub fn resolve(
        remote: Option<graphit_auth::RemoteRecordStorage>,
        scope: &CapturedPublicationScope,
        version: &str,
        limits: graphit_core::native_document_file::NativeDocumentFileLimits,
        original: &Arc<SnapshotContext>,
    ) -> Result<Self, TaskStoreError> {
        if let Some(remote) = remote {
            return Ok(Self::Remote(remote));
        }
        let relative = local_relative(scope, version, limits, original)?;
        let uri = uri(scope, &relative)?;
        Ok(Self::Local { relative, uri })
    }
    pub fn uri(&self) -> &str {
        match self {
            Self::Local { uri, .. } => uri,
            Self::Remote(storage) => &storage.uri,
        }
    }
}
fn local_relative(
    scope: &CapturedPublicationScope,
    version: &str,
    limits: graphit_core::native_document_file::NativeDocumentFileLimits,
    original: &Arc<SnapshotContext>,
) -> Result<PathBuf, TaskStoreError> {
    Ok(graphit_core::local_corpus::resolve(
        scope,
        version,
        "task",
        "index.lance",
        limits,
        original,
    )?)
}
pub(super) struct ReadPin {
    pub pin: BranchSnapshot,
    pub guard: crate::storage::Guard,
}
pub(super) async fn read_pin(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    limits: TaskStoreReadLimits,
) -> Result<Option<ReadPin>, TaskStoreError> {
    read_pin_inner(original, admission, limits, false).await
}
pub(super) async fn read_pin_with_parent_check(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    limits: TaskStoreReadLimits,
) -> Result<Option<ReadPin>, TaskStoreError> {
    read_pin_inner(original, admission, limits, true).await
}
async fn read_pin_inner(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    limits: TaskStoreReadLimits,
    reject_partial: bool,
) -> Result<Option<ReadPin>, TaskStoreError> {
    check_read(original, admission)?;
    let project = admission.project()?;
    let scope = project.scope.live;
    if let Some(storage) = admission.record_storage().await? {
        check_read(original, admission)?;
        storage.check()?;
        let branch = project.version.lance_branch();
        let Some(pin) = remote_pin(
            original,
            &storage,
            (!branch.is_empty()).then_some(branch.as_str()),
            false,
        )
        .await?
        else {
            return Ok(None);
        };
        check_read(original, admission)?;
        storage.check()?;
        return Ok(Some(ReadPin {
            pin,
            guard: crate::storage::Guard::Remote(storage),
        }));
    }
    let selected = local_relative(scope, project.version.version(), limits.native, original)?;
    let Some(guard) = capture(scope, &selected, original, limits, true, || {
        check_read(original, admission)
    })?
    else {
        if reject_partial
            && let Some(parent) = capture(scope, &selected, original, limits, false, || {
                check_read(original, admission)
            })?
        {
            check_read(original, admission)?;
            parent.revalidate(original)?;
            return Err(TaskStoreError::IncompleteCorpus);
        }
        return Ok(None);
    };
    let before = || {
        check_read(original, admission)?;
        guard.revalidate(original)?;
        Ok::<_, TaskStoreError>(())
    };
    before()?;
    let uri = uri(scope, &selected)?;
    let branch = project.version.lance_branch();
    let pin = crate::storage::read(
        original,
        BranchSnapshot::capture_observed(
            &uri,
            (!branch.is_empty()).then_some(branch.as_str()),
            None,
        ),
    )
    .await??;
    rows::validate_schema(pin.schema())?;
    before()?;
    Ok(Some(ReadPin {
        pin,
        guard: crate::storage::Guard::Project(guard),
    }))
}

pub(super) fn require_operation(
    admission: &ProjectWriteAdmission<'_>,
    allowed: &[ProjectWriteOperation],
) -> Result<(), TaskStoreError> {
    if allowed.contains(&admission.operation()) {
        Ok(())
    } else {
        Err(TaskStoreError::WrongAdmission)
    }
}

/// Capture the selected authoritative S3 branch with its actual optional vector
/// field. Native nullable-column evolution never replaces existing fragments.
pub(super) async fn remote_pin(
    original: &Arc<SnapshotContext>,
    storage: &graphit_auth::RemoteRecordStorage,
    branch: Option<&str>,
    create: bool,
) -> Result<Option<BranchSnapshot>, TaskStoreError> {
    storage.check()?;
    let params = crate::storage::params(storage)?;
    let capture = || BranchSnapshot::capture_observed(&storage.uri, branch, Some(params.clone()));
    let pin = match crate::storage::read(original, capture()).await? {
        Ok(pin) => pin,
        Err(graphit_data::lance_branch_store::BranchError::Sdk(error))
            if matches!(error.as_ref(), lance::Error::DatasetNotFound { .. }) =>
        {
            if !create {
                return Ok(None);
            }
            crate::storage::open_remote(original, storage, rows::schema(), true).await?;
            crate::storage::read(original, capture()).await??
        }
        Err(error) => return Err(error.into()),
    };
    rows::validate_schema(pin.schema())?;
    storage.check()?;
    Ok(Some(pin))
}
