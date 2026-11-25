//! Real Native publication, logical target adapter and live read boundaries.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]
#[path = "../../graphit-data/tests/support/knowledge_reference_fixture.rs"]
mod support;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::file_lock::Cancellation;
use graphit_data::{
    knowledge_document_projection as h,
    knowledge_index::{KnowledgeIndexError, publish_native_authored_with_references},
    knowledge_reference_query::*,
    knowledge_references::*,
};
use graphit_knowledge::native_reference_query::*;
use graphit_records::reference_projection::{LogicalScope, ReferenceTargetKind};
use std::{error::Error, io::ErrorKind};
use support::*;

#[tokio::test]
async fn logical_targets_keep_categories_namespaces_and_live_original() -> Result<(), Box<dyn Error>>
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
    let categories = [
        (
            ReferenceTargetKind::Task,
            KnowledgeReferenceTargetKind::Task,
        ),
        (
            ReferenceTargetKind::Session,
            KnowledgeReferenceTargetKind::Session,
        ),
        (
            ReferenceTargetKind::Memory,
            KnowledgeReferenceTargetKind::Memory,
        ),
        (
            ReferenceTargetKind::Knowledge,
            KnowledgeReferenceTargetKind::Knowledge,
        ),
    ];
    let authored = categories
        .iter()
        .map(|(_, kind)| KnowledgeAuthoredReference {
            target_kind: *kind,
            scope: KnowledgeReferenceScope::User("owner β"),
            id: "literal same ID",
            context: if *kind == KnowledgeReferenceTargetKind::Knowledge {
                Some("source context β")
            } else {
                None
            },
            relation: "literal relation",
            field: Some(""),
        })
        .collect::<Vec<_>>();
    let docs = [page("a", &[])];
    let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits())?;
    let fields = [KnowledgeReferenceFields {
        slug: "a",
        state: KnowledgeDocumentState::Active,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW808".parse()?),
            context: None,
        },
        authored: &authored,
    }];
    let refs = prepare_knowledge_references(ctx.snapshot(), &rows, &fields, read_limits().content)?;
    publish_native_authored_with_references(ctx.snapshot(), &write, &rows, &refs, write_limits())
        .await?;
    let limits = read_limits();
    for (kind, data_kind) in categories {
        let query = KnowledgeReferencesQuery {
            outgoing: None,
            incoming: Some(KnowledgeLogicalTarget {
                target_kind: kind,
                scope: LogicalScope::User("owner β"),
                id: "literal same ID",
                context: if kind == ReferenceTargetKind::Knowledge {
                    Some("source context β")
                } else {
                    None
                },
            }),
            page_size: n(100),
            outgoing_cursor: None,
            incoming_cursor: None,
        };
        let result = read_native_knowledge_references(ctx.snapshot(), &read, query, limits).await?;
        let KnowledgeReferencesRead::Published { read: result, .. } = result else {
            panic!("real publication expected")
        };
        let edges = result.incoming.unwrap();
        assert_eq!(edges.total, 1);
        assert!(edges.complete);
        assert!(
            matches!(&edges.results[0].payload,KnowledgeReferencePayload::Authored{target,relation,field} if target.target_kind==data_kind && target.namespace.scope==KnowledgeReferenceOwnedScope::User("owner β".to_owned()) && target.id=="literal same ID" && relation=="literal relation" && field.as_deref()==Some(""))
        );
        assert_eq!(
            edges.results[0].target_observation,
            KnowledgeReferenceObservation::Unresolved
        );
        let foreign_cancel = Cancellation::default();
        let foreign = request(&foreign_cancel);
        assert!(matches!(
            read_native_knowledge_references(foreign.snapshot(), &read, query, limits).await,
            Err(KnowledgeReferenceQueryError::Data(
                KnowledgeIndexError::DifferentRequest
            ))
        ));
    }
    let query = KnowledgeReferencesQuery {
        outgoing: Some("a"),
        incoming: None,
        page_size: n(100),
        outgoing_cursor: None,
        incoming_cursor: None,
    };
    let memory_issuer = f.issuer_permissions(
        &ctx,
        &[graphit_auth::Permission::Read(
            graphit_core::module_settings::Module::Memory,
        )],
    );
    let memory_caller = memory_issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let memory_auth = ProjectRecordAuthority::new(&memory_issuer);
    let memory_read = memory_auth.admit_authenticated_read(
        &memory_caller,
        &ctx,
        f.scope(),
        ProjectReadModule::Memory,
    )?;
    assert!(matches!(
        read_native_knowledge_references(ctx.snapshot(), &memory_read, query, limits).await,
        Err(KnowledgeReferenceQueryError::Data(
            KnowledgeIndexError::WrongAdmission
        ))
    ));
    let deadline_cancel = Cancellation::default();
    let short = request_deadline(
        &deadline_cancel,
        std::time::Instant::now() + std::time::Duration::from_secs(1),
    );
    let short_caller = issuer.authenticate_runtime(&f.secret(), &short)?;
    let short_read = auth.admit_authenticated_read(
        &short_caller,
        &short,
        f.scope(),
        ProjectReadModule::Knowledge,
    )?;
    tokio::time::sleep_until(tokio::time::Instant::from_std(
        short.deadline().expect("finite fixture deadline"),
    ))
    .await;
    assert!(
        matches!(read_native_knowledge_references(short.snapshot(), &short_read, query, limits).await,
        Err(KnowledgeReferenceQueryError::Data(KnowledgeIndexError::Request(cause))) if cause.kind()==ErrorKind::TimedOut)
    );
    let (result, ()) = tokio::join!(
        read_native_knowledge_references(ctx.snapshot(), &read, query, limits),
        async {
            tokio::task::yield_now().await;
            cancel.cancel();
        }
    );
    assert!(
        matches!(result,Err(KnowledgeReferenceQueryError::Data(KnowledgeIndexError::Request(cause))) if cause.kind()==ErrorKind::Interrupted)
    );
    let error = read_native_knowledge_references(ctx.snapshot(), &read, query, limits)
        .await
        .unwrap_err();
    assert!(
        matches!(&error,KnowledgeReferenceQueryError::Data(KnowledgeIndexError::Request(cause)) if cause.kind()==ErrorKind::Interrupted)
    );
    assert!(error.source().is_some());
    Ok(())
}
