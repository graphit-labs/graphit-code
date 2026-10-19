use super::super::write::tests::fixture::*;
use super::*;
use graphit_auth::{Permission, ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::{file_lock::Cancellation, module_settings::Module};
use graphit_hub::native::Role;
use std::{cell::Cell, error::Error, num::NonZeroU128};

const PERMISSIONS: [Permission; 2] = [
    Permission::Read(Module::Memory),
    Permission::WriteFencedMemory,
];
macro_rules! admissions {
    ($f:ident,$ctx:ident,$issuer:ident,$authority:ident,$grant:ident,$read:ident,$write:ident,$bearer:ident,$credential:ident,$run:ident,$admin:ident) => {
        let $issuer = $f.issuer(&$ctx);
        let ($bearer, $credential, $run, $admin) =
            $f.issue_role_for(&$issuer, &$ctx, $f.scope(), Role::Dream, &PERMISSIONS);
        let $grant = $issuer
            .verify(
                $bearer.expose_for_transport(),
                &$credential,
                $f.scope(),
                &$ctx,
            )
            .unwrap();
        let $authority = ProjectRecordAuthority::new(&$issuer);
        let $read = $authority
            .admit_read(&$grant, &$ctx, ProjectReadModule::Memory)
            .unwrap();
        let $write = $authority
            .admit_write(&$grant, &$ctx, ProjectWriteOperation::FencedMemory)
            .unwrap();
    };
}
fn authored(key: &str) -> CreateMemory {
    CreateMemory {
        idempotency_key: key.into(),
        title: "Observed memory".into(),
        body: "Actual source content".into(),
        attributes: MemoryAttributes {
            classification: crate::memory_attributes::MemoryClass::Fact,
            important: false,
            mandatory: false,
        },
        tags: vec![],
        references: None,
    }
}
fn proposal(a: &Memory, b: &Memory) -> FoldMemories {
    FoldMemories {
        kind: MemoryFoldKind::Merge,
        survivor: MemoryFoldMember {
            id: a.id,
            expected_revision: a.revision,
            idempotency_key: "group-survivor".into(),
        },
        members: vec![MemoryFoldMember {
            id: b.id,
            expected_revision: b.revision,
            idempotency_key: "group-member".into(),
        }],
        title: None,
        body: a.body.clone(),
        reason: "Observed consolidation".into(),
    }
}
async fn group_batches(
    capture: &write::WritePin,
    ctx: &graphit_hub::policy::RequestContext<'_>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    input: &FoldMemories,
) -> Vec<arrow_array::RecordBatch> {
    let project = write.project().unwrap();
    let id = native::project_id(project.scope.live).unwrap();
    let before = || {
        check_pair(ctx.snapshot(), read, write)?;
        capture.check(ctx.snapshot(), write)
    };
    let a = live_chain(
        &capture.pin,
        ctx.snapshot(),
        id,
        &input.survivor,
        read_limits(),
        &before,
    )
    .await
    .unwrap();
    let b = live_chain(
        &capture.pin,
        ctx.snapshot(),
        id,
        &input.members[0],
        read_limits(),
        &before,
    )
    .await
    .unwrap();
    let at = now().unwrap();
    let actor = write::actor(write).unwrap();
    let anchor = event(
        ctx.snapshot(),
        id,
        project.version.version(),
        &actor,
        at,
        &a.current,
        MemoryMutation::FoldSurvivor(MemoryFoldSurvivor {
            input: input.clone(),
            observations: vec![
                FoldAttributeObservation {
                    id: a.current.id,
                    revision: a.current.revision,
                    attributes: a.current.attributes,
                },
                FoldAttributeObservation {
                    id: b.current.id,
                    revision: b.current.revision,
                    attributes: b.current.attributes,
                },
            ],
        }),
    )
    .unwrap();
    let removal = event(
        ctx.snapshot(),
        id,
        project.version.version(),
        &actor,
        at,
        &b.current,
        delete_operation(input, &input.members[0]).unwrap(),
    )
    .unwrap();
    vec![
        rows::mutation_batch(&a.creation, &anchor, write_limits()).unwrap(),
        rows::mutation_batch(&b.creation, &removal, write_limits()).unwrap(),
    ]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automatic_real_sdk_predecessor_conflict_leaves_entire_group_unpublished() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, admission, bearer, credential, _run, _admin
    );
    let a =
        NativeMemoryStore::fenced_create(ctx.snapshot(), &admission, authored("a"), write_limits())
            .await
            .unwrap();
    let b =
        NativeMemoryStore::fenced_create(ctx.snapshot(), &admission, authored("b"), write_limits())
            .await
            .unwrap();
    let input = proposal(&a, &b);
    let creation = NativeMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
        .await
        .unwrap()
        .unwrap()
        .creation;
    let capture = write::acquire_with_check(
        ctx.snapshot(),
        &admission,
        write_limits(),
        false,
        native::WritePolicy::Fenced,
        || check_pair(ctx.snapshot(), &read, &admission),
    )
    .await
    .unwrap()
    .unwrap();
    let pending = capture
        .pin
        .prepare_merge(
            group_batches(&capture, &ctx, &read, &admission, &input).await,
            vec!["row_key".into()],
        )
        .await
        .unwrap();
    let recovery = pending.recovery_key().clone();
    let mut external = creation;
    external.memory.id = MemoryId::new(NonZeroU128::new(91).unwrap());
    external.input.idempotency_key = "external".into();
    capture
        .pin
        .prepare_merge(
            vec![rows::batch(&external, write_limits()).unwrap()],
            vec!["row_key".into()],
        )
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    let result = write::publish_version(pending, || {
        check_pair(ctx.snapshot(), &read, &admission)?;
        capture.check(ctx.snapshot(), &admission)
    })
    .await;
    let MemoryStoreError::Commit(failure) = result.unwrap_err() else {
        panic!("actual predecessor failure")
    };
    assert_eq!(failure.recovery_key(), &recovery);
    assert!(failure.source().is_some());
    drop(capture);
    let current = NativeMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    let version = current.generation.sdk_version;
    assert_eq!(current.memory, a);
    assert_eq!(
        NativeMemoryStore::get(ctx.snapshot(), &read, b.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory,
        b
    );
    assert!(
        NativeMemoryStore::reobserve_automatic_fold(
            ctx.snapshot(),
            &read,
            &admission,
            input,
            write_limits()
        )
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(
        NativeMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .generation
            .sdk_version,
        version
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automatic_real_confirmed_group_cancel_or_revoke_then_fresh_read_only_observation() {
    for revoke in [false, true] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        admissions!(
            f, ctx, issuer, authority, grant, read, admission, bearer, credential, run, admin
        );
        let a = NativeMemoryStore::fenced_create(
            ctx.snapshot(),
            &admission,
            authored("a"),
            write_limits(),
        )
        .await
        .unwrap();
        let b = NativeMemoryStore::fenced_create(
            ctx.snapshot(),
            &admission,
            authored("b"),
            write_limits(),
        )
        .await
        .unwrap();
        let input = proposal(&a, &b);
        let capture = write::acquire_with_check(
            ctx.snapshot(),
            &admission,
            write_limits(),
            false,
            native::WritePolicy::Fenced,
            || check_pair(ctx.snapshot(), &read, &admission),
        )
        .await
        .unwrap()
        .unwrap();
        let pending = capture
            .pin
            .prepare_merge(
                group_batches(&capture, &ctx, &read, &admission, &input).await,
                vec!["row_key".into()],
            )
            .await
            .unwrap();
        let version = capture.pin.version() + 1;
        let secret =
            std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
        let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
        let parent = issuer.admin_parent(&caller, &admin).unwrap();
        let checks = Cell::new(0);
        let result = write::publish_version(pending, || {
            checks.set(checks.get() + 1);
            if checks.get() == 2 {
                if revoke {
                    issuer.revoke(&parent, &bearer).unwrap();
                } else {
                    cancel.cancel();
                }
            }
            check_pair(ctx.snapshot(), &read, &admission)?;
            capture.check(ctx.snapshot(), &admission)
        })
        .await;
        let MemoryStoreError::Confirmed {
            version: confirmed,
            cause,
        } = result.unwrap_err()
        else {
            panic!("actual confirmed group")
        };
        assert_eq!(confirmed, version);
        assert!(if revoke {
            matches!(cause.as_ref(), MemoryStoreError::Auth(_))
        } else {
            matches!(cause.as_ref(), MemoryStoreError::Request(_))
        });
        drop(capture);
        let fresh_cancel = Cancellation::default();
        let fresh = request(&fresh_cancel);
        let fresh_bearer = if revoke {
            assert!(
                issuer
                    .verify(
                        bearer.expose_for_transport(),
                        &credential,
                        f.scope(),
                        &fresh
                    )
                    .is_err()
            );
            issuer
                .issue(&parent, &run, &credential, &PERMISSIONS, &f.scope())
                .unwrap()
        } else {
            bearer
        };
        let fresh_grant = issuer
            .verify(
                fresh_bearer.expose_for_transport(),
                &credential,
                f.scope(),
                &fresh,
            )
            .unwrap();
        let fresh_read = authority
            .admit_read(&fresh_grant, &fresh, ProjectReadModule::Memory)
            .unwrap();
        let fresh_write = authority
            .admit_write(&fresh_grant, &fresh, ProjectWriteOperation::FencedMemory)
            .unwrap();
        let observation = NativeMemoryStore::reobserve_automatic_fold(
            fresh.snapshot(),
            &fresh_read,
            &fresh_write,
            input.clone(),
            write_limits(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(observation.survivor.after_revision, 2);
        assert_eq!(observation.removed[0].after_revision, 2);
        assert!(observation.survivor.sdk_version.is_none());
        assert!(observation.removed[0].sdk_version.is_none());
        assert!(
            NativeMemoryStore::get(fresh.snapshot(), &fresh_read, b.id, read_limits())
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            NativeMemoryStore::automatic_fold(
                fresh.snapshot(),
                &fresh_read,
                &fresh_write,
                input,
                write_limits()
            )
            .await
            .unwrap(),
            observation
        );
        assert_eq!(
            NativeMemoryStore::get(fresh.snapshot(), &fresh_read, a.id, read_limits())
                .await
                .unwrap()
                .unwrap()
                .generation
                .sdk_version,
            version
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automatic_capture_rechecks_independent_grants_after_acquisition_boundary() {
    for writer_capture in [true, false] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        admissions!(
            f,
            ctx,
            issuer,
            authority,
            grant,
            _seed_read,
            _seed_write,
            _seed_bearer,
            credential,
            run,
            admin
        );
        let secret =
            std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
        let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
        let parent = issuer.admin_parent(&caller, &admin).unwrap();
        let read_bearer = issuer
            .issue(
                &parent,
                &run,
                &credential,
                &[Permission::Read(Module::Memory)],
                &f.scope(),
            )
            .unwrap();
        let write_bearer = issuer
            .issue(
                &parent,
                &run,
                &credential,
                &[Permission::WriteFencedMemory],
                &f.scope(),
            )
            .unwrap();
        let read_grant = issuer
            .verify(
                read_bearer.expose_for_transport(),
                &credential,
                f.scope(),
                &ctx,
            )
            .unwrap();
        let write_grant = issuer
            .verify(
                write_bearer.expose_for_transport(),
                &credential,
                f.scope(),
                &ctx,
            )
            .unwrap();
        let read = authority
            .admit_read(&read_grant, &ctx, ProjectReadModule::Memory)
            .unwrap();
        let admission = authority
            .admit_write(&write_grant, &ctx, ProjectWriteOperation::FencedMemory)
            .unwrap();
        let checks = Cell::new(0);
        let paired = || {
            checks.set(checks.get() + 1);
            if checks.get() == 2 {
                issuer
                    .revoke(
                        &parent,
                        if writer_capture {
                            &read_bearer
                        } else {
                            &write_bearer
                        },
                    )
                    .unwrap();
            }
            check_pair(ctx.snapshot(), &read, &admission)
        };
        let denied = if writer_capture {
            write::acquire_with_check(
                ctx.snapshot(),
                &admission,
                write_limits(),
                false,
                native::WritePolicy::Fenced,
                paired,
            )
            .await
            .err()
        } else {
            native::read_pin_with_check(ctx.snapshot(), &read, read_limits().native, paired)
                .await
                .err()
        };
        assert!(matches!(denied, Some(MemoryStoreError::Auth(_))));
        assert_eq!(checks.get(), 2);
        assert!(!f.project.runtime_root().join("memory").exists());
    }
}
