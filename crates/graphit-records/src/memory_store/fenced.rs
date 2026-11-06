//! Direct Memory tools under a real borrowed FencedMemory admission.
use super::{native::WritePolicy, revision, *};
use graphit_auth::ProjectWriteAdmission;
use graphit_core::file_io::SnapshotContext;
use std::sync::Arc;

/// Direct Dream updates expose content and authored references, never metadata flags.
/// Host parsing and rejection of unknown JSON fields belong to the host consumer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FencedUpdateMemory {
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub reason: String,
    pub title: Option<String>,
    pub body: Option<String>,
    pub references: Option<Vec<MemoryReference>>,
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::{
        memory_attributes::{MemoryAttributes, MemoryClass},
        memory_store as memory_api,
    };
    use graphit_auth::{
        Permission, ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation,
    };
    use graphit_core::{file_lock::Cancellation, module_settings::Module};
    use graphit_hub::native::Role;
    use std::cell::Cell;
    mod fixture {
        include!("../../tests/memory_store/fixture.rs");
    }
    use fixture::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fenced_confirmed_sdk_delta_preserves_event_and_fresh_reobservation() {
        // This exercises the production publish composition, not SDK interception.
        for revoked in [false, true] {
            let cancel = Cancellation::default();
            let ctx = request(&cancel);
            let f = Fixture::new(&ctx, "main");
            let issuer = f.issuer(&ctx);
            let permissions = [
                Permission::Read(Module::Memory),
                Permission::WriteFencedMemory,
            ];
            let (bearer, credential, run, admin) =
                f.issue_role_for(&issuer, &ctx, f.scope(), Role::Dream, &permissions);
            let grant = issuer
                .verify(bearer.expose_for_transport(), &credential, f.scope(), &ctx)
                .unwrap();
            let authority = ProjectRecordAuthority::new(&issuer);
            let write = authority
                .admit_write(&grant, &ctx, ProjectWriteOperation::FencedMemory)
                .unwrap();
            let authored = CreateMemory {
                idempotency_key: "confirmed-seed".into(),
                title: "Seed".into(),
                body: "Durable body".into(),
                attributes: MemoryAttributes {
                    classification: MemoryClass::Decision,
                    mandatory: true,
                    important: true,
                },
                tags: vec![],
                references: None,
            };
            let initial =
                NativeMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
                    .await
                    .unwrap();
            let capture = write::acquire_with_policy(
                ctx.snapshot(),
                &write,
                write_limits(),
                false,
                WritePolicy::Fenced,
            )
            .await
            .unwrap()
            .unwrap();
            let before = || capture.check(ctx.snapshot(), &write);
            let project_id = native::project_id(write.project().unwrap().scope.live).unwrap();
            let chain = history::chain(
                &capture.pin,
                ctx.snapshot(),
                initial.id,
                project_id,
                read_limits(),
                &before,
            )
            .await
            .unwrap()
            .unwrap();
            let attempt = FencedUpdateMemory {
                expected_revision: 1,
                idempotency_key: "confirmed-update".into(),
                reason: "Actual publication".into(),
                title: Some("Confirmed title".into()),
                body: None,
                references: None,
            };
            let operation = FencedMemoryMutation::Update(attempt.clone()).into_mutation();
            let actor = write::actor(&write).unwrap();
            let after = revision::derive(
                ctx.snapshot(),
                project_id,
                &initial,
                &operation,
                &actor,
                initial.updated_at + 1,
            )
            .unwrap();
            let event = MemoryRevision {
                project_id,
                operation_version: "main".into(),
                actor,
                operation,
                before: initial.clone(),
                after: after.clone(),
            };
            let pending = capture
                .pin
                .prepare_merge(
                    vec![rows::mutation_batch(&chain.creation, &event, write_limits()).unwrap()],
                    vec!["row_key".into()],
                )
                .await
                .unwrap();
            let expected_version = capture.pin.version() + 1;
            let secret =
                std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key"))
                    .unwrap();
            let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
            let parent = issuer.admin_parent(&caller, &admin).unwrap();
            let checks = Cell::new(0);
            let checked = || {
                checks.set(checks.get() + 1);
                if checks.get() == 2 {
                    if revoked {
                        issuer.revoke(&parent, &bearer).unwrap();
                    } else {
                        cancel.cancel();
                    }
                }
                capture.check(ctx.snapshot(), &write)
            };
            let MemoryStoreError::Confirmed { version, cause } =
                write::publish(pending, checked).await.unwrap_err()
            else {
                panic!("known commit must remain confirmed");
            };
            assert_eq!(version, expected_version);
            assert_eq!(checks.get(), 2);
            if revoked {
                assert!(matches!(cause.as_ref(), MemoryStoreError::Auth(_)));
            } else {
                assert!(matches!(cause.as_ref(), MemoryStoreError::Request(_)));
            }
            drop(capture);
            let fresh_cancel = Cancellation::default();
            let fresh = request(&fresh_cancel);
            let replacement = if revoked {
                let caller = issuer.authenticate_runtime(&secret, &fresh).unwrap();
                let parent = issuer.admin_parent(&caller, &admin).unwrap();
                issuer
                    .issue(&parent, &run, &credential, &permissions, &f.scope())
                    .unwrap()
            } else {
                bearer
            };
            let fresh_grant = issuer
                .verify(
                    replacement.expose_for_transport(),
                    &credential,
                    f.scope(),
                    &fresh,
                )
                .unwrap();
            let fresh_write = authority
                .admit_write(&fresh_grant, &fresh, ProjectWriteOperation::FencedMemory)
                .unwrap();
            let fresh_read = authority
                .admit_read(&fresh_grant, &fresh, ProjectReadModule::Memory)
                .unwrap();
            assert_eq!(
                NativeMemoryStore::reobserve_fenced_mutation(
                    fresh.snapshot(),
                    &fresh_write,
                    initial.id,
                    FencedMemoryMutation::Update(attempt),
                    write_limits()
                )
                .await
                .unwrap(),
                Some(event.clone())
            );
            let history = NativeMemoryStore::history(
                fresh.snapshot(),
                &fresh_read,
                initial.id,
                read_limits(),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(history.generation.sdk_version, version);
            assert_eq!(history.current, after);
            assert_eq!(history.revisions, vec![event]);
        }
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn native_capture_rechecks_cancel_and_revocation_after_acquisition() {
        for revoked in [false, true] {
            let cancel = Cancellation::default();
            let ctx = request(&cancel);
            let f = Fixture::new(&ctx, "main");
            let issuer = f.issuer(&ctx);
            let (bearer, credential, _run, admin) = f.issue_role_for(
                &issuer,
                &ctx,
                f.scope(),
                Role::Dream,
                &[
                    Permission::Read(Module::Memory),
                    Permission::WriteFencedMemory,
                ],
            );
            let grant = issuer
                .verify(bearer.expose_for_transport(), &credential, f.scope(), &ctx)
                .unwrap();
            let authority = ProjectRecordAuthority::new(&issuer);
            let read = authority
                .admit_read(&grant, &ctx, ProjectReadModule::Memory)
                .unwrap();
            let write = authority
                .admit_write(&grant, &ctx, ProjectWriteOperation::FencedMemory)
                .unwrap();
            NativeMemoryStore::fenced_create(
                ctx.snapshot(),
                &write,
                CreateMemory {
                    idempotency_key: "capture-seed".into(),
                    title: "seed".into(),
                    body: "body".into(),
                    attributes: MemoryAttributes {
                        classification: MemoryClass::Fact,
                        mandatory: false,
                        important: false,
                    },
                    tags: vec![],
                    references: None,
                },
                write_limits(),
            )
            .await
            .unwrap();
            let selected = std::path::Path::new("memory").join("index.lance");
            let uri = native::uri(&f.project, &selected).unwrap();
            let version =
                graphit_data::lance_module_store::LanceModuleStore::open(&uri, rows::schema())
                    .await
                    .unwrap()
                    .snapshot()
                    .version();
            let secret =
                std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key"))
                    .unwrap();
            let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
            let parent = issuer.admin_parent(&caller, &admin).unwrap();
            let checks = Cell::new(0);
            let Err(error) = native::capture(
                &f.project,
                &selected,
                ctx.snapshot(),
                read_limits().native,
                true,
                || {
                    checks.set(checks.get() + 1);
                    if checks.get() == 2 {
                        if revoked {
                            issuer.revoke(&parent, &bearer).unwrap();
                        } else {
                            cancel.cancel();
                        }
                    }
                    native::check_read(ctx.snapshot(), &read)
                },
            ) else {
                panic!("late invalidation must not deliver a captured directory");
            };
            assert_eq!(checks.get(), 2);
            if revoked {
                assert!(matches!(error, MemoryStoreError::Auth(_)));
            } else {
                assert!(matches!(error, MemoryStoreError::Request(_)));
            }
            assert_eq!(
                graphit_data::lance_module_store::LanceModuleStore::open(&uri, rows::schema())
                    .await
                    .unwrap()
                    .snapshot()
                    .version(),
                version
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn native_capture_keeps_initial_notfound_when_probe_observes_a_racing_directory() {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        let issuer = f.issuer(&ctx);
        let (bearer, credential, _run, _admin) = f.issue(&issuer, &ctx);
        let grant = issuer
            .verify(bearer.expose_for_transport(), &credential, f.scope(), &ctx)
            .unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let read = authority
            .admit_read(&grant, &ctx, ProjectReadModule::Memory)
            .unwrap();
        let limits = write_limits();
        let artifacts =
            graphit_core::artifact_store::ArtifactStore::new(f.project.clone(), limits.artifacts);
        let mut writer = artifacts.writer_with_context(ctx.snapshot()).unwrap();
        writer
            .create_record_directory(
                graphit_core::artifact_store::ProjectRecordDirectory::Memory,
                limits.read.native,
            )
            .unwrap();
        drop(writer);
        let selected = std::path::Path::new("memory").join("index.lance");
        let index = f.project.runtime_root().join(&selected);
        let checks = Cell::new(0);
        let result = native::capture(
            &f.project,
            &selected,
            ctx.snapshot(),
            read_limits().native,
            true,
            || {
                checks.set(checks.get() + 1);
                if checks.get() == 2 {
                    std::fs::create_dir(&index).unwrap();
                }
                native::check_read(ctx.snapshot(), &read)
            },
        );
        assert!(
            matches!(result, Err(MemoryStoreError::Request(error)) if error.kind() == std::io::ErrorKind::NotFound)
        );
        assert_eq!(checks.get(), 3);
        assert!(index.is_dir());
        read.recheck().unwrap();
    }
}

/// Exact typed attempt used to reobserve a direct FencedMemory mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FencedMemoryMutation {
    Update(FencedUpdateMemory),
    Delete(DeleteMemory),
    Promote(ChangeMemoryImportance),
    Demote(ChangeMemoryImportance),
}
impl FencedMemoryMutation {
    pub(crate) fn into_mutation(self) -> MemoryMutation {
        match self {
            Self::Update(input) => MemoryMutation::Update(UpdateMemory {
                expected_revision: input.expected_revision,
                idempotency_key: input.idempotency_key,
                reason: input.reason,
                title: input.title,
                body: input.body,
                references: input.references,
                classification: None,
                mandatory: None,
                important: None,
                tags: None,
            }),
            Self::Delete(input) => MemoryMutation::Delete(input),
            Self::Promote(input) => MemoryMutation::Promote(input),
            Self::Demote(input) => MemoryMutation::Demote(input),
        }
    }
}

impl NativeMemoryStore {
    /// Set important, or return the current state after a matching revision check.
    pub async fn fenced_promote(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: MemoryId,
        input: ChangeMemoryImportance,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        revision::mutate(
            original,
            admission,
            id,
            MemoryMutation::Promote(input),
            limits,
            WritePolicy::Fenced,
        )
        .await
    }

    /// Clear important independently of mandatory, with the same revision precondition.
    pub async fn fenced_demote(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: MemoryId,
        input: ChangeMemoryImportance,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        revision::mutate(
            original,
            admission,
            id,
            MemoryMutation::Demote(input),
            limits,
            WritePolicy::Fenced,
        )
        .await
    }

    /// Create a Memory with canonical attributes and references under FencedMemory.
    pub async fn fenced_create(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        input: CreateMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        Self::create_with_policy(original, admission, input, limits, WritePolicy::Fenced).await
    }

    /// Update content/references with a current expected revision and immutable metadata.
    pub async fn fenced_update(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: MemoryId,
        input: FencedUpdateMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        revision::mutate(
            original,
            admission,
            id,
            FencedMemoryMutation::Update(input).into_mutation(),
            limits,
            WritePolicy::Fenced,
        )
        .await
    }

    /// Archive the final state and tombstone it in one conditional publication.
    pub async fn fenced_delete(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: MemoryId,
        input: DeleteMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        revision::mutate(
            original,
            admission,
            id,
            FencedMemoryMutation::Delete(input).into_mutation(),
            limits,
            WritePolicy::Fenced,
        )
        .await
    }

    /// Reobserve the original actor/version/key/input with fresh live FencedMemory authority.
    pub async fn reobserve_fenced_create(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        input: CreateMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<MemoryCreateObservation, MemoryStoreError> {
        Self::reobserve_create_with_policy(original, admission, input, limits, WritePolicy::Fenced)
            .await
    }

    /// Observe an exact historical attempt without publishing or replaying it.
    pub async fn reobserve_fenced_mutation(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: MemoryId,
        input: FencedMemoryMutation,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Option<MemoryRevision>, MemoryStoreError> {
        Self::reobserve_mutation_with_policy(
            original,
            admission,
            id,
            input.into_mutation(),
            limits,
            WritePolicy::Fenced,
        )
        .await
    }
}
