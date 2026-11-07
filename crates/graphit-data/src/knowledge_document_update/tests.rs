use super::*;
#[path = "fixture.rs"]
mod fixture;
use crate::knowledge_document_projection as h;
use crate::lance_branch_store::{BranchSnapshot, DeltaResolution};
use fixture::*;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::file_lock::Cancellation;
use std::{
    error::Error,
    time::{Duration, Instant},
};

fn limits() -> KnowledgeDocumentUpdateLimits {
    KnowledgeDocumentUpdateLimits {
        read: read_limits(),
        write: write_limits(),
    }
}
fn changes() -> [KnowledgeDocumentReferenceUpdate<'static>; 1] {
    [KnowledgeDocumentReferenceUpdate {
        slug: "a",
        authored: KnowledgeDocumentAuthoredUpdate::Preserve,
    }]
}

#[tokio::test]
async fn interruptions_at_real_phases_preserve_confirmed_commit_evidence()
-> Result<(), Box<dyn Error>> {
    for phase in [
        UpdatePhase::BeforeWriter,
        UpdatePhase::WriterAcquired,
        UpdatePhase::BatchValidated,
        UpdatePhase::ImageValidated,
        UpdatePhase::Prepared,
        UpdatePhase::BeforeCommit,
        UpdatePhase::Committed,
        UpdatePhase::Delivery,
    ] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
        let admin = issuer.bootstrap_admin(&caller)?;
        let parent = issuer.admin_parent(&caller, &admin)?;
        let (bearer, reference) = f.issue(&issuer, &parent);
        let grant = issuer.verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)?;
        let auth = ProjectRecordAuthority::new(&issuer);
        let read = auth.admit_read(&grant, &ctx, ProjectReadModule::Knowledge)?;
        let write = auth.admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)?;
        let initial = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
        let first =
            index::publish_native_authored(ctx.snapshot(), &write, &initial, write_limits())
                .await?
                .default;
        let pin = BranchSnapshot::capture_observed(&first.corpus_uri, None, None).await?;
        let mut changed = page("a", &[]);
        changed.body = Some("replacement");
        let rows = h::prepare(ctx.snapshot(), &[changed], &[], hlimits())?;
        let observe = |reached| {
            if phase == reached {
                cancel.cancel();
            }
        };
        let error = update_documents_inner(
            UpdateObservation {
                original: ctx.snapshot(),
                phase: &observe,
            },
            &read,
            &write,
            &first,
            &rows,
            &changes(),
            limits(),
            |_, old, new| Ok(old == new),
        )
        .await
        .unwrap_err();
        let fresh = BranchSnapshot::capture_observed(&first.corpus_uri, None, None).await?;
        if matches!(phase, UpdatePhase::Committed | UpdatePhase::Delivery) {
            let KnowledgeDocumentUpdateError::Published {
                generation,
                recovery_key,
                cause,
            } = error
            else {
                panic!("confirmed commit lost")
            };
            assert_eq!(generation.version, fresh.version());
            assert!(generation.version > first.version);
            assert_eq!(recovery_key.base_version(), first.version);
            assert_eq!(recovery_key.corpus_uri(), first.corpus_uri);
            assert!(
                matches!(*cause,KnowledgeDocumentUpdateError::Data(KnowledgeIndexError::Request(ref e)) if e.kind()==io::ErrorKind::Interrupted)
            );
            assert!(
                matches!(pin.resolve(&recovery_key).await?,DeltaResolution::Applied{version} if version==generation.version)
            );
        } else {
            assert!(
                matches!(error,KnowledgeDocumentUpdateError::Data(KnowledgeIndexError::Request(ref e)) if e.kind()==io::ErrorKind::Interrupted)
            );
            assert_eq!(fresh.version(), first.version);
        }
    }
    Ok(())
}

#[tokio::test]
async fn actual_sdk_concurrent_commit_retains_cas_key_and_sdk_cause() -> Result<(), Box<dyn Error>>
{
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let admin = issuer.bootstrap_admin(&caller)?;
    let parent = issuer.admin_parent(&caller, &admin)?;
    let (bearer, reference) = f.issue(&issuer, &parent);
    let grant = issuer.verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let read = auth.admit_read(&grant, &ctx, ProjectReadModule::Knowledge)?;
    let write = auth.admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)?;
    let initial = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
    let first = index::publish_native_authored(ctx.snapshot(), &write, &initial, write_limits())
        .await?
        .default;
    let pin = BranchSnapshot::capture_observed(&first.corpus_uri, None, None).await?;
    let previous = index::encode_with_references(
        &initial,
        None,
        ctx.snapshot(),
        write_limits().content,
        || Ok(()),
    )?;
    let external = pin.clone();
    let (go, started) = std::sync::mpsc::sync_channel(1);
    let (done, finished) = std::sync::mpsc::sync_channel(1);
    let interloper = std::thread::spawn(move || {
        started.recv().unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let committed = runtime.block_on(async {
            external
                .prepare_overwrite(vec![previous])
                .await
                .unwrap()
                .commit()
                .await
                .unwrap()
        });
        done.send(committed).unwrap();
    });
    let mut changed = page("a", &[]);
    changed.body = Some("my replacement");
    let rows = h::prepare(ctx.snapshot(), &[changed], &[], hlimits())?;
    let external_result = std::sync::Mutex::new(None);
    let finished = std::sync::Mutex::new(finished);
    let observe = |phase| {
        if phase == UpdatePhase::BeforeCommit {
            go.send(()).unwrap();
            *external_result.lock().unwrap() = Some(finished.lock().unwrap().recv().unwrap());
        }
    };
    let error = update_documents_inner(
        UpdateObservation {
            original: ctx.snapshot(),
            phase: &observe,
        },
        &read,
        &write,
        &first,
        &rows,
        &changes(),
        limits(),
        |_, old, new| Ok(old == new),
    )
    .await
    .unwrap_err();
    interloper.join().unwrap();
    let KnowledgeDocumentUpdateError::Commit(failure) = error else {
        panic!("SDK conflict must remain typed")
    };
    assert!(failure.source().is_some());
    let key = failure.recovery_key();
    assert_eq!(key.corpus_uri(), first.corpus_uri);
    assert_eq!(key.base_version(), first.version);
    assert!(!key.transaction_id().is_empty());
    let external = external_result.into_inner().unwrap().unwrap();
    assert_eq!(external.version(), first.version + 1);
    assert!(!matches!(
        pin.resolve(key).await?,
        DeltaResolution::Applied { .. }
    ));
    assert_eq!(
        BranchSnapshot::capture_observed(&first.corpus_uri, None, None)
            .await?
            .version(),
        external.version()
    );
    Ok(())
}

#[tokio::test]
async fn real_deadline_after_prepare_stops_before_commit_without_renewing_original()
-> Result<(), Box<dyn Error>> {
    // Bootstrap is a separate operation; the measured update receives its own
    // finite Original and actual admissions before any update work begins.
    let setup_cancel = Cancellation::default();
    let setup = request(&setup_cancel);
    let f = Fixture::new(&setup);
    let setup_issuer = f.issuer(&setup);
    let setup_caller = setup_issuer.authenticate_runtime(&f.secret(), &setup)?;
    let setup_auth = ProjectRecordAuthority::new(&setup_issuer);
    let setup_write = setup_auth.admit_authenticated_write(
        &setup_caller,
        &setup,
        f.scope(),
        ProjectWriteOperation::Knowledge,
    )?;
    let initial = h::prepare(setup.snapshot(), &[page("a", &[])], &[], hlimits())?;
    let first =
        index::publish_native_authored(setup.snapshot(), &setup_write, &initial, write_limits())
            .await?
            .default;

    let cancel = Cancellation::default();
    let now = Instant::now();
    let deadline = now + Duration::from_secs(5);
    let clock = std::sync::Arc::new(std::sync::Mutex::new(now));
    let ctx = request_deadline(&cancel, deadline);
    ctx.snapshot()
        .set_test_clock(std::sync::Arc::clone(&clock))
        .unwrap();
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let admin = issuer.bootstrap_admin(&caller)?;
    let parent = issuer.admin_parent(&caller, &admin)?;
    let (bearer, reference) = f.issue(&issuer, &parent);
    let grant = issuer.verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let read = auth.admit_read(&grant, &ctx, ProjectReadModule::Knowledge)?;
    let write = auth.admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)?;
    let mut changed = page("a", &[]);
    changed.body = Some("replacement");
    let rows = h::prepare(ctx.snapshot(), &[changed], &[], hlimits())?;
    assert_eq!(ctx.snapshot().deadline(), Some(deadline));
    ctx.check()?;
    let prepared_reached = std::sync::atomic::AtomicBool::new(false);
    let observe = |phase| {
        if phase == UpdatePhase::Prepared {
            prepared_reached.store(true, std::sync::atomic::Ordering::SeqCst);
            // Expire this same Original only after the real SDK preparation.
            *clock.lock().unwrap() = deadline;
        }
    };
    let error = update_documents_inner(
        UpdateObservation {
            original: ctx.snapshot(),
            phase: &observe,
        },
        &read,
        &write,
        &first,
        &rows,
        &changes(),
        limits(),
        |_, old, new| Ok(old == new),
    )
    .await
    .unwrap_err();
    assert!(
        prepared_reached.load(std::sync::atomic::Ordering::SeqCst),
        "deadline must occur after real prepare"
    );
    assert!(
        matches!(error,KnowledgeDocumentUpdateError::Data(KnowledgeIndexError::Request(ref e)) if e.kind()==io::ErrorKind::TimedOut)
    );
    assert_eq!(
        BranchSnapshot::capture_observed(&first.corpus_uri, None, None)
            .await?
            .version(),
        first.version
    );
    Ok(())
}
