use super::*;
#[path = "fixture.rs"]
mod fixture;
use crate::{knowledge_document_projection as h, lance_branch_store::BranchSnapshot};
use fixture::*;
use graphit_auth::{ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::file_lock::Cancellation;
use std::{
    error::Error,
    io,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[tokio::test]
async fn real_first_publication_phase_cancel_retains_confirmed_default()
-> Result<(), Box<dyn Error>> {
    for version in ["", "topic"] {
        for phase in [
            FirstPublicationPhase::RowsValidated,
            FirstPublicationPhase::ReferencesEncoded,
            FirstPublicationPhase::ImageValidated,
            FirstPublicationPhase::BeforeWriter,
            FirstPublicationPhase::WriterAcquired,
            FirstPublicationPhase::DefaultConfirmed,
            FirstPublicationPhase::Delivery,
        ] {
            let cancel = Cancellation::default();
            let ctx = request(&cancel);
            let f = Fixture::with_version(&ctx, version);
            let issuer = f.issuer(&ctx);
            let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
            let admin = issuer.bootstrap_admin(&caller)?;
            let parent = issuer.admin_parent(&caller, &admin)?;
            let (bearer, reference) = f.issue(&issuer, &parent);
            let grant =
                issuer.verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)?;
            let auth = ProjectRecordAuthority::new(&issuer);
            let write = auth.admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)?;
            let rows = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
            let updates = [KnowledgeDocumentReferenceUpdate {
                slug: "a",
                authored: KnowledgeDocumentAuthoredUpdate::Replace(&[]),
            }];
            let reached = AtomicBool::new(false);
            let observe = |actual| {
                if actual == phase {
                    reached.store(true, Ordering::SeqCst);
                    cancel.cancel();
                }
            };
            let error = publish_inner(
                ctx.snapshot(),
                &write,
                &rows,
                &updates,
                write_limits(),
                &observe,
            )
            .await
            .unwrap_err();
            assert!(reached.load(Ordering::SeqCst));
            if matches!(
                phase,
                FirstPublicationPhase::DefaultConfirmed | FirstPublicationPhase::Delivery
            ) {
                assert!(error.source().is_some());
                let KnowledgeIndexError::PartialDefault { generation, cause } = error else {
                    panic!("confirmed default lost")
                };
                assert!(
                    matches!(*cause, KnowledgeIndexError::Request(ref e) if e.kind()==io::ErrorKind::Interrupted)
                );
                let pin =
                    BranchSnapshot::capture_observed(&generation.corpus_uri, None, None).await?;
                assert_eq!(pin.version(), generation.version);
                assert_eq!(generation.physical_branch, None);
                if version == "topic" && phase == FirstPublicationPhase::Delivery {
                    let named = BranchSnapshot::capture_observed(
                        &generation.corpus_uri,
                        Some(f.scope().project.unwrap().version.lance_branch().as_str()),
                        None,
                    )
                    .await?;
                    assert!(named.inherited_target().is_none());
                }
            } else {
                assert!(
                    matches!(error, KnowledgeIndexError::Request(ref e) if e.kind()==io::ErrorKind::Interrupted)
                );
                assert!(
                    !f.project
                        .runtime_root()
                        .join("knowledge/index.lance")
                        .exists()
                );
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn deadline_after_confirmed_default_keeps_actual_effect_and_cause()
-> Result<(), Box<dyn Error>> {
    let cancel = Cancellation::default();
    let ctx = request_deadline(&cancel, Instant::now() + Duration::from_secs(3));
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    let rows = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
    let updates = [KnowledgeDocumentReferenceUpdate {
        slug: "a",
        authored: KnowledgeDocumentAuthoredUpdate::Preserve,
    }];
    let reached = AtomicBool::new(false);
    let observe = |phase| {
        if phase == FirstPublicationPhase::DefaultConfirmed {
            reached.store(true, Ordering::SeqCst);
            std::thread::sleep(Duration::from_secs(4));
        }
    };
    let error = publish_inner(
        ctx.snapshot(),
        &write,
        &rows,
        &updates,
        write_limits(),
        &observe,
    )
    .await
    .unwrap_err();
    assert!(
        reached.load(Ordering::SeqCst),
        "setup expiry cannot qualify post-confirmation deadline"
    );
    let KnowledgeIndexError::PartialDefault { generation, cause } = error else {
        panic!("confirmed default lost")
    };
    assert!(
        matches!(*cause, KnowledgeIndexError::Request(ref e) if e.kind()==io::ErrorKind::TimedOut)
    );
    assert_eq!(
        BranchSnapshot::capture_observed(&generation.corpus_uri, None, None)
            .await?
            .version(),
        generation.version
    );
    Ok(())
}

#[tokio::test]
async fn bearer_revoked_after_actual_default_keeps_generation_and_auth_cause()
-> Result<(), Box<dyn Error>> {
    use graphit_auth::AuthError;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_version(&ctx, "topic");
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let admin = issuer.bootstrap_admin(&caller)?;
    let parent = issuer.admin_parent(&caller, &admin)?;
    let (bearer, reference) = f.issue(&issuer, &parent);
    let grant = issuer.verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let write = auth.admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)?;
    let rows = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
    let updates = [KnowledgeDocumentReferenceUpdate {
        slug: "a",
        authored: KnowledgeDocumentAuthoredUpdate::Replace(&[]),
    }];
    let reached = AtomicBool::new(false);
    let observe = |phase| {
        if phase == FirstPublicationPhase::DefaultConfirmed {
            reached.store(true, Ordering::SeqCst);
            issuer.revoke(&parent, &bearer).unwrap();
        }
    };
    let error = publish_inner(
        ctx.snapshot(),
        &write,
        &rows,
        &updates,
        write_limits(),
        &observe,
    )
    .await
    .unwrap_err();
    assert!(reached.load(Ordering::SeqCst));
    assert!(error.source().is_some());
    let KnowledgeIndexError::PartialDefault { generation, cause } = error else {
        panic!("confirmed default lost")
    };
    assert!(matches!(
        *cause,
        KnowledgeIndexError::Auth(AuthError::Denied)
    ));
    assert!(cause.source().is_some());
    assert_eq!(
        BranchSnapshot::capture_observed(&generation.corpus_uri, None, None)
            .await?
            .version(),
        generation.version
    );
    Ok(())
}
