use super::*;
#[path = "fixture.rs"]
mod fixture;
use crate::{
    knowledge_document_projection as h,
    lance_branch_store::{BranchError, BranchSnapshot},
};
use fixture::*;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::{
    file_lock::Cancellation,
    project_paths::{InstallationPaths, ProjectPaths},
    version_context::{Checkout, VersionContext},
};
use std::{
    error::Error,
    io,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
fn limits() -> KnowledgeDocumentUpdateLimits {
    KnowledgeDocumentUpdateLimits {
        read: read_limits(),
        write: write_limits(),
    }
}
fn branch(f: &Fixture) -> VersionContext {
    let install = InstallationPaths::resolve(
        f.project.brand().clone(),
        f.project.owner_root(),
        None,
        Some(f.realm().live.owner_root()),
    )
    .unwrap();
    VersionContext::select(
        &ProjectPaths::at(f.project.owner_root(), &install).unwrap(),
        Checkout::NonGit,
        "topic",
        None,
        "",
    )
    .unwrap()
}
#[tokio::test]
async fn actual_phases_cancel_prevent_creation_or_preserve_confirmed_materialization()
-> Result<(), Box<dyn Error>> {
    for phase in [
        MaterializationPhase::BeforeWriter,
        MaterializationPhase::WriterAcquired,
        MaterializationPhase::Captured,
        MaterializationPhase::BatchValidated,
        MaterializationPhase::ImageValidated,
        MaterializationPhase::BeforeMaterialize,
        MaterializationPhase::Materialized,
        MaterializationPhase::Delivery,
    ] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
        let auth = ProjectRecordAuthority::new(&issuer);
        let write = auth.admit_authenticated_write(
            &caller,
            &ctx,
            f.scope(),
            ProjectWriteOperation::Knowledge,
        )?;
        let rows = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
        let first = index::publish_native_authored(ctx.snapshot(), &write, &rows, write_limits())
            .await?
            .default;
        let selected = branch(&f);
        let name = selected.lance_branch();
        let scope = f.scope_for(&selected);
        let read =
            auth.admit_authenticated_read(&caller, &ctx, scope, ProjectReadModule::Knowledge)?;
        let write =
            auth.admit_authenticated_write(&caller, &ctx, scope, ProjectWriteOperation::Knowledge)?;
        let expected = index::generation(
            &first.corpus_uri,
            &BranchSnapshot::capture_observed(&first.corpus_uri, Some(&name), None).await?,
        );
        let reached = AtomicBool::new(false);
        let observe = |actual| {
            if actual == phase {
                reached.store(true, Ordering::SeqCst);
                cancel.cancel();
            }
        };
        let error = materialize_inner(ctx.snapshot(), &read, &write, &expected, limits(), &observe)
            .await
            .unwrap_err();
        assert!(reached.load(Ordering::SeqCst));
        assert!(error.source().is_some());
        let fresh = BranchSnapshot::capture_observed(&first.corpus_uri, Some(&name), None).await?;
        if matches!(
            phase,
            MaterializationPhase::Materialized | MaterializationPhase::Delivery
        ) {
            let KnowledgeBranchMaterializationError::Materialized { generation, cause } = error
            else {
                panic!("confirmed materialization lost")
            };
            assert_eq!(generation, index::generation(&first.corpus_uri, &fresh));
            assert!(fresh.inherited_target().is_none());
            assert!(
                matches!(*cause,KnowledgeBranchMaterializationError::Data(KnowledgeIndexError::Request(ref e)) if e.kind()==io::ErrorKind::Interrupted)
            );
        } else {
            assert!(fresh.inherited_target().is_some());
            assert!(
                matches!(error,KnowledgeBranchMaterializationError::Data(KnowledgeIndexError::Request(ref e)) if e.kind()==io::ErrorKind::Interrupted)
            );
        }
        assert_eq!(
            BranchSnapshot::capture_observed(&first.corpus_uri, None, None)
                .await?
                .version(),
            first.version
        );
    }
    Ok(())
}
#[tokio::test]
async fn actual_sdk_branch_conflict_and_parent_advance_retain_captured_version_semantics()
-> Result<(), Box<dyn Error>> {
    for interference in ["branch", "parent"] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
        let auth = ProjectRecordAuthority::new(&issuer);
        let write = auth.admit_authenticated_write(
            &caller,
            &ctx,
            f.scope(),
            ProjectWriteOperation::Knowledge,
        )?;
        let rows = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
        let first = index::publish_native_authored(ctx.snapshot(), &write, &rows, write_limits())
            .await?
            .default;
        let selected = branch(&f);
        let name = selected.lance_branch();
        let scope = f.scope_for(&selected);
        let read =
            auth.admit_authenticated_read(&caller, &ctx, scope, ProjectReadModule::Knowledge)?;
        let write =
            auth.admit_authenticated_write(&caller, &ctx, scope, ProjectWriteOperation::Knowledge)?;
        let inherited =
            BranchSnapshot::capture_observed(&first.corpus_uri, Some(&name), None).await?;
        let expected = index::generation(&first.corpus_uri, &inherited);
        let external = if interference == "branch" {
            inherited
        } else {
            BranchSnapshot::capture_observed(&first.corpus_uri, None, None).await?
        };
        let batch = index::encode_with_references(
            &rows,
            None,
            ctx.snapshot(),
            write_limits().content,
            || Ok(()),
        )?;
        let (go, started) = std::sync::mpsc::sync_channel(1);
        let (done, finished) = std::sync::mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || {
            started.recv().unwrap();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let result = runtime.block_on(async {
                if interference == "branch" {
                    external.materialize().await.unwrap()
                } else {
                    external
                        .prepare_overwrite(vec![batch])
                        .await
                        .unwrap()
                        .commit()
                        .await
                        .unwrap()
                }
            });
            done.send(result.version()).unwrap();
        });
        let finished = std::sync::Mutex::new(finished);
        let external_version = std::sync::Mutex::new(None);
        let observe = |phase| {
            if phase == MaterializationPhase::BeforeMaterialize {
                go.send(()).unwrap();
                *external_version.lock().unwrap() = Some(finished.lock().unwrap().recv().unwrap());
            }
        };
        let result =
            materialize_inner(ctx.snapshot(), &read, &write, &expected, limits(), &observe).await;
        thread.join().unwrap();
        let external_version = external_version.into_inner().unwrap().unwrap();
        if interference == "branch" {
            let error = result.unwrap_err();
            assert!(error.source().is_some());
            assert!(matches!(
                error,
                KnowledgeBranchMaterializationError::Data(KnowledgeIndexError::Branch(
                    BranchError::CreationConflict { .. }
                ))
            ));
        } else {
            let KnowledgeBranchMaterialization::Materialized {
                generation,
                inherited,
            } = result?
            else {
                panic!()
            };
            assert_eq!(inherited, expected);
            assert_eq!(generation.version, first.version);
            assert!(external_version > first.version);
            assert_eq!(
                BranchSnapshot::capture_observed(&first.corpus_uri, None, None)
                    .await?
                    .version(),
                external_version
            );
            assert_eq!(
                BranchSnapshot::capture_observed(&first.corpus_uri, Some(&name), None)
                    .await?
                    .version(),
                first.version
            );
        }
    }
    Ok(())
}
#[tokio::test]
async fn confirmed_materialization_then_revoke_or_deadline_keeps_actual_effect()
-> Result<(), Box<dyn Error>> {
    for cause in ["revoke", "deadline"] {
        let cancel = Cancellation::default();
        let ctx = request_deadline(&cancel, Instant::now() + Duration::from_secs(10));
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
        let admin = issuer.bootstrap_admin(&caller)?;
        let parent = issuer.admin_parent(&caller, &admin)?;
        let (bearer, reference) = f.issue(&issuer, &parent);
        let grant = issuer.verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)?;
        let auth = ProjectRecordAuthority::new(&issuer);
        let write = auth.admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)?;
        let rows = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
        let first = index::publish_native_authored(ctx.snapshot(), &write, &rows, write_limits())
            .await?
            .default;
        let selected = branch(&f);
        let name = selected.lance_branch();
        let scope = f.scope_for(&selected);
        let run = issuer.start_run(&parent, &scope, Duration::from_secs(900))?;
        let named_bearer = issuer.issue(
            &parent,
            &run,
            &reference,
            &[
                graphit_auth::Permission::Read(graphit_core::module_settings::Module::Knowledge),
                graphit_auth::Permission::MutateModule(
                    graphit_core::module_settings::Module::Knowledge,
                ),
            ],
            &scope,
        )?;
        let named_grant =
            issuer.verify(named_bearer.expose_for_transport(), &reference, scope, &ctx)?;
        let read = auth.admit_read(&named_grant, &ctx, ProjectReadModule::Knowledge)?;
        let write = auth.admit_write(&named_grant, &ctx, ProjectWriteOperation::Knowledge)?;
        let expected = index::generation(
            &first.corpus_uri,
            &BranchSnapshot::capture_observed(&first.corpus_uri, Some(&name), None).await?,
        );
        let reached = AtomicBool::new(false);
        let observe = |phase| {
            if phase == MaterializationPhase::Materialized {
                reached.store(true, Ordering::SeqCst);
                if cause == "revoke" {
                    issuer.revoke(&parent, &named_bearer).unwrap();
                } else {
                    let deadline = ctx.snapshot().deadline().unwrap();
                    std::thread::sleep(
                        deadline.saturating_duration_since(Instant::now())
                            + Duration::from_millis(20),
                    );
                }
            }
        };
        let error = materialize_inner(ctx.snapshot(), &read, &write, &expected, limits(), &observe)
            .await
            .unwrap_err();
        assert!(
            reached.load(Ordering::SeqCst),
            "setup failure does not prove known phase deadline"
        );
        assert!(error.source().is_some());
        let KnowledgeBranchMaterializationError::Materialized {
            generation,
            cause: actual,
        } = error
        else {
            panic!("confirmed materialization lost")
        };
        if cause == "revoke" {
            assert!(matches!(
                *actual,
                KnowledgeBranchMaterializationError::Data(KnowledgeIndexError::Auth(
                    graphit_auth::AuthError::Denied
                ))
            ));
        } else {
            assert!(
                matches!(*actual,KnowledgeBranchMaterializationError::Data(KnowledgeIndexError::Request(ref e)) if e.kind()==io::ErrorKind::TimedOut)
            );
        }
        let fresh = BranchSnapshot::capture_observed(&first.corpus_uri, Some(&name), None).await?;
        assert_eq!(generation, index::generation(&first.corpus_uri, &fresh));
        assert!(fresh.inherited_target().is_none());
    }
    Ok(())
}

#[tokio::test]
async fn capture_replacement_after_sdk_confirmation_preserves_materialized_generation()
-> Result<(), Box<dyn Error>> {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    let rows = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
    let first = index::publish_native_authored(ctx.snapshot(), &write, &rows, write_limits())
        .await?
        .default;
    let selected = branch(&f);
    let name = selected.lance_branch();
    let scope = f.scope_for(&selected);
    let read = auth.admit_authenticated_read(&caller, &ctx, scope, ProjectReadModule::Knowledge)?;
    let write =
        auth.admit_authenticated_write(&caller, &ctx, scope, ProjectWriteOperation::Knowledge)?;
    let expected = index::generation(
        &first.corpus_uri,
        &BranchSnapshot::capture_observed(&first.corpus_uri, Some(&name), None).await?,
    );
    let path = f.project.runtime_root().join("knowledge/index.lance");
    let saved = f.project.runtime_root().join("saved-index.lance");
    let reached = AtomicBool::new(false);
    let observe = |phase| {
        if phase == MaterializationPhase::Materialized {
            reached.store(true, Ordering::SeqCst);
            std::fs::rename(&path, &saved).unwrap();
            std::fs::create_dir(&path).unwrap();
        }
    };
    let error = materialize_inner(ctx.snapshot(), &read, &write, &expected, limits(), &observe)
        .await
        .unwrap_err();
    assert!(reached.load(Ordering::SeqCst));
    assert!(error.source().is_some());
    let KnowledgeBranchMaterializationError::Materialized { generation, cause } = error else {
        panic!("confirmed materialization lost")
    };
    assert!(
        matches!(
            *cause,
            KnowledgeBranchMaterializationError::Data(KnowledgeIndexError::Request(_))
                | KnowledgeBranchMaterializationError::Data(KnowledgeIndexError::Artifact(_))
        ),
        "{cause:?}"
    );
    assert!(cause.source().is_some());
    assert_eq!(generation.version, first.version);
    assert_eq!(generation.physical_branch.as_deref(), Some(name.as_str()));
    let saved_uri = url::Url::from_file_path(&saved).unwrap().to_string();
    let actual = BranchSnapshot::capture_observed(&saved_uri, Some(&name), None).await?;
    assert!(actual.inherited_target().is_none());
    assert_eq!(actual.version(), generation.version);
    Ok(())
}
