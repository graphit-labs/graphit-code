use super::*;
use crate::memory_store::{MemoryStoreError, MemoryStoreReadLimits, MemoryStoreWriteLimits};
use graphit_auth::{Obligation, UserMemoryWriteOperation, UserReadAdmission, UserWriteAdmission};
use graphit_core::{
    artifact_store::{ArtifactStore, ArtifactWriter, CapturedPublicationScope, StoreOwner},
    file_io::SnapshotContext,
    native_document_file::{
        CapturedUserDocumentDirectory, DirectoryPresence, NativeDocumentFileLimits,
    },
};
use graphit_data::{
    lance_branch_store::{BranchSnapshot, LanceBranchStore},
    lance_module_store::LanceModuleStore,
};
use std::{ffi::OsStr, path::Path, sync::Arc};

pub(super) fn owner(scope: &CapturedPublicationScope) -> Result<UserMemoryOwner, MemoryStoreError> {
    match scope.owner() {
        StoreOwner::User {
            installation_id,
            identity,
        } => Ok(UserMemoryOwner {
            installation_id: installation_id.clone(),
            identity: identity.clone(),
        }),
        _ => Err(MemoryStoreError::WrongAdmission),
    }
}
pub(super) fn check_read(
    original: &Arc<SnapshotContext>,
    admission: &UserReadAdmission<'_>,
) -> Result<(), MemoryStoreError> {
    original.check()?;
    if !Arc::ptr_eq(original, admission.request()?.snapshot()) {
        return Err(MemoryStoreError::DifferentRequest);
    }
    admission.recheck()?;
    let scope = admission.user()?;
    if owner(scope.live)?.identity != *admission.identity()? {
        return Err(MemoryStoreError::WrongAdmission);
    }
    Ok(())
}
#[derive(Clone, Copy)]
pub(super) enum UserWritePolicy {
    Ordinary,
    Fenced,
}
impl UserWritePolicy {
    pub fn check(
        self,
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
    ) -> Result<(), MemoryStoreError> {
        original.check()?;
        if !Arc::ptr_eq(original, admission.request()?.snapshot()) {
            return Err(MemoryStoreError::DifferentRequest);
        }
        let (operation, obligation) = match self {
            Self::Ordinary => (UserMemoryWriteOperation::Memory, Obligation::Unconditional),
            Self::Fenced => (
                UserMemoryWriteOperation::FencedMemory,
                Obligation::MemoryFenceRequired,
            ),
        };
        if admission.operation()? != operation
            || admission.recheck()? != obligation
            || owner(admission.user()?.live)?.identity != *admission.identity()?
        {
            return Err(MemoryStoreError::WrongAdmission);
        }
        Ok(())
    }
}
pub(super) fn check_write(
    original: &Arc<SnapshotContext>,
    admission: &UserWriteAdmission<'_>,
) -> Result<(), MemoryStoreError> {
    UserWritePolicy::Ordinary.check(original, admission)
}
pub(super) fn uri(scope: &CapturedPublicationScope) -> Result<String, MemoryStoreError> {
    scope
        .runtime_root()
        .join("memory/index.lance")
        .into_os_string()
        .into_string()
        .map_err(|_| MemoryStoreError::Invalid("non-Unicode USER corpus path"))
}
fn capture(
    scope: &CapturedPublicationScope,
    original: &Arc<SnapshotContext>,
    limits: NativeDocumentFileLimits,
    check: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<CapturedUserDocumentDirectory>, MemoryStoreError> {
    check()?;
    if CapturedUserDocumentDirectory::probe_memory(scope, limits, original)?
        == DirectoryPresence::Absent
    {
        check()?;
        return Ok(None);
    }
    check()?;
    let parent = CapturedUserDocumentDirectory::capture(
        scope,
        Path::new("runtime/memory"),
        limits,
        original,
    )?;
    parent.revalidate(original)?;
    let present = parent.probe_child_directory(OsStr::new("index.lance"), original)?;
    parent.revalidate(original)?;
    check()?;
    if present == DirectoryPresence::Absent {
        return Err(MemoryStoreError::IncompleteCorpus);
    }
    let guard = CapturedUserDocumentDirectory::capture(
        scope,
        Path::new("runtime/memory/index.lance"),
        limits,
        original,
    )?;
    guard.revalidate(original)?;
    check()?;
    Ok(Some(guard))
}
pub(super) struct ReadPin {
    pub pin: BranchSnapshot,
    pub guard: crate::storage::Guard,
    pub owner: UserMemoryOwner,
}
pub(super) async fn read_pin(
    original: &Arc<SnapshotContext>,
    admission: &UserReadAdmission<'_>,
    limits: NativeDocumentFileLimits,
) -> Result<Option<ReadPin>, MemoryStoreError> {
    read_pin_with_check(original, admission, limits, || Ok(())).await
}
pub(super) async fn read_pin_with_check(
    original: &Arc<SnapshotContext>,
    admission: &UserReadAdmission<'_>,
    limits: NativeDocumentFileLimits,
    extra: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<ReadPin>, MemoryStoreError> {
    let check = || {
        check_read(original, admission)?;
        extra()
    };
    check()?;
    open_read_pin(
        original,
        admission.user()?.live,
        limits,
        check,
        admission.record_storage().await?,
    )
    .await
}
pub(super) async fn read_write_pin(
    original: &Arc<SnapshotContext>,
    admission: &UserWriteAdmission<'_>,
    limits: MemoryStoreReadLimits,
    policy: UserWritePolicy,
) -> Result<Option<ReadPin>, MemoryStoreError> {
    policy.check(original, admission)?;
    open_read_pin(
        original,
        admission.user()?.live,
        limits.native,
        || policy.check(original, admission),
        admission.record_storage().await?,
    )
    .await
}
async fn open_read_pin(
    original: &Arc<SnapshotContext>,
    scope: &CapturedPublicationScope,
    limits: NativeDocumentFileLimits,
    check: impl Fn() -> Result<(), MemoryStoreError>,
    storage: Option<graphit_auth::RemoteRecordStorage>,
) -> Result<Option<ReadPin>, MemoryStoreError> {
    check()?;
    if let Some(storage) = storage {
        storage.check()?;
        let opened =
            crate::storage::open_remote(original, &storage, rows::user_memory_schema(), false)
                .await;
        check()?;
        let Some(store) = opened? else {
            check()?;
            return Ok(None);
        };
        let pin = crate::storage::read(original, store.capture_target(None)).await??;
        check()?;
        storage.check()?;
        return Ok(Some(ReadPin {
            pin,
            guard: crate::storage::Guard::Remote(storage),
            owner: owner(scope)?,
        }));
    }

    let Some(guard) = capture(scope, original, limits, &check)? else {
        return Ok(None);
    };
    let before = || {
        check()?;
        guard.revalidate(original)?;
        Ok::<_, MemoryStoreError>(())
    };
    before()?;
    let store = LanceBranchStore::open(&uri(scope)?, rows::user_memory_schema(), None).await?;
    before()?;
    let pin = crate::storage::read(original, store.capture_target(None)).await??;
    before()?;
    Ok(Some(ReadPin {
        pin,
        guard: crate::storage::Guard::User(guard),
        owner: owner(scope)?,
    }))
}
pub(super) struct WritePin {
    pub pin: BranchSnapshot,
    pub guard: crate::storage::Guard,
    pub owner: UserMemoryOwner,
    _writer: Option<ArtifactWriter>,
    policy: UserWritePolicy,
}
impl WritePin {
    pub fn check(
        &self,
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
    ) -> Result<(), MemoryStoreError> {
        self.policy.check(original, admission)?;
        self.guard.revalidate(original)?;
        if self.owner != owner(admission.user()?.live)? {
            return Err(MemoryStoreError::WrongAdmission);
        }
        Ok(())
    }
}
pub(super) async fn acquire(
    original: &Arc<SnapshotContext>,
    admission: &UserWriteAdmission<'_>,
    limits: MemoryStoreWriteLimits,
    create: bool,
) -> Result<Option<WritePin>, MemoryStoreError> {
    acquire_with_check(
        original,
        admission,
        limits,
        create,
        UserWritePolicy::Ordinary,
        || Ok(()),
    )
    .await
}
pub(super) async fn acquire_with_check(
    original: &Arc<SnapshotContext>,
    admission: &UserWriteAdmission<'_>,
    limits: MemoryStoreWriteLimits,
    create: bool,
    policy: UserWritePolicy,
    extra: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<WritePin>, MemoryStoreError> {
    let check = || {
        policy.check(original, admission)?;
        extra()
    };
    check()?;
    let scope = admission.user()?.live;
    if let Some(storage) = admission.record_storage().await? {
        check()?;
        storage.check()?;
        let opened =
            crate::storage::open_remote(original, &storage, rows::user_memory_schema(), create)
                .await;
        check()?;
        let Some(store) = opened? else {
            check()?;
            return Ok(None);
        };
        let pin = crate::storage::read(original, store.capture_target(None)).await??;
        check()?;
        storage.check()?;
        return Ok(Some(WritePin {
            pin,
            guard: crate::storage::Guard::Remote(storage),
            owner: owner(scope)?,
            _writer: None,
            policy,
        }));
    }
    // Missing non-create operations return before acquiring a writer or birthing USER directories.
    if !create && capture(scope, original, limits.read.native, check)?.is_none() {
        check()?;
        return Ok(None);
    }
    let artifacts = ArtifactStore::new(scope.clone(), limits.artifacts);
    let request = Arc::clone(original);
    let mut writer = tokio::task::spawn_blocking(move || artifacts.writer_with_context(&request))
        .await
        .map_err(MemoryStoreError::WriterAcquisition)??;
    check()?;
    let address = uri(scope)?;
    let guard = match capture(scope, original, limits.read.native, check)? {
        Some(guard) => guard,
        None => {
            if !create {
                return Ok(None);
            }
            check()?;
            let parent = writer.create_user_memory_directory(limits.read.native)?;
            parent.revalidate(original)?;
            check()?;
            LanceModuleStore::create(
                &address,
                rows::user_memory_schema(),
                vec![arrow_array::RecordBatch::new_empty(
                    rows::user_memory_schema(),
                )],
            )
            .await?;
            parent.revalidate(original)?;
            check()?;
            capture(scope, original, limits.read.native, check)?
                .ok_or(MemoryStoreError::IncompleteCorpus)?
        }
    };
    let before = || {
        check()?;
        guard.revalidate(original)?;
        Ok::<_, MemoryStoreError>(())
    };
    before()?;
    let store = LanceBranchStore::open(&address, rows::user_memory_schema(), None).await?;
    before()?;
    let pin = crate::storage::read(original, store.capture_target(None)).await??;
    before()?;
    Ok(Some(WritePin {
        pin,
        guard: crate::storage::Guard::User(guard),
        owner: owner(scope)?,
        _writer: Some(writer),
        policy,
    }))
}
pub(super) fn generation(pin: &BranchSnapshot, owner: &UserMemoryOwner) -> UserMemoryGeneration {
    UserMemoryGeneration {
        owner: owner.clone(),
        sdk_version: pin.version(),
        physical_branch: pin.physical_branch().map(str::to_owned),
        inherited_target: pin.inherited_target().map(str::to_owned),
    }
}
pub(super) fn actor(admission: &UserWriteAdmission<'_>) -> Result<String, MemoryStoreError> {
    let mut bytes = [0; 64];
    admission.actor()?.write_identifier(&mut bytes);
    String::from_utf8(bytes.to_vec()).map_err(|_| MemoryStoreError::Invalid("USER actor encoding"))
}
