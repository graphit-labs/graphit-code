//! Actual captured FILE compilation, Knowledge mapping, authenticated Data CAS.
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
    knowledge_document_projection as h, knowledge_document_update::*, knowledge_index::*,
    knowledge_reference_query::*, knowledge_references::*,
};
use graphit_fs::NodeKind;
use graphit_hub::policy::RequestContext;
use graphit_knowledge::{
    compilation_fingerprint::CompilationFingerprintLimits,
    document::DocumentParseLimits,
    document_republication::*,
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
fn limits() -> NativeDocumentRepublicationLimits {
    NativeDocumentRepublicationLimits {
        read: read_limits(),
        publication: NativeDocumentPublicationLimits {
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
fn updated(result: KnowledgeDocumentUpdateResult) -> KnowledgeGeneration {
    match result {
        KnowledgeDocumentUpdateResult::Updated { generation, .. } => generation,
        _ => panic!("update expected"),
    }
}

#[tokio::test]
async fn compiled_subset_and_recipe_change_preserve_other_page_and_full_source()
-> Result<(), Box<dyn Error>> {
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
    let ar = capture(
        &f,
        &ctx,
        "a.md",
        "---\ntitle: A\n---\nWhole source A [[b]]\n",
    );
    let br = capture(
        &f,
        &ctx,
        "b.md",
        "---\ntitle: B\n---\nWhole source B [[a]]\n",
    );
    let a = compile(&ar, &ctx, "fallback");
    let b = compile(&br, &ctx, "fallback");
    let first = publish_native_documents(
        ctx.snapshot(),
        &write,
        &[input(&a, "a"), input(&b, "b")],
        limits().publication,
    )
    .await?
    .default;
    let (_, before) = read_docs(ctx.snapshot(), &read).await;
    let br2 = capture(
        &f,
        &ctx,
        "b-next.md",
        "---\ntitle: B changed\ndescription: ''\n---\nNew whole source B [[a]]\n",
    );
    let b2 = compile(&br2, &ctx, "fallback");
    let next = updated(
        republish_native_documents(
            ctx.snapshot(),
            &read,
            &write,
            &first,
            &[input(&b2, "b")],
            limits(),
        )
        .await?,
    );
    let (observed, after) = read_docs(ctx.snapshot(), &read).await;
    assert_eq!(observed, next);
    assert!(next.version > first.version);
    assert_eq!(
        before.iter().find(|d| d.slug == "a"),
        after.iter().find(|d| d.slug == "a")
    );
    let b_after = after.iter().find(|d| d.slug == "b").unwrap();
    assert_eq!(b_after.body, b2.whole_source());
    assert_eq!(b_after.description.as_deref(), Some(""));
    assert_ne!(
        before
            .iter()
            .find(|d| d.slug == "b")
            .unwrap()
            .compilation_digest,
        b_after.compilation_digest
    );
    assert!(matches!(
        republish_native_documents(
            ctx.snapshot(),
            &read,
            &write,
            &next,
            &[input(&b2, "b")],
            limits()
        )
        .await?,
        KnowledgeDocumentUpdateResult::Unchanged { .. }
    ));
    let a_recipe = compile(&ar, &ctx, "different recipe fallback");
    let next2 = updated(
        republish_native_documents(
            ctx.snapshot(),
            &read,
            &write,
            &next,
            &[input(&a_recipe, "a")],
            limits(),
        )
        .await?,
    );
    let (_, after2) = read_docs(ctx.snapshot(), &read).await;
    let a_before = after.iter().find(|d| d.slug == "a").unwrap();
    let a_after = after2.iter().find(|d| d.slug == "a").unwrap();
    assert_eq!(a_before.body, a_after.body);
    assert_eq!(a_before.physical_digest, a_after.physical_digest);
    assert_ne!(a_before.compilation_digest, a_after.compilation_digest);
    assert_eq!(
        a_after.recipe_parameters.fallback_title,
        "different recipe fallback"
    );
    assert!(next2.version > next.version);
    assert_eq!(
        after.iter().find(|d| d.slug == "b"),
        after2.iter().find(|d| d.slug == "b")
    );
    Ok(())
}

#[tokio::test]
async fn compiled_authored_replace_decoded_literals_preserve_and_clear()
-> Result<(), Box<dyn Error>> {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let read =
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    // Qualified complete first publisher seeds existing Some; update uses real compiled input.
    let seed = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
    let fields = [KnowledgeReferenceFields {
        slug: "a",
        state: KnowledgeDocumentState::Active,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW808".parse()?),
            context: None,
        },
        authored: &[],
    }];
    let refs = prepare_knowledge_references(ctx.snapshot(), &seed, &fields, read_limits().content)?;
    let first = publish_native_authored_with_references(
        ctx.snapshot(),
        &write,
        &seed,
        &refs,
        write_limits(),
    )
    .await?
    .default;
    let source = "---\ntitle: References\nreferences:\n- target: {type: task, id: ' tarefa '}\n  relation: ' supports '\n  field: ''\n- target: {type: session, id: ses-x}\n  relation: supports\n- target: {type: memory, id: \"line\\nbreak\", scope: user, scope_id: 'owner β'}\n  relation: literal\n- target: {type: knowledge, id: 'Café', context: 'literal alias'}\n  relation: supports\n- target: {type: knowledge, id: 'Café', context: 'literal alias'}\n  relation: supports\n---\nWhole source [[a]]\n";
    let captured = capture(&f, &ctx, "references.md", source);
    let compiled = compile(&captured, &ctx, "fallback");
    let mut current = updated(
        republish_native_documents(
            ctx.snapshot(),
            &read,
            &write,
            &first,
            &[input(&compiled, "a")],
            limits(),
        )
        .await?,
    );
    let q = KnowledgeReferenceQuery {
        outgoing: Some("a"),
        incoming: None,
        page_size: n(20),
        outgoing_cursor: None,
        incoming_cursor: None,
    };
    let qlimits = read_limits();
    let KnowledgeReferencesRead::Published { read: result, .. } =
        read_native_knowledge_reference_query(ctx.snapshot(), &read, q, qlimits).await?
    else {
        panic!()
    };
    let out = result.outgoing.unwrap();
    assert_eq!(out.total, 6);
    assert!(
        matches!(&out.results[2].payload,KnowledgeReferencePayload::Authored{target,..} if target.id=="line\nbreak")
    );
    assert_eq!(out.results[3].payload, out.results[4].payload);
    assert!(result.coverage.knowledge_source_complete);
    let preserve_reader = capture(
        &f,
        &ctx,
        "preserve.md",
        "---\ntitle: Changed\n---\nWhole source preserved refs [[a]]\n",
    );
    let preserve = compile(&preserve_reader, &ctx, "fallback");
    current = updated(
        republish_native_documents(
            ctx.snapshot(),
            &read,
            &write,
            &current,
            &[input(&preserve, "a")],
            limits(),
        )
        .await?,
    );
    let KnowledgeReferencesRead::Published {
        read: preserved, ..
    } = read_native_knowledge_reference_query(ctx.snapshot(), &read, q, qlimits).await?
    else {
        panic!()
    };
    assert_eq!(preserved.outgoing.unwrap().total, 6);
    let clear_reader = capture(
        &f,
        &ctx,
        "clear.md",
        "---\nreferences: []\n---\nWhole source no refs\n",
    );
    let clear = compile(&clear_reader, &ctx, "fallback");
    current = updated(
        republish_native_documents(
            ctx.snapshot(),
            &read,
            &write,
            &current,
            &[input(&clear, "a")],
            limits(),
        )
        .await?,
    );
    let KnowledgeReferencesRead::Published { read: cleared, .. } =
        read_native_knowledge_reference_query(ctx.snapshot(), &read, q, qlimits).await?
    else {
        panic!()
    };
    assert!(cleared.coverage.knowledge_source_complete);
    assert_eq!(cleared.outgoing.unwrap().total, 0);
    assert!(matches!(
        republish_native_documents(
            ctx.snapshot(),
            &read,
            &write,
            &current,
            &[input(&clear, "a")],
            limits()
        )
        .await?,
        KnowledgeDocumentUpdateResult::Unchanged { .. }
    ));
    Ok(())
}

#[tokio::test]
async fn invalid_metadata_bounds_foreign_compiled_original_and_cancel_prevent_commit()
-> Result<(), Box<dyn Error>> {
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
    let captured = capture(&f, &ctx, "a.md", "Whole source");
    let a = compile(&captured, &ctx, "fallback");
    let first = publish_native_documents(
        ctx.snapshot(),
        &write,
        &[input(&a, "a")],
        limits().publication,
    )
    .await?
    .default;
    for (i, yaml) in ["references: null", "references: {}", "references: [false]"]
        .iter()
        .enumerate()
    {
        let source = format!("---\n{yaml}\n---\nBad refs\n");
        let r = capture(&f, &ctx, &format!("bad-{i}.md"), &source);
        let doc = compile(&r, &ctx, "fallback");
        let error = republish_native_documents(
            ctx.snapshot(),
            &read,
            &write,
            &first,
            &[input(&doc, "a")],
            limits(),
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            NativeDocumentRepublicationError::Authored(_)
        ));
        assert!(error.source().is_some());
        assert_eq!(read_docs(ctx.snapshot(), &read).await.0, first);
    }
    let mut bounded = limits();
    bounded.publication.parse.max_document_bytes = n(a.whole_source().len() - 1);
    assert!(matches!(
        republish_native_documents(
            ctx.snapshot(),
            &read,
            &write,
            &first,
            &[input(&a, "a")],
            bounded
        )
        .await,
        Err(NativeDocumentRepublicationError::Mapping(_))
    ));
    let other_cancel = Cancellation::default();
    let other = request(&other_cancel);
    let other_fixture = Fixture::new(&other);
    let foreign_r = capture(&other_fixture, &other, "foreign.md", "Foreign source");
    let foreign = compile(&foreign_r, &other, "fallback");
    assert!(matches!(
        republish_native_documents(
            ctx.snapshot(),
            &read,
            &write,
            &first,
            &[input(&foreign, "a")],
            limits()
        )
        .await,
        Err(NativeDocumentRepublicationError::Mapping(_))
    ));
    assert_eq!(read_docs(ctx.snapshot(), &read).await.0, first);
    cancel.cancel();
    let error = republish_native_documents(
        ctx.snapshot(),
        &read,
        &write,
        &first,
        &[input(&a, "a")],
        limits(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error,NativeDocumentRepublicationError::Data(KnowledgeDocumentUpdateError::Data(KnowledgeIndexError::Request(ref e))) if e.kind()==std::io::ErrorKind::Interrupted)
    );
    Ok(())
}
