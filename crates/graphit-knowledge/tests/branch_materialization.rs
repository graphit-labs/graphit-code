//! Thin Knowledge materialization followed by actual captured FILE republication.
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
    knowledge_branch_materialization::*,
    knowledge_document_projection as h,
    knowledge_document_update::{KnowledgeDocumentUpdateLimits, KnowledgeDocumentUpdateResult},
    knowledge_index::*,
    knowledge_reference_query::*,
};
use graphit_fs::NodeKind;
use graphit_hub::policy::RequestContext;
use graphit_knowledge::{
    branch_materialization::*,
    compilation_fingerprint::CompilationFingerprintLimits,
    document::DocumentParseLimits,
    document_republication::{NativeDocumentRepublicationLimits, republish_native_documents},
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
fn publication_limits() -> NativeDocumentPublicationLimits {
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

fn limits() -> KnowledgeDocumentUpdateLimits {
    KnowledgeDocumentUpdateLimits {
        read: read_limits(),
        write: write_limits(),
    }
}
fn branch(f: &Fixture) -> graphit_core::version_context::VersionContext {
    use graphit_core::{
        project_paths::{InstallationPaths, ProjectPaths},
        version_context::{Checkout, VersionContext},
    };
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
        "knowledge β",
        None,
        "",
    )
    .unwrap()
}
#[tokio::test]
async fn real_public_knowledge_adapter_then_compiled_republication_changes_named_only()
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
    let write = auth.admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)?;
    let r = capture(
        &f,
        &ctx,
        "first.md",
        "---\ntitle: First\n---\nFull source\n",
    );
    let d = compile(&r, &ctx, "fallback");
    let first = publish_native_documents(
        ctx.snapshot(),
        &write,
        &[input(&d, "a")],
        publication_limits(),
    )
    .await?
    .default;
    let default_read =
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
    let (_, before) = read_docs(ctx.snapshot(), &default_read).await;
    let selected = branch(&f);
    let scope = f.scope_for(&selected);
    let read = auth.admit_authenticated_read(&caller, &ctx, scope, ProjectReadModule::Knowledge)?;
    let write =
        auth.admit_authenticated_write(&caller, &ctx, scope, ProjectWriteOperation::Knowledge)?;
    let (inherited, documents) = read_docs(ctx.snapshot(), &read).await;
    assert!(inherited.inherited_target.is_some());
    assert_eq!(documents, before);
    let KnowledgeBranchMaterialization::Materialized {
        generation,
        inherited: observed,
    } = materialize_native_document_branch(ctx.snapshot(), &read, &write, &inherited, limits())
        .await?
    else {
        panic!()
    };
    assert_eq!(observed, inherited);
    assert_eq!(generation.version, first.version);
    assert!(generation.physical_branch.is_some());
    assert_eq!(generation.inherited_target, None);
    assert_eq!(read_docs(ctx.snapshot(), &read).await.1, before);
    let updated = capture(
        &f,
        &ctx,
        "new.md",
        "---\ntitle: Changed\nreferences: []\n---\nNew whole source [[a]]\n",
    );
    let compiled = compile(&updated, &ctx, "fallback");
    let result = republish_native_documents(
        ctx.snapshot(),
        &read,
        &write,
        &generation,
        &[input(&compiled, "a")],
        NativeDocumentRepublicationLimits {
            publication: publication_limits(),
            read: read_limits(),
        },
    )
    .await?;
    assert!(matches!(
        result,
        KnowledgeDocumentUpdateResult::Updated { .. }
    ));
    let (_, named) = read_docs(ctx.snapshot(), &read).await;
    assert_eq!(named[0].body, compiled.whole_source());
    assert_eq!(named[0].title, "Changed");
    let (default, unchanged) = read_docs(ctx.snapshot(), &default_read).await;
    assert_eq!(default, first);
    assert_eq!(unchanged, before);
    let KnowledgeReferencesRead::Published { read: refs, .. } =
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
    assert!(refs.coverage.knowledge_source_complete);
    assert!(refs.warnings.is_empty());
    assert_eq!(refs.outgoing.unwrap().total, 1);
    Ok(())
}
#[tokio::test]
async fn thin_adapter_retains_data_original_cancel_and_noop_contracts() -> Result<(), Box<dyn Error>>
{
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
    let rows = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
    let first = publish_native_authored(ctx.snapshot(), &write, &rows, write_limits())
        .await?
        .default;
    assert!(
        matches!(materialize_native_document_branch(ctx.snapshot(),&read,&write,&first,limits()).await?,KnowledgeBranchMaterialization::Unchanged{generation} if generation==first)
    );
    let other_cancel = Cancellation::default();
    let other = request(&other_cancel);
    assert!(matches!(
        materialize_native_document_branch(other.snapshot(), &read, &write, &first, limits()).await,
        Err(KnowledgeBranchMaterializationError::Data(
            KnowledgeIndexError::DifferentRequest
        ))
    ));
    cancel.cancel();
    assert!(
        matches!(materialize_native_document_branch(ctx.snapshot(),&read,&write,&first,limits()).await,Err(KnowledgeBranchMaterializationError::Data(KnowledgeIndexError::Request(ref e))) if e.kind()==std::io::ErrorKind::Interrupted)
    );
    Ok(())
}
