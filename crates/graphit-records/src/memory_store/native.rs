use super::{MemoryGeneration, MemoryStoreError, rows};
use graphit_auth::{
    ProjectReadAdmission, ProjectReadModule, ProjectWriteAdmission, ProjectWriteOperation,
};
use graphit_core::{
    artifact_store::{PublicationScope, StoreOwner},
    file_io::SnapshotContext,
    project_identity::ProjectId,
};
use graphit_data::lance_branch_store::{BranchSnapshot, LanceBranchStore};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub(super) fn check_read(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
) -> Result<(), MemoryStoreError> {
    original.check()?;
    if admission.module() != ProjectReadModule::Memory {
        return Err(MemoryStoreError::WrongAdmission);
    }
    if !Arc::ptr_eq(original, admission.request()?.snapshot()) {
        return Err(MemoryStoreError::DifferentRequest);
    }
    Ok(())
}
/// Chooses the domain policy; every check still requires the original live admission.
#[derive(Clone, Copy)]
pub(super) enum WritePolicy {
    Ordinary,
    Fenced,
}
impl WritePolicy {
    pub fn check(
        self,
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
    ) -> Result<(), MemoryStoreError> {
        original.check()?;
        let operation = match self {
            Self::Ordinary => ProjectWriteOperation::Memory,
            Self::Fenced => ProjectWriteOperation::FencedMemory,
        };
        if admission.operation() != operation {
            return Err(MemoryStoreError::WrongAdmission);
        }
        if !Arc::ptr_eq(original, admission.request()?.snapshot()) {
            return Err(MemoryStoreError::DifferentRequest);
        }
        let obligation = admission.recheck()?;
        if matches!(self, Self::Fenced)
            && obligation != graphit_auth::Obligation::MemoryFenceRequired
        {
            return Err(MemoryStoreError::WrongAdmission);
        }
        Ok(())
    }
}
pub(super) fn check_write(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
) -> Result<(), MemoryStoreError> {
    WritePolicy::Ordinary.check(original, admission)
}
pub(super) fn project_id(scope: &PublicationScope) -> Result<ProjectId, MemoryStoreError> {
    match scope.owner() {
        StoreOwner::Project { project_id } => Ok(*project_id),
        _ => Err(MemoryStoreError::WrongAdmission),
    }
}
pub(super) fn uri(scope: &PublicationScope, relative: &Path) -> Result<String, MemoryStoreError> {
    scope
        .runtime_root()
        .join(relative)
        .into_os_string()
        .into_string()
        .map_err(|_| MemoryStoreError::Invalid("non-Unicode corpus path"))
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
    ) -> Result<Self, MemoryStoreError> {
        if let Some(remote) = remote {
            return Ok(Self::Remote(remote));
        }
        let relative = Path::new("memory").join("index.lance");
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
) -> Result<Option<ReadPin>, MemoryStoreError> {
    read_pin_with_check(original, admission, || Ok(())).await
}
pub(super) async fn read_pin_with_check(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    paired: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<ReadPin>, MemoryStoreError> {
    let check = || {
        check_read(original, admission)?;
        paired()
    };
    check()?;
    let project = admission.project()?;
    if let Some(storage) = admission.record_storage().await? {
        check()?;
        storage.check()?;
        let opened = crate::storage::open_remote(original, &storage, rows::schema(), false).await;
        check()?;
        let Some(store) = opened? else {
            check()?;
            return Ok(None);
        };
        let branch = project.version.lance_branch();
        let pin = crate::storage::read(
            original,
            store.into_target((!branch.is_empty()).then_some(branch.as_str())),
        )
        .await??;
        check()?;
        storage.check()?;
        return Ok(Some(ReadPin {
            pin,
            storage: crate::storage::StorageBinding::Remote(storage),
        }));
    }
    let selected = Path::new("memory").join("index.lance");
    let path = project.scope.runtime_root().join(&selected);
    if !graphit_core::local_files::directory_exists(&path)? {
        check()?;
        return Ok(None);
    }
    let before = || {
        check()?;
        Ok::<_, MemoryStoreError>(())
    };
    before()?;
    let store =
        LanceBranchStore::open(&uri(project.scope, &selected)?, rows::schema(), None).await?;
    before()?;
    let branch = project.version.lance_branch();
    let pin = crate::storage::read(
        original,
        store.into_target((!branch.is_empty()).then_some(branch.as_str())),
    )
    .await??;
    before()?;
    Ok(Some(ReadPin {
        pin,
        storage: crate::storage::StorageBinding::Local,
    }))
}
pub(super) fn generation(
    pin: &BranchSnapshot,
    project_id: ProjectId,
    project_version: &str,
) -> MemoryGeneration {
    MemoryGeneration {
        sdk_version: pin.version(),
        project_id,
        project_version: project_version.to_owned(),
        physical_branch: pin.physical_branch().map(str::to_owned),
        inherited_target: pin.inherited_target().map(str::to_owned),
    }
}
pub(super) fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
