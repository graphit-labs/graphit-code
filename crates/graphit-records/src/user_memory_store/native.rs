use super::*;
use crate::memory_store::MemoryStoreError;
use graphit_auth::{Obligation, UserMemoryWriteOperation, UserReadAdmission, UserWriteAdmission};
use graphit_core::{
    artifact_store::{PublicationScope, StoreOwner},
    file_io::SnapshotContext,
    file_lock::FileLock,
};
use graphit_data::{
    lance_branch_store::{BranchSnapshot, LanceBranchStore},
    lance_module_store::LanceModuleStore,
};
use std::sync::Arc;

pub(super) fn owner(scope: &PublicationScope) -> Result<UserMemoryOwner, MemoryStoreError> {
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
    if owner(scope)?.identity != *admission.identity()? {
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
            || owner(admission.user()?)?.identity != *admission.identity()?
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
pub(super) fn uri(scope: &PublicationScope) -> Result<String, MemoryStoreError> {
    scope
        .runtime_root()
        .join("memory")
        .join("index.lance")
        .into_os_string()
        .into_string()
        .map_err(|_| MemoryStoreError::Invalid("non-Unicode USER corpus path"))
}
pub(super) struct ReadPin {
    pub pin: BranchSnapshot,
    pub storage: crate::storage::StorageBinding,
    pub owner: UserMemoryOwner,
}
pub(super) async fn read_pin(
    original: &Arc<SnapshotContext>,
    admission: &UserReadAdmission<'_>,
) -> Result<Option<ReadPin>, MemoryStoreError> {
    read_pin_with_check(original, admission, || Ok(())).await
}
pub(super) async fn read_pin_with_check(
    original: &Arc<SnapshotContext>,
    admission: &UserReadAdmission<'_>,
    extra: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<ReadPin>, MemoryStoreError> {
    let check = || {
        check_read(original, admission)?;
        extra()
    };
    check()?;
    open_read_pin(
        original,
        admission.user()?,
        check,
        admission.record_storage().await?,
    )
    .await
}
pub(super) async fn read_write_pin(
    original: &Arc<SnapshotContext>,
    admission: &UserWriteAdmission<'_>,
    policy: UserWritePolicy,
) -> Result<Option<ReadPin>, MemoryStoreError> {
    policy.check(original, admission)?;
    open_read_pin(
        original,
        admission.user()?,
        || policy.check(original, admission),
        admission.record_storage().await?,
    )
    .await
}
async fn open_read_pin(
    original: &Arc<SnapshotContext>,
    scope: &PublicationScope,
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
            storage: crate::storage::StorageBinding::Remote(storage),
            owner: owner(scope)?,
        }));
    }

    let path = scope.runtime_root().join("memory").join("index.lance");
    if !graphit_core::local_files::directory_exists(&path)? {
        check()?;
        return Ok(None);
    }
    let before = || {
        check()?;
        Ok::<_, MemoryStoreError>(())
    };
    before()?;
    let store = LanceBranchStore::open(&uri(scope)?, rows::user_memory_schema(), None).await?;
    before()?;
    let pin = crate::storage::read(original, store.capture_target(None)).await??;
    before()?;
    Ok(Some(ReadPin {
        pin,
        storage: crate::storage::StorageBinding::Local,
        owner: owner(scope)?,
    }))
}
pub(super) struct WritePin {
    pub pin: BranchSnapshot,
    pub storage: crate::storage::StorageBinding,
    pub owner: UserMemoryOwner,
    _lock: Option<FileLock>,
    policy: UserWritePolicy,
}
impl WritePin {
    pub fn check(
        &self,
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
    ) -> Result<(), MemoryStoreError> {
        self.policy.check(original, admission)?;
        if self.owner != owner(admission.user()?)? {
            return Err(MemoryStoreError::WrongAdmission);
        }
        Ok(())
    }
}
pub(super) async fn acquire(
    original: &Arc<SnapshotContext>,
    admission: &UserWriteAdmission<'_>,
    create: bool,
) -> Result<Option<WritePin>, MemoryStoreError> {
    acquire_with_check(
        original,
        admission,
        create,
        UserWritePolicy::Ordinary,
        || Ok(()),
    )
    .await
}
pub(super) async fn acquire_with_check(
    original: &Arc<SnapshotContext>,
    admission: &UserWriteAdmission<'_>,
    create: bool,
    policy: UserWritePolicy,
    extra: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<WritePin>, MemoryStoreError> {
    let check = || {
        policy.check(original, admission)?;
        extra()
    };
    check()?;
    let scope = admission.user()?;
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
            storage: crate::storage::StorageBinding::Remote(storage),
            owner: owner(scope)?,
            _lock: None,
            policy,
        }));
    }
    let path = scope.runtime_root().join("memory").join("index.lance");
    if !create && !graphit_core::local_files::directory_exists(&path)? {
        check()?;
        return Ok(None);
    }
    let lock_path = path.clone();
    let request = Arc::clone(original);
    let lock = tokio::task::spawn_blocking(move || {
        crate::storage::local_writer_lock(&request, &lock_path)
    })
    .await
    .map_err(MemoryStoreError::WriterAcquisition)??;
    check()?;
    let address = uri(scope)?;
    if !graphit_core::local_files::directory_exists(&path)? {
        if !create {
            return Ok(None);
        }
        check()?;
        std::fs::create_dir_all(
            path.parent()
                .ok_or(MemoryStoreError::Invalid("corpus parent"))?,
        )?;
        check()?;
        LanceModuleStore::create(
            &address,
            rows::user_memory_schema(),
            vec![arrow_array::RecordBatch::new_empty(
                rows::user_memory_schema(),
            )],
        )
        .await?;
        check()?;
    }
    let before = || {
        check()?;
        original.check()?;
        Ok::<_, MemoryStoreError>(())
    };
    before()?;
    let store = LanceBranchStore::open(&address, rows::user_memory_schema(), None).await?;
    before()?;
    let pin = crate::storage::read(original, store.capture_target(None)).await??;
    before()?;
    Ok(Some(WritePin {
        pin,
        storage: crate::storage::StorageBinding::Local,
        owner: owner(scope)?,
        _lock: Some(lock),
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
    Ok(admission.actor()?.identifier().to_string())
}
