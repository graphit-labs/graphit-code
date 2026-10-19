//! Actual captured FILE compilation and first-publication authored metadata.
#![cfg(target_os = "linux")]
#[path = "../../graphit-data/tests/support/knowledge_reference_fixture.rs"]
mod support;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::{
    artifact_store::{
        CapturedSourceReader, CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet,
        SnapshotPolicy, SourceReadLimits,
    },
    file_lock::Cancellation,
};
use graphit_data::{
    knowledge_document_projection as h, knowledge_index::*, knowledge_reference_query::*,
    knowledge_references::*,
};
use graphit_fs::NodeKind;
use graphit_hub::policy::RequestContext;
use graphit_knowledge::{
    authored_first_publication::*,
    compilation_fingerprint::CompilationFingerprintLimits,
    document::DocumentParseLimits,
    native_document_mapping::{AuthoredDocumentInput, NativeAuthoredMappingLimits},
    native_index_publication::{NativeDocumentPublicationLimits, publish_native_documents},
    publication_metadata::{
        CompiledDocument, PublicationMetadataLimits, PublicationMetadataPolicy,
        compile_captured_document,
    },
    references::ReferenceLimits,
};
use std::{
    error::Error,
    path::{Path, PathBuf},
};
use support::*;

fn parser() -> DocumentParseLimits {
    let cap = n(65536);
    DocumentParseLimits {
        max_document_bytes: cap,
        max_input_bytes: cap,
        max_frontmatter_bytes: cap,
        max_yaml_events: n(1024),
        max_markdown_events: n(1024),
        max_yaml_depth: n(16),
        max_scalar_bytes: n(4096),
        max_links: n(100),
        max_projection_bytes: cap,
        max_summary_bytes: n(16),
    }
}
fn metadata() -> PublicationMetadataLimits {
    PublicationMetadataLimits {
        max_input_bytes: n(65536),
        max_title_bytes: n(1024),
        max_kind_bytes: n(1024),
        max_description_bytes: n(1024),
        max_effective_summary_bytes: n(1024),
        max_effective_metadata_bytes: n(65536),
        max_owned_metadata_bytes: n(65536),
        max_root_entries: n(100),
        max_typed_nodes: n(1024),
        max_typed_depth: n(16),
        max_typed_scalar_bytes: n(4096),
        max_typed_events: n(2048),
        max_typed_lookahead_characters: n(1024),
        max_typed_flow_depth: n(16),
    }
}
fn limits() -> NativeDocumentPublicationLimits {
    NativeDocumentPublicationLimits {
        parse: parser(),
        mapping: NativeAuthoredMappingLimits {
            max_field_bytes: n(65536),
            max_uri_bytes: n(4096),
            max_target_bytes: n(4096),
        },
        fingerprint: CompilationFingerprintLimits {
            max_preimage_bytes: n(131072),
        },
        references: ReferenceLimits {
            max_identifier_bytes: n(256),
        },
        projection: hlimits(),
        index: write_limits(),
        authored_references:
            graphit_knowledge::document_authored_references::DocumentAuthoredReferenceLimits {
                max_results: n(32),
                max_identifier_bytes: n(256),
            },
    }
}
fn capture(
    f: &Fixture,
    ctx: &RequestContext<'_>,
    name: &str,
    source: &str,
) -> CapturedSourceReader {
    std::fs::write(f.project.owner_root().join(name), source).unwrap();
    let bytes = source.len().max(1);
    let retention = RetainedBytesLimits::new(bytes, 1, 1).unwrap();
    let copy = CopyLimits::new(bytes as u64, 1, 1, 1).unwrap();
    let policy = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        copy,
        RetainedFileSet::new([PathBuf::new()], retention).unwrap(),
        retention,
    )
    .unwrap();
    CapturedSourceReader::capture_project(
        &f.project,
        Path::new(name),
        NodeKind::File,
        &policy,
        SourceReadLimits::new(bytes, 4096, bytes as u64).unwrap(),
        ctx.snapshot(),
    )
    .unwrap()
}
fn compile<'a>(
    reader: &'a CapturedSourceReader,
    ctx: &RequestContext<'_>,
    fallback: &str,
) -> CompiledDocument<'a> {
    compile_captured_document(
        reader,
        Path::new(""),
        ctx.snapshot(),
        PublicationMetadataPolicy {
            fallback_title: fallback.into(),
            kind_on_absent: "guide".into(),
        },
        parser(),
        metadata(),
    )
    .unwrap()
}
fn input<'d, 's>(
    compiled: &'d CompiledDocument<'s>,
    slug: &'d str,
) -> AuthoredDocumentInput<'d, 's> {
    AuthoredDocumentInput {
        compiled,
        slug,
        source_path: slug,
        updated_at: None,
        stale_since: None,
        stale_after: None,
    }
}
async fn read_docs(
    original: &std::sync::Arc<graphit_core::file_io::SnapshotContext>,
    read: &graphit_auth::ProjectReadAdmission<'_>,
) -> (KnowledgeGeneration, Vec<KnowledgeDocument>) {
    match read_native_knowledge_index(original, read, read_limits())
        .await
        .unwrap()
    {
        KnowledgeIndexRead::Published {
            generation,
            documents,
        } => (generation, documents),
        _ => panic!(),
    }
}

#[tokio::test]
async fn actual_compiled_mixed_metadata_decoded_literals_and_full_source_publish_together()
-> Result<(), Box<dyn Error>> {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let admin = issuer.bootstrap_admin(&caller)?;
    let parent = issuer.admin_parent(&caller, &admin)?;
    let (bearer, reference) = f.issue(&issuer, &parent);
    let grant = issuer.verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)?;
    let write = auth.admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)?;
    let ar = capture(&f, &ctx, "a.md", "---\ntitle: A\n---\nWhole source [[c]]\n");
    let br = capture(&f, &ctx, "b.md", "---\nreferences: []\n---\nKnown empty\n");
    let source = "---\ntitle: Café\nreferences:\n- target: {type: task, id: ' tarefa '}\n  relation: ' supports '\n  field: ''\n- target: {type: session, id: ses-x}\n  relation: supports\n- target: {type: memory, id: \"line\\nbreak\", scope: user, scope_id: 'owner β'}\n  relation: literal\n- target: {type: knowledge, id: 'Café', context: 'literal alias'}\n  relation: supports\n- target: {type: knowledge, id: 'Café', context: 'literal alias'}\n  relation: supports\n---\nWhole source [[c]] [[b]] [[c]]\n";
    let cr = capture(&f, &ctx, "c.md", source);
    let a = compile(&ar, &ctx, "fallback");
    let b = compile(&br, &ctx, "fallback");
    let c = compile(&cr, &ctx, "fallback");
    let first = publish_native_documents_with_authored_references(
        ctx.snapshot(),
        &write,
        &[input(&a, "a"), input(&b, "b"), input(&c, "c")],
        limits(),
    )
    .await?;
    let read =
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
    let (generation, docs) = read_docs(ctx.snapshot(), &read).await;
    assert_eq!(generation, first.default);
    assert_eq!(docs.len(), 3);
    let doc = docs.iter().find(|d| d.slug == "c").unwrap();
    assert_eq!(doc.body, source);
    assert_eq!(doc.title, "Café");
    assert_eq!(
        doc.physical_digest.unwrap().as_bytes(),
        c.physical_source_digest().as_bytes()
    );
    let KnowledgeReferencesRead::Published { read: result, .. } =
        read_native_knowledge_reference_query(
            ctx.snapshot(),
            &read,
            KnowledgeReferenceQuery {
                outgoing: Some("c"),
                incoming: None,
                page_size: n(20),
                outgoing_cursor: None,
                incoming_cursor: None,
            },
            read_limits(),
        )
        .await?
    else {
        panic!()
    };
    assert!(!result.coverage.knowledge_source_complete);
    assert_eq!(result.warnings.len(), 1);
    let out = result.outgoing.unwrap();
    assert_eq!(out.total, 8);
    assert_eq!(out.results[3].payload, out.results[4].payload);
    assert!(
        matches!(&out.results[0].payload,KnowledgeReferencePayload::Authored{target,relation,field} if target.id==" tarefa " && relation==" supports " && field.as_deref()==Some(""))
    );
    assert!(
        matches!(&out.results[2].payload,KnowledgeReferencePayload::Authored{target,..} if target.id=="line\nbreak" && target.namespace.scope==KnowledgeReferenceOwnedScope::User("owner β".into()))
    );
    let KnowledgeReferencesRead::Published { read: empty, .. } =
        read_native_knowledge_reference_query(
            ctx.snapshot(),
            &read,
            KnowledgeReferenceQuery {
                outgoing: Some("b"),
                incoming: None,
                page_size: n(20),
                outgoing_cursor: None,
                incoming_cursor: None,
            },
            read_limits(),
        )
        .await?
    else {
        panic!()
    };
    assert_eq!(empty.outgoing.unwrap().total, 0);
    assert!(
        publish_native_documents_with_authored_references(
            ctx.snapshot(),
            &write,
            &[input(&c, "c")],
            limits()
        )
        .await
        .is_err()
    );
    assert_eq!(read_docs(ctx.snapshot(), &read).await.0, generation);
    Ok(())
}
#[tokio::test]
async fn invalid_metadata_caps_duplicate_and_foreign_compilation_fail_before_birth()
-> Result<(), Box<dyn Error>> {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    for metadata in [
        "references: null",
        "references: wrong",
        "references: [42]",
        "references: [{target: {type: task, id: x}, relation: 42}]",
        "references: [{target: {type: task, id: x, context: alias}, relation: supports}]",
    ] {
        let source = format!("---\n{metadata}\n---\nWhole source\n");
        let r = capture(&f, &ctx, "invalid.md", &source);
        let d = compile(&r, &ctx, "fallback");
        let error = publish_native_documents_with_authored_references(
            ctx.snapshot(),
            &write,
            &[input(&d, "a")],
            limits(),
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            NativeAuthoredFirstPublicationError::Authored(_)
        ));
        assert!(error.source().is_some());
        assert!(
            !f.project
                .runtime_root()
                .join("knowledge/index.lance")
                .exists()
        );
    }
    let r = capture(
        &f,
        &ctx,
        "valid.md",
        "---\nreferences: [{target: {type: task, id: x}, relation: supports}, {target: {type: task, id: y}, relation: supports}]\n---\nBody\n",
    );
    let d = compile(&r, &ctx, "fallback");
    let mut capped = limits();
    capped.authored_references.max_results = n(1);
    assert!(
        publish_native_documents_with_authored_references(
            ctx.snapshot(),
            &write,
            &[input(&d, "a")],
            capped
        )
        .await
        .is_err()
    );
    capped = limits();
    capped.parse.max_frontmatter_bytes = n(1);
    assert!(
        publish_native_documents_with_authored_references(
            ctx.snapshot(),
            &write,
            &[input(&d, "a")],
            capped
        )
        .await
        .is_err()
    );
    assert!(
        publish_native_documents_with_authored_references(
            ctx.snapshot(),
            &write,
            &[input(&d, "a"), input(&d, "a")],
            limits()
        )
        .await
        .is_err()
    );
    let other_cancel = Cancellation::default();
    let other = request(&other_cancel);
    let other_fixture = Fixture::new(&other);
    let foreign_r = capture(&other_fixture, &other, "foreign.md", "Foreign source");
    let foreign = compile(&foreign_r, &other, "fallback");
    assert!(
        publish_native_documents_with_authored_references(
            ctx.snapshot(),
            &write,
            &[input(&foreign, "a")],
            limits()
        )
        .await
        .is_err()
    );
    cancel.cancel();
    assert!(
        publish_native_documents_with_authored_references(
            ctx.snapshot(),
            &write,
            &[input(&d, "a")],
            limits()
        )
        .await
        .is_err()
    );
    assert!(
        !f.project
            .runtime_root()
            .join("knowledge/index.lance")
            .exists()
    );
    Ok(())
}
#[tokio::test]
async fn ordinary_first_publisher_remains_incomplete_despite_yaml_metadata()
-> Result<(), Box<dyn Error>> {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    let r = capture(
        &f,
        &ctx,
        "ordinary.md",
        "---\nreferences: []\n---\nOrdinary remains incomplete\n",
    );
    let d = compile(&r, &ctx, "fallback");
    publish_native_documents(ctx.snapshot(), &write, &[input(&d, "a")], limits()).await?;
    let read =
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
    let KnowledgeReferencesRead::Published { read: result, .. } =
        read_native_knowledge_reference_query(
            ctx.snapshot(),
            &read,
            KnowledgeReferenceQuery {
                outgoing: Some("a"),
                incoming: None,
                page_size: n(20),
                outgoing_cursor: None,
                incoming_cursor: None,
            },
            read_limits(),
        )
        .await?
    else {
        panic!()
    };
    assert!(!result.coverage.knowledge_source_complete);
    assert_eq!(result.warnings.len(), 1);
    let complete = Fixture::new(&ctx);
    let issuer = complete.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&complete.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let write = auth.admit_authenticated_write(
        &caller,
        &ctx,
        complete.scope(),
        ProjectWriteOperation::Knowledge,
    )?;
    let authored = page("a", &[]);
    let rows = h::prepare(ctx.snapshot(), &[authored], &[], hlimits())?;
    let fields = [KnowledgeReferenceFields {
        slug: "a",
        state: KnowledgeDocumentState::Active,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW808".parse()?),
            context: None,
        },
        authored: &[],
    }];
    let refs =
        prepare_knowledge_references(ctx.snapshot(), &rows, &fields, write_limits().content)?;
    publish_native_authored_with_references(ctx.snapshot(), &write, &rows, &refs, write_limits())
        .await?;
    let read = auth.admit_authenticated_read(
        &caller,
        &ctx,
        complete.scope(),
        ProjectReadModule::Knowledge,
    )?;
    let (_, documents) = read_docs(ctx.snapshot(), &read).await;
    assert_eq!(documents[0].body, authored.body.unwrap());
    let KnowledgeReferencesRead::Published { read: result, .. } =
        read_native_knowledge_reference_query(
            ctx.snapshot(),
            &read,
            KnowledgeReferenceQuery {
                outgoing: Some("a"),
                incoming: None,
                page_size: n(20),
                outgoing_cursor: None,
                incoming_cursor: None,
            },
            read_limits(),
        )
        .await?
    else {
        panic!()
    };
    assert!(result.coverage.knowledge_source_complete);
    assert!(result.warnings.is_empty());
    assert_eq!(result.outgoing.unwrap().total, 0);
    Ok(())
}
