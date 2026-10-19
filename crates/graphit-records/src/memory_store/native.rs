use super::{MemoryGeneration, MemoryStoreError, rows};
use graphit_auth::{
    ProjectReadAdmission, ProjectReadModule, ProjectWriteAdmission, ProjectWriteOperation,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, StoreOwner},
    file_io::SnapshotContext,
    native_document_file::{
        CapturedProjectDocumentDirectory, DirectoryPresence, NativeDocumentFileLimits,
    },
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
pub(super) fn project_id(scope: &CapturedPublicationScope) -> Result<ProjectId, MemoryStoreError> {
    match scope.owner() {
        StoreOwner::Project { project_id } => Ok(*project_id),
        _ => Err(MemoryStoreError::WrongAdmission),
    }
}
pub(super) fn capture(
    scope: &CapturedPublicationScope,
    selected: &Path,
    original: &Arc<SnapshotContext>,
    limits: NativeDocumentFileLimits,
    index: bool,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<CapturedProjectDocumentDirectory>, MemoryStoreError> {
    if selected != Path::new("memory/index.lance") {
        return Err(MemoryStoreError::Invalid("corpus path"));
    }
    before()?;
    if CapturedProjectDocumentDirectory::probe_runtime(scope, Path::new(""), limits, original)?
        == DirectoryPresence::Absent
    {
        before()?;
        return Ok(None);
    }
    before()?;
    let mut relative = PathBuf::new();
    let mut guard =
        CapturedProjectDocumentDirectory::capture_runtime(scope, Path::new(""), limits, original)?;
    let selected = if index {
        selected
    } else {
        selected
            .parent()
            .ok_or(MemoryStoreError::Invalid("corpus parent"))?
    };
    for child in selected.components() {
        before()?;
        let presence = guard.probe_child_directory(child.as_os_str(), original)?;
        before()?;
        if presence == DirectoryPresence::Absent {
            return Ok(None);
        }
        relative.push(child.as_os_str());
        guard =
            CapturedProjectDocumentDirectory::capture_runtime(scope, &relative, limits, original)?;
    }
    guard.revalidate(original)?;
    before()?;
    Ok(Some(guard))
}
pub(super) fn uri(
    scope: &CapturedPublicationScope,
    relative: &Path,
) -> Result<String, MemoryStoreError> {
    scope
        .runtime_root()
        .join(relative)
        .into_os_string()
        .into_string()
        .map_err(|_| MemoryStoreError::Invalid("non-Unicode corpus path"))
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
    ) -> Result<Self, MemoryStoreError> {
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
) -> Result<PathBuf, MemoryStoreError> {
    Ok(graphit_core::local_corpus::resolve(
        scope,
        version,
        "memory",
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
    limits: NativeDocumentFileLimits,
) -> Result<Option<ReadPin>, MemoryStoreError> {
    read_pin_with_check(original, admission, limits, || Ok(())).await
}
pub(super) async fn read_pin_with_check(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    limits: NativeDocumentFileLimits,
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
            store.capture_target((!branch.is_empty()).then_some(branch.as_str())),
        )
        .await??;
        check()?;
        storage.check()?;
        return Ok(Some(ReadPin {
            pin,
            guard: crate::storage::Guard::Remote(storage),
        }));
    }
    let selected = local_relative(
        project.scope.live,
        project.version.version(),
        limits,
        original,
    )?;
    let Some(guard) = capture(project.scope.live, &selected, original, limits, true, check)? else {
        if let Some(parent) = capture(
            project.scope.live,
            &selected,
            original,
            limits,
            false,
            check,
        )? {
            check()?;
            parent.revalidate(original)?;
            return Err(MemoryStoreError::IncompleteCorpus);
        }
        return Ok(None);
    };
    let before = || {
        check()?;
        guard.revalidate(original)?;
        Ok::<_, MemoryStoreError>(())
    };
    before()?;
    let store =
        LanceBranchStore::open(&uri(project.scope.live, &selected)?, rows::schema(), None).await?;
    before()?;
    let branch = project.version.lance_branch();
    let pin = crate::storage::read(
        original,
        store.capture_target((!branch.is_empty()).then_some(branch.as_str())),
    )
    .await??;
    before()?;
    Ok(Some(ReadPin {
        pin,
        guard: crate::storage::Guard::Project(guard),
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
