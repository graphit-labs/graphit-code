use super::{TaskStoreError, rows};
use graphit_auth::{
    ProjectReadAdmission, ProjectReadModule, ProjectWriteAdmission, ProjectWriteOperation,
};
use graphit_core::{artifact_store::PublicationScope, file_io::SnapshotContext};
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
    scope: &PublicationScope,
) -> Result<graphit_core::project_identity::ProjectId, TaskStoreError> {
    match scope.owner() {
        graphit_core::artifact_store::StoreOwner::Project { project_id } => Ok(*project_id),
        _ => Err(TaskStoreError::WrongAdmission),
    }
}
pub(super) fn uri(scope: &PublicationScope, relative: &Path) -> Result<String, TaskStoreError> {
    scope
        .runtime_root()
        .join(relative)
        .into_os_string()
        .into_string()
        .map_err(|_| TaskStoreError::Invalid("non-Unicode corpus path"))
}
/// Select the admitted authoritative URI without filesystem capture.
pub(super) enum Selection {
    Local { path: PathBuf, uri: String },
    Remote(graphit_auth::RemoteRecordStorage),
}
impl Selection {
    pub fn resolve(
        remote: Option<graphit_auth::RemoteRecordStorage>,
        scope: &PublicationScope,
    ) -> Result<Self, TaskStoreError> {
        if let Some(remote) = remote {
            return Ok(Self::Remote(remote));
        }
        let relative = Path::new("task").join("index.lance");
        let path = scope.runtime_root().join(&relative);
        let uri = uri(scope, &relative)?;
        Ok(Self::Local { path, uri })
    }
    pub fn uri(&self) -> &str {
        match self {
            Self::Local { uri, .. } => uri,
            Self::Remote(storage) => &storage.uri,
        }
    }
}
pub(super) struct ReadPin {
    pub pin: BranchSnapshot,
    pub storage: crate::storage::StorageBinding,
}
pub(super) async fn read_pin(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
) -> Result<Option<ReadPin>, TaskStoreError> {
    check_read(original, admission)?;
    let project = admission.project()?;
    let scope = project.scope;
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
            storage: crate::storage::StorageBinding::Remote(storage),
        }));
    }
    let selected = Path::new("task").join("index.lance");
    if !graphit_core::local_files::directory_exists(&scope.runtime_root().join(&selected))? {
        check_read(original, admission)?;
        return Ok(None);
    }
    let before = || {
        check_read(original, admission)?;
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
        storage: crate::storage::StorageBinding::Local,
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
