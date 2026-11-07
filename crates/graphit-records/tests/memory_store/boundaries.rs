use super::*;
use crate::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store as memory_api,
};
pub(in crate::memory_store) mod fixture {
    include!("fixture.rs");
}
use fixture::*;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::file_lock::Cancellation;
use std::{cell::Cell, error::Error, num::NonZeroU32};

fn authored(key: &str) -> CreateMemory {
    CreateMemory {
        idempotency_key: key.into(),
        title: "Memory".into(),
        body: "Body".into(),
        attributes: MemoryAttributes {
            classification: MemoryClass::Fact,
            mandatory: false,
            important: true,
        },
        tags: vec![],
        references: None,
    }
}
macro_rules! setup {
    ($f:ident,$ctx:ident,$issuer:ident,$authority:ident,$grant:ident,$read:ident,$write:ident,$bearer:ident,$reference:ident,$admin:ident,$run:ident) => {
        let $issuer = $f.issuer(&$ctx);
        let ($bearer, $reference, $run, $admin) = $f.issue(&$issuer, &$ctx);
        let $grant = $issuer
            .verify(
                $bearer.expose_for_transport(),
                &$reference,
                $f.scope(),
                &$ctx,
            )
            .unwrap();
        let $authority = ProjectRecordAuthority::new(&$issuer);
        let $read = $authority
            .admit_read(&$grant, &$ctx, ProjectReadModule::Memory)
            .unwrap();
        let $write = $authority
            .admit_write(&$grant, &$ctx, ProjectWriteOperation::Memory)
            .unwrap();
    };
    ($f:ident,$ctx:ident,$issuer:ident,$authority:ident,$grant:ident,$read:ident,$write:ident,$bearer:ident,$reference:ident,$admin:ident) => {
        setup!(
            $f, $ctx, $issuer, $authority, $grant, $read, $write, $bearer, $reference, $admin, _run
        );
    };
}
fn event(base: &CreationMemory, key: &str, id: u128) -> CreationMemory {
    let mut e = base.clone();
    e.input.idempotency_key = key.into();
    e.memory.id = MemoryId::new(NonZeroU128::new(id).unwrap());
    e
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_stale_sdk_delta_retains_cause_key_and_authorized_unresolved() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    setup!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let seed = NativeMemoryStore::create(ctx.snapshot(), &write, authored("seed"), write_limits())
        .await
        .unwrap();
    let base = NativeMemoryStore::get(ctx.snapshot(), &read, seed.id, read_limits())
        .await
        .unwrap()
        .unwrap()
        .creation;
    let MemoryLexicalIndexOutcome::Published(index) = NativeMemoryStore::maintain_lexical_index(
        ctx.snapshot(),
        &write,
        MemoryLexicalIndexOperation::Refresh,
        write_limits(),
    )
    .await
    .unwrap() else {
        panic!("actual index publication");
    };
    let capture = acquire(ctx.snapshot(), &write, write_limits(), false)
        .await
        .unwrap()
        .unwrap();
    let failed = event(&base, "failed", 77);
    let foreign = event(&base, "foreign", 78);
    let pending = capture
        .pin
        .prepare_merge(
            vec![rows::batch(&failed, write_limits()).unwrap()],
            vec!["row_key".into()],
        )
        .await
        .unwrap();
    let key = pending.recovery_key().clone();
    capture
        .pin
        .prepare_merge(
            vec![rows::batch(&foreign, write_limits()).unwrap()],
            vec!["row_key".into()],
        )
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    let before = || {
        native::check_write(ctx.snapshot(), &write)?;
        capture.guard.revalidate(ctx.snapshot())?;
        Ok(())
    };
    let error = super::super::lexical_index::preserve_publication(
        publish(pending, before).await,
        Some(index.clone()),
    )
    .unwrap_err();
    let MemoryStoreError::IndexConfirmed { publication, cause } = error else {
        panic!("known index and failed CRUD");
    };
    assert_eq!(*publication, index);
    let error = *cause;
    let MemoryStoreError::Commit(failure) = error else {
        panic!("actual SDK delta failure");
    };
    assert_eq!(failure.recovery_key(), &key);
    assert!(failure.source().is_some());
    drop(capture);
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &fresh)
        .unwrap();
    let write = authority
        .admit_write(&grant, &fresh, ProjectWriteOperation::Memory)
        .unwrap();
    let read = authority
        .admit_read(&grant, &fresh, ProjectReadModule::Memory)
        .unwrap();
    assert_eq!(
        NativeMemoryStore::resolve_lexical_index(
            fresh.snapshot(),
            &write,
            &index.recovery_key,
            write_limits()
        )
        .await
        .unwrap(),
        graphit_data::lance_branch_store::DeltaResolution::Applied {
            version: index.generation.sdk_version
        }
    );
    let before = NativeMemoryStore::observe_heads(fresh.snapshot(), &read, head_limits())
        .await
        .unwrap();
    assert_eq!(
        NativeMemoryStore::reobserve_create(fresh.snapshot(), &write, failed.input, write_limits())
            .await
            .unwrap(),
        MemoryCreateObservation::Unresolved
    );
    assert_eq!(
        NativeMemoryStore::observe_heads(fresh.snapshot(), &read, head_limits())
            .await
            .unwrap(),
        before
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_confirmed_delta_then_cancel_keeps_history_and_fresh_found_without_write() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    setup!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let seed = NativeMemoryStore::create(ctx.snapshot(), &write, authored("seed"), write_limits())
        .await
        .unwrap();
    let base = NativeMemoryStore::get(ctx.snapshot(), &read, seed.id, read_limits())
        .await
        .unwrap()
        .unwrap()
        .creation;
    let MemoryLexicalIndexOutcome::Published(index) = NativeMemoryStore::maintain_lexical_index(
        ctx.snapshot(),
        &write,
        MemoryLexicalIndexOperation::Refresh,
        write_limits(),
    )
    .await
    .unwrap() else {
        panic!("actual index publication");
    };
    let capture = acquire(ctx.snapshot(), &write, write_limits(), false)
        .await
        .unwrap()
        .unwrap();
    let confirmed = event(&base, "confirmed", 79);
    let pending = capture
        .pin
        .prepare_merge(
            vec![rows::batch(&confirmed, write_limits()).unwrap()],
            vec!["row_key".into()],
        )
        .await
        .unwrap();
    let expected = capture.pin.version() + 1;
    let checks = Cell::new(0);
    // Drive the real request cancellation at the post-commit check. No SDK or Auth cause is mocked.
    let before = || {
        checks.set(checks.get() + 1);
        if checks.get() == 2 {
            cancel.cancel();
        }
        native::check_write(ctx.snapshot(), &write)?;
        capture.guard.revalidate(ctx.snapshot())?;
        Ok(())
    };
    let error = super::super::lexical_index::preserve_publication(
        publish(pending, before).await,
        Some(index.clone()),
    )
    .unwrap_err();
    let MemoryStoreError::Confirmed { version, cause } = error else {
        panic!("confirmed outcome");
    };
    assert_eq!(version, expected);
    let MemoryStoreError::IndexConfirmed { publication, cause } = *cause else {
        panic!("retained index effect");
    };
    assert_eq!(*publication, index);
    assert!(matches!(cause.as_ref(), MemoryStoreError::Request(_)));
    drop(capture);
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &fresh)
        .unwrap();
    let write = authority
        .admit_write(&grant, &fresh, ProjectWriteOperation::Memory)
        .unwrap();
    let read = authority
        .admit_read(&grant, &fresh, ProjectReadModule::Memory)
        .unwrap();
    assert_eq!(
        NativeMemoryStore::reobserve_create(
            fresh.snapshot(),
            &write,
            confirmed.input.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        MemoryCreateObservation::Found(confirmed.memory.clone())
    );
    let detail =
        NativeMemoryStore::get(fresh.snapshot(), &read, confirmed.memory.id, read_limits())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(detail.creation, confirmed);
    assert_eq!(detail.generation.sdk_version, expected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn old_heads_pin_stays_complete_after_other_actor_publication() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    setup!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let first =
        NativeMemoryStore::create(ctx.snapshot(), &write, authored("first"), write_limits())
            .await
            .unwrap();
    let old = native::read_pin(ctx.snapshot(), &read, read_limits().native)
        .await
        .unwrap()
        .unwrap();
    let (other, _) = f.issue_registered_for(&issuer, &ctx, f.scope(), &reference);
    let othergrant = issuer
        .verify(other.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let otherwrite = authority
        .admit_write(&othergrant, &ctx, ProjectWriteOperation::Memory)
        .unwrap();
    let second = NativeMemoryStore::create(
        ctx.snapshot(),
        &otherwrite,
        authored("second"),
        write_limits(),
    )
    .await
    .unwrap();
    assert_ne!(first.updated_by, second.updated_by);
    let before = || {
        native::check_read(ctx.snapshot(), &read)?;
        old.guard.revalidate(ctx.snapshot())?;
        Ok(())
    };
    let heads = read::collect_heads(&old.pin, ctx.snapshot(), head_limits(), before)
        .await
        .unwrap();
    assert_eq!(heads.len(), 1);
    assert_eq!(heads[0].id, first.id);
    let MemoryHeadRead::Published { generation, heads } =
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .unwrap()
    else {
        panic!("published");
    };
    assert_eq!(heads.len(), 2);
    assert!(generation.sdk_version > old.pin.version());
    // Preserve the credential in this real fixture, without exposing it in diagnostics.
    assert!(
        issuer
            .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
            .is_ok()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn late_cancel_revoke_and_captured_directory_replacement_discard_partial_heads() {
    for failure in 0..3 {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        setup!(
            f, ctx, issuer, authority, grant, read, write, bearer, reference, admin
        );
        for key in ["first", "second"] {
            NativeMemoryStore::create(ctx.snapshot(), &write, authored(key), write_limits())
                .await
                .unwrap();
        }
        let capture = native::read_pin(ctx.snapshot(), &read, read_limits().native)
            .await
            .unwrap()
            .unwrap();
        let secret =
            std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
        let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
        let parent = issuer.admin_parent(&caller, &admin).unwrap();
        let checks = Cell::new(0);
        let mut limits = head_limits();
        limits.batch_rows = NonZeroU32::new(1).unwrap();
        let before = || {
            checks.set(checks.get() + 1);
            if checks.get() == 5 {
                match failure {
                    0 => cancel.cancel(),
                    1 => issuer.revoke(&parent, &bearer).unwrap(),
                    _ => {
                        let index = f.project.runtime_root().join("memory/index.lance");
                        std::fs::rename(&index, index.with_file_name("moved.lance")).unwrap();
                        std::fs::create_dir(index).unwrap();
                    }
                }
            }
            native::check_read(ctx.snapshot(), &read)?;
            capture.guard.revalidate(ctx.snapshot())?;
            Ok(())
        };
        assert!(
            read::collect_heads(&capture.pin, ctx.snapshot(), limits, before)
                .await
                .is_err()
        );
        assert_eq!(checks.get(), 5);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn embedding_stream_boundaries_recheck_auth_cancel_and_namespace() {
    for semantic in [false, true] {
        for failure in 0..3 {
            let cancel = Cancellation::default();
            let ctx = request(&cancel);
            let f = Fixture::new(&ctx, "main");
            setup!(
                f, ctx, issuer, authority, grant, read, write, bearer, reference, admin
            );
            for key in ["first", "second"] {
                NativeMemoryStore::create(ctx.snapshot(), &write, authored(key), write_limits())
                    .await
                    .unwrap();
            }
            let capture = native::read_pin(ctx.snapshot(), &read, read_limits().native)
                .await
                .unwrap()
                .unwrap();
            let secret =
                std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key"))
                    .unwrap();
            let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
            let parent = issuer.admin_parent(&caller, &admin).unwrap();
            let checks = Cell::new(0);
            let mut limits = read_limits();
            limits.batch_rows = NonZeroU32::new(1).unwrap();
            let before = || {
                checks.set(checks.get() + 1);
                if checks.get() == 5 {
                    match failure {
                        0 => cancel.cancel(),
                        1 => issuer.revoke(&parent, &bearer).unwrap(),
                        _ => {
                            let index = f.project.runtime_root().join("memory/index.lance");
                            std::fs::rename(&index, index.with_file_name("moved.lance")).unwrap();
                            std::fs::create_dir(index).unwrap();
                        }
                    }
                }
                native::check_read(ctx.snapshot(), &read)?;
                capture.guard.revalidate(ctx.snapshot())?;
                Ok(())
            };
            let project = native::project_id(&f.project).unwrap();
            let owner = crate::memory_store::embeddings::SourceOwner::Project(project);
            let result = if semantic {
                crate::memory_store::embeddings::query::rank_groups(
                    &crate::memory_store::embeddings::query::EmbeddingScan {
                        pin: &capture.pin,
                        original: ctx.snapshot(),
                        owner,
                        limits,
                    },
                    &memory_api::MemoryEmbeddingIdentity::new(
                        [1; 32],
                        NonZeroU32::new(2).unwrap(),
                        memory_api::MEMORY_DOCUMENT_RECIPE.into(),
                    )
                    .unwrap(),
                    &[1., 0.],
                    &MemoryListFilter::default(),
                    false,
                    std::collections::HashMap::new(),
                    before,
                )
                .await
                .map(|_| ())
            } else {
                crate::memory_store::embeddings::capture::discover_ids(
                    &capture.pin,
                    ctx.snapshot(),
                    owner,
                    limits,
                    before,
                )
                .await
                .map(|_| ())
            };
            assert!(result.is_err(), "semantic={semantic}, failure={failure}");
            assert!(
                checks.get() >= 5,
                "failure must occur after stream progress"
            );
        }
    }
}

#[test]
fn projected_schema_rejects_nullability_metadata_and_budget_overflow() {
    let mut total = usize::MAX;
    assert!(matches!(
        charge(&mut total, 1, usize::MAX, "overflow"),
        Err(MemoryStoreError::Limit("overflow"))
    ));
    let mut fields = rows::head_schema()
        .fields()
        .iter()
        .map(|f| f.as_ref().clone())
        .collect::<Vec<_>>();
    fields[0] = arrow_schema::Field::new("record_id", arrow_schema::DataType::Utf8, true);
    let bad = arrow_array::RecordBatch::new_empty(Arc::new(
        arrow_schema::Schema::new_with_metadata(fields, rows::head_schema().metadata().clone()),
    ));
    assert!(rows::validate_schema(&bad, &rows::head_schema()).is_err());
    let bad = arrow_array::RecordBatch::new_empty(Arc::new(arrow_schema::Schema::new(
        rows::head_schema().fields().clone(),
    )));
    assert!(rows::validate_schema(&bad, &rows::head_schema()).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_confirmed_delta_then_revoke_requires_reissued_same_run_authority() {
    use graphit_auth::Permission;
    use graphit_core::module_settings::Module;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    setup!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, admin, run
    );
    let seed = NativeMemoryStore::create(ctx.snapshot(), &write, authored("seed"), write_limits())
        .await
        .unwrap();
    let base = NativeMemoryStore::get(ctx.snapshot(), &read, seed.id, read_limits())
        .await
        .unwrap()
        .unwrap()
        .creation;
    let capture = acquire(ctx.snapshot(), &write, write_limits(), false)
        .await
        .unwrap()
        .unwrap();
    let confirmed = event(&base, "revoked", 80);
    let pending = capture
        .pin
        .prepare_merge(
            vec![rows::batch(&confirmed, write_limits()).unwrap()],
            vec!["row_key".into()],
        )
        .await
        .unwrap();
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let checks = Cell::new(0);
    let expected = capture.pin.version() + 1;
    let before = || {
        checks.set(checks.get() + 1);
        if checks.get() == 2 {
            issuer.revoke(&parent, &bearer).unwrap();
        }
        native::check_write(ctx.snapshot(), &write)?;
        capture.guard.revalidate(ctx.snapshot())?;
        Ok(())
    };
    let MemoryStoreError::Confirmed { version, cause } =
        publish(pending, before).await.unwrap_err()
    else {
        panic!("confirmed SDK");
    };
    assert_eq!(version, expected);
    assert!(matches!(cause.as_ref(), MemoryStoreError::Auth(_)));
    drop(capture);
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    assert!(
        issuer
            .verify(bearer.expose_for_transport(), &reference, f.scope(), &fresh)
            .is_err()
    );
    let new_bearer = issuer
        .issue(
            &parent,
            &run,
            &reference,
            &[Permission::Read(Module::Memory), Permission::WriteMemory],
            &f.scope(),
        )
        .unwrap();
    let freshgrant = issuer
        .verify(
            new_bearer.expose_for_transport(),
            &reference,
            f.scope(),
            &fresh,
        )
        .unwrap();
    let freshwrite = authority
        .admit_write(&freshgrant, &fresh, ProjectWriteOperation::Memory)
        .unwrap();
    let freshread = authority
        .admit_read(&freshgrant, &fresh, ProjectReadModule::Memory)
        .unwrap();
    assert_eq!(
        NativeMemoryStore::reobserve_create(
            fresh.snapshot(),
            &freshwrite,
            confirmed.input.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        MemoryCreateObservation::Found(confirmed.memory.clone())
    );
    let detail = NativeMemoryStore::get(
        fresh.snapshot(),
        &freshread,
        confirmed.memory.id,
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(detail.generation.sdk_version, expected);
    assert_eq!(detail.creation.actor, base.actor);
    let references = NativeMemoryStore::references(
        fresh.snapshot(),
        &freshread,
        MemoryReferencesQuery {
            outgoing: Some(confirmed.memory.id),
            incoming: None,
            page_size: std::num::NonZeroUsize::new(20).unwrap(),
            outgoing_cursor: None,
            incoming_cursor: None,
        },
        read_limits(),
    )
    .await
    .unwrap();
    let MemoryReferencesRead::Published {
        generation,
        outgoing,
        coverage,
        warnings,
        ..
    } = references
    else {
        panic!("confirmed publication must be readable")
    };
    assert_eq!(generation, detail.generation);
    assert!(
        outgoing.unwrap().complete
            && coverage.memory_project_source_complete
            && warnings.is_empty()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exact_authored_and_encoded_batch_limits_pass_and_first_excess_fails() {
    use std::num::NonZeroUsize;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    setup!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    let seed = NativeMemoryStore::create(ctx.snapshot(), &write, authored("seed"), write_limits())
        .await
        .unwrap();
    let creation = NativeMemoryStore::get(ctx.snapshot(), &read, seed.id, read_limits())
        .await
        .unwrap()
        .unwrap()
        .creation;
    let input_bytes = creation.input.idempotency_key.len()
        + creation.input.title.len()
        + creation.input.body.len();
    input_budget(ctx.snapshot(), &creation.input, input_bytes).unwrap();
    assert!(matches!(
        input_budget(ctx.snapshot(), &creation.input, input_bytes - 1),
        Err(MemoryStoreError::Limit(_))
    ));
    let batch = rows::batch(&creation, write_limits()).unwrap();
    let maximum = (0..batch.num_rows())
        .map(|row| {
            batch
                .columns()
                .iter()
                .filter_map(|a| a.as_any().downcast_ref::<arrow_array::StringArray>())
                .map(|a| a.value(row).len())
                .sum::<usize>()
        })
        .max()
        .unwrap();
    let mut limits = write_limits();
    limits.max_row_bytes = NonZeroUsize::new(maximum).unwrap();
    rows::batch(&creation, limits).unwrap();
    limits.max_row_bytes = NonZeroUsize::new(maximum - 1).unwrap();
    assert!(matches!(
        rows::batch(&creation, limits),
        Err(MemoryStoreError::Limit(_))
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalogue_pin_is_stable_and_late_request_cancellation_returns_no_prefix() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    setup!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    let initial = NativeMemoryStore::create(
        ctx.snapshot(),
        &write,
        authored("catalogue-pin"),
        write_limits(),
    )
    .await
    .unwrap();
    let old = native::read_pin(ctx.snapshot(), &read, read_limits().native)
        .await
        .unwrap()
        .unwrap();
    NativeMemoryStore::create(
        ctx.snapshot(),
        &write,
        authored("catalogue-later"),
        write_limits(),
    )
    .await
    .unwrap();
    let project_id = native::project_id(read.project().unwrap().scope.live).unwrap();
    let before = || {
        native::check_read(ctx.snapshot(), &read)?;
        old.guard.revalidate(ctx.snapshot())?;
        Ok(())
    };
    assert_eq!(
        crate::memory_store::catalogue::collect(
            &old.pin,
            ctx.snapshot(),
            project_id,
            &MemoryListFilter::default(),
            read_limits(),
            before
        )
        .await
        .unwrap(),
        vec![initial]
    );
    let mut limits = read_limits();
    limits.batch_rows = NonZeroU32::new(1).unwrap();
    let checks = Cell::new(0);
    let before = || {
        checks.set(checks.get() + 1);
        if checks.get() == 5 {
            cancel.cancel();
        }
        native::check_read(ctx.snapshot(), &read)?;
        old.guard.revalidate(ctx.snapshot())?;
        Ok(())
    };
    let result = crate::memory_store::catalogue::collect(
        &old.pin,
        ctx.snapshot(),
        project_id,
        &MemoryListFilter::default(),
        limits,
        before,
    )
    .await;
    assert!(matches!(result, Err(MemoryStoreError::Request(_))));
    assert_eq!(checks.get(), 5);
}

fn mutation(key: &str, expected_revision: u64) -> MemoryMutation {
    MemoryMutation::Update(UpdateMemory {
        expected_revision,
        idempotency_key: key.into(),
        reason: "Bounded mutation proof".into(),
        title: None,
        body: Some("Mutated body".into()),
        classification: None,
        mandatory: None,
        important: None,
        tags: None,
        references: None,
    })
}
fn mutation_event(
    original: &graphit_core::file_io::SnapshotContext,
    creation: &CreationMemory,
    operation: MemoryMutation,
) -> MemoryRevision {
    let before = creation.memory.clone();
    let after = crate::memory_store::revision::derive(
        original,
        creation.project_id,
        &before,
        &operation,
        &creation.actor,
        before.updated_at + 1,
    )
    .unwrap();
    MemoryRevision {
        project_id: creation.project_id,
        operation_version: creation.operation_version.clone(),
        actor: creation.actor.clone(),
        operation,
        before,
        after,
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_stale_revision_sdk_delta_preserves_recovery_key_and_current_chain() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    setup!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    let m = NativeMemoryStore::create(ctx.snapshot(), &write, authored("base"), write_limits())
        .await
        .unwrap();
    let creation = NativeMemoryStore::get(ctx.snapshot(), &read, m.id, read_limits())
        .await
        .unwrap()
        .unwrap()
        .creation;
    let capture = acquire(ctx.snapshot(), &write, write_limits(), false)
        .await
        .unwrap()
        .unwrap();
    let failed = mutation_event(ctx.snapshot(), &creation, mutation("failed-revision", 1));
    let foreign = mutation_event(ctx.snapshot(), &creation, mutation("foreign-revision", 1));
    let pending = capture
        .pin
        .prepare_merge(
            vec![rows::mutation_batch(&creation, &failed, write_limits()).unwrap()],
            vec!["row_key".into()],
        )
        .await
        .unwrap();
    let key = pending.recovery_key().clone();
    capture
        .pin
        .prepare_merge(
            vec![rows::mutation_batch(&creation, &foreign, write_limits()).unwrap()],
            vec!["row_key".into()],
        )
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    let before = || {
        native::check_write(ctx.snapshot(), &write)?;
        capture.guard.revalidate(ctx.snapshot())?;
        Ok(())
    };
    let error = publish(pending, before).await.unwrap_err();
    let MemoryStoreError::Commit(failure) = error else {
        panic!("SDK cause")
    };
    assert_eq!(failure.recovery_key(), &key);
    assert!(failure.source().is_some());
    drop(capture);
    let h = NativeMemoryStore::history(ctx.snapshot(), &read, m.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(h.revisions, vec![foreign.clone()]);
    assert_eq!(h.current, foreign.after);
    assert!(
        NativeMemoryStore::reobserve_mutation(
            ctx.snapshot(),
            &write,
            m.id,
            failed.operation,
            write_limits()
        )
        .await
        .unwrap()
        .is_none()
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn confirmed_revision_then_cancel_keeps_event_and_fresh_reobservation_is_read_only() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    setup!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let m = NativeMemoryStore::create(ctx.snapshot(), &write, authored("base"), write_limits())
        .await
        .unwrap();
    let creation = NativeMemoryStore::get(ctx.snapshot(), &read, m.id, read_limits())
        .await
        .unwrap()
        .unwrap()
        .creation;
    let capture = acquire(ctx.snapshot(), &write, write_limits(), false)
        .await
        .unwrap()
        .unwrap();
    let event = mutation_event(ctx.snapshot(), &creation, mutation("confirmed-revision", 1));
    let pending = capture
        .pin
        .prepare_merge(
            vec![rows::mutation_batch(&creation, &event, write_limits()).unwrap()],
            vec!["row_key".into()],
        )
        .await
        .unwrap();
    let version = capture.pin.version() + 1;
    let checks = Cell::new(0);
    let before = || {
        checks.set(checks.get() + 1);
        if checks.get() == 2 {
            cancel.cancel();
        }
        native::check_write(ctx.snapshot(), &write)?;
        capture.guard.revalidate(ctx.snapshot())?;
        Ok(())
    };
    let MemoryStoreError::Confirmed {
        version: confirmed,
        cause,
    } = publish(pending, before).await.unwrap_err()
    else {
        panic!("confirmed")
    };
    assert_eq!(confirmed, version);
    assert!(matches!(cause.as_ref(), MemoryStoreError::Request(_)));
    drop(capture);
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    let g = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &fresh)
        .unwrap();
    let w = authority
        .admit_write(&g, &fresh, ProjectWriteOperation::Memory)
        .unwrap();
    let r = authority
        .admit_read(&g, &fresh, ProjectReadModule::Memory)
        .unwrap();
    assert_eq!(
        NativeMemoryStore::reobserve_mutation(
            fresh.snapshot(),
            &w,
            m.id,
            event.operation.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        Some(event.clone())
    );
    let h = NativeMemoryStore::history(fresh.snapshot(), &r, m.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(h.current, event.after);
    assert_eq!(h.generation.sdk_version, version);
    assert_eq!(h.revisions, vec![event]);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn confirmed_revision_then_revoke_requires_reissued_same_actor_authority() {
    use graphit_auth::Permission;
    use graphit_core::module_settings::Module;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    setup!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, admin, run
    );
    let m = NativeMemoryStore::create(ctx.snapshot(), &write, authored("base"), write_limits())
        .await
        .unwrap();
    let creation = NativeMemoryStore::get(ctx.snapshot(), &read, m.id, read_limits())
        .await
        .unwrap()
        .unwrap()
        .creation;
    let capture = acquire(ctx.snapshot(), &write, write_limits(), false)
        .await
        .unwrap()
        .unwrap();
    let event = mutation_event(ctx.snapshot(), &creation, mutation("confirmed-revision", 1));
    let pending = capture
        .pin
        .prepare_merge(
            vec![rows::mutation_batch(&creation, &event, write_limits()).unwrap()],
            vec!["row_key".into()],
        )
        .await
        .unwrap();
    let version = capture.pin.version() + 1;
    let checks = Cell::new(0);
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let before = || {
        checks.set(checks.get() + 1);
        if checks.get() == 2 {
            issuer.revoke(&parent, &bearer).unwrap();
        }
        native::check_write(ctx.snapshot(), &write)?;
        capture.guard.revalidate(ctx.snapshot())?;
        Ok(())
    };
    let MemoryStoreError::Confirmed {
        version: confirmed,
        cause,
    } = publish(pending, before).await.unwrap_err()
    else {
        panic!("confirmed")
    };
    assert_eq!(confirmed, version);
    assert!(matches!(cause.as_ref(), MemoryStoreError::Auth(_)));
    drop(capture);
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    assert!(
        issuer
            .verify(bearer.expose_for_transport(), &reference, f.scope(), &fresh)
            .is_err()
    );
    let new_bearer = issuer
        .issue(
            &parent,
            &run,
            &reference,
            &[Permission::Read(Module::Memory), Permission::WriteMemory],
            &f.scope(),
        )
        .unwrap();
    let g = issuer
        .verify(
            new_bearer.expose_for_transport(),
            &reference,
            f.scope(),
            &fresh,
        )
        .unwrap();
    let w = authority
        .admit_write(&g, &fresh, ProjectWriteOperation::Memory)
        .unwrap();
    let r = authority
        .admit_read(&g, &fresh, ProjectReadModule::Memory)
        .unwrap();
    assert_eq!(
        NativeMemoryStore::reobserve_mutation(
            fresh.snapshot(),
            &w,
            m.id,
            event.operation.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        Some(event.clone())
    );
    let h = NativeMemoryStore::history(fresh.snapshot(), &r, m.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(h.current, event.after);
    assert_eq!(h.generation.sdk_version, version);
    assert_eq!(h.revisions, vec![event]);
}
#[test]
fn revision_patch_no_change_clock_and_overflow_are_explicit() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let actor = ulid::Ulid::from(10u128).to_string();
    let id = MemoryId::new(NonZeroU128::new(1).unwrap());
    let project = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
    let mut before = Memory {
        id,
        revision: 1,
        title: "Title".into(),
        body: "Body".into(),
        attributes: MemoryAttributes {
            classification: MemoryClass::Fact,
            mandatory: true,
            important: true,
        },
        tags: vec![],
        references: vec![],
        reference_generation: 1,
        created_by: actor.clone(),
        updated_by: actor.clone(),
        created_at: 10,
        updated_at: 10,
    };
    let mut no_change = mutation("equal", 1);
    if let MemoryMutation::Update(patch) = &mut no_change {
        patch.body = Some(before.body.clone());
    }
    assert!(matches!(
        crate::memory_store::revision::derive(
            ctx.snapshot(),
            project,
            &before,
            &no_change,
            &actor,
            10
        ),
        Err(MemoryStoreError::NoChange)
    ));
    let op = mutation("revision", 1);
    assert!(matches!(
        crate::memory_store::revision::derive(ctx.snapshot(), project, &before, &op, &actor, 9),
        Err(MemoryStoreError::Invalid("backward Memory clock"))
    ));
    before.revision = u64::MAX;
    assert!(matches!(
        crate::memory_store::revision::derive(ctx.snapshot(), project, &before, &op, &actor, 10),
        Err(MemoryStoreError::Invalid("Memory revision overflow"))
    ));
    before.reference_generation = u64::MAX;
    let delete = MemoryMutation::Delete(DeleteMemory {
        expected_revision: u64::MAX,
        idempotency_key: "delete".into(),
        reason: "Delete".into(),
    });
    assert!(matches!(
        crate::memory_store::revision::derive(
            ctx.snapshot(),
            project,
            &before,
            &delete,
            &actor,
            10
        ),
        Err(MemoryStoreError::Invalid("reference generation overflow"))
    ));
}
