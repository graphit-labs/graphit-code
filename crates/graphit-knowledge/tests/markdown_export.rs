use graphit_core::{
    artifact_store::{CopyLimits, GeneratedNode, PayloadMode, PayloadPolicy, ValidatedPayloadPlan},
    file_io::SnapshotContext,
    file_lock::Cancellation,
};
use graphit_data::{
    knowledge_document_projection::CompilationDigest32,
    knowledge_index::{KnowledgeDocument, KnowledgeRecipeParameters},
};
use graphit_fs::NodeKind;
use graphit_knowledge::{
    document::DocumentParseLimits,
    markdown_export::{
        MarkdownExportError as E, MarkdownExportLimits, MarkdownExportModes, MarkdownExportPlan,
        MarkdownExportStage,
    },
    okf_export::KnowledgeTextExportFormat,
    references::{ReferenceError, ReferenceLimits},
};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    io,
    num::NonZeroUsize,
    path::Path,
    time::{Duration, Instant},
};

fn n(v: usize) -> NonZeroUsize {
    NonZeroUsize::new(v).unwrap()
}
fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}
fn doc(slug: &str) -> KnowledgeDocument {
    KnowledgeDocument {
        slug: slug.into(),
        title: format!("Title {slug}"),
        body: "  # Literal\r\n\r\nbody  \r\n".into(),
        kind: "document".into(),
        summary: "preview".into(),
        description: Some("integral".into()),
        summary_truncated: false,
        source_path: "docs/input.md".into(),
        physical_digest: None,
        compilation_digest: CompilationDigest32::from_bytes([42; 32]),
        recipe: "authored-document-v1".into(),
        recipe_parameters: KnowledgeRecipeParameters {
            fallback_title: "fallback".into(),
            kind_on_absent: "document".into(),
            max_summary_bytes: n(32),
        },
        updated_at: None,
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: vec![],
    }
}
fn limits() -> MarkdownExportLimits {
    MarkdownExportLimits {
        max_file_bytes: n(1_000_000),
        document: DocumentParseLimits {
            max_document_bytes: n(65536),
            max_input_bytes: n(131072),
            max_frontmatter_bytes: n(65536),
            max_yaml_events: n(1000),
            max_markdown_events: n(1000),
            max_yaml_depth: n(32),
            max_scalar_bytes: n(65536),
            max_links: n(100),
            max_projection_bytes: n(131072),
            max_summary_bytes: n(32),
        },
        references: ReferenceLimits {
            max_identifier_bytes: n(65536),
        },
    }
}
fn modes() -> MarkdownExportModes {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("file"), b"mode observation").unwrap();
    MarkdownExportModes {
        directory: PayloadMode::from_permissions(
            &std::fs::metadata(directory.path()).unwrap().permissions(),
        )
        .unwrap(),
        file: PayloadMode::from_permissions(
            &std::fs::metadata(directory.path().join("file"))
                .unwrap()
                .permissions(),
        )
        .unwrap(),
    }
}
fn policy(m: MarkdownExportModes, bytes: u64, nodes: usize) -> PayloadPolicy {
    PayloadPolicy::new(
        NodeKind::Directory,
        m.directory,
        CopyLimits::new(bytes, nodes, 1_000_000, 16).unwrap(),
    )
}
fn stage_export(
    ctx: &SnapshotContext,
    documents: &[KnowledgeDocument],
    modes: MarkdownExportModes,
    limits: MarkdownExportLimits,
) -> Result<MarkdownExportPlan, E> {
    let mut stage = MarkdownExportStage::new_in(
        ctx,
        &std::env::temp_dir(),
        modes,
        limits,
        KnowledgeTextExportFormat::Markdown,
    )?;
    for document in documents {
        stage.push(ctx, document.clone())?;
    }
    stage.finish(ctx)
}

#[test]
fn file_backed_tree_namespace_and_fingerprint_are_deterministic() {
    let ctx = context();
    let m = modes();
    let mut documents = vec![
        doc("index"),
        doc("CON"),
        doc("A"),
        doc("a"),
        doc("世界"),
        doc(&"x".repeat(600)),
        doc("alpha"),
    ];
    let plan = stage_export(&ctx, &documents, m, limits()).unwrap();

    let nodes = plan.payload.nodes().collect::<Vec<_>>();
    assert_eq!(nodes.len(), 10);
    assert_eq!(
        nodes
            .iter()
            .filter(|n| n.kind() == NodeKind::Directory)
            .count(),
        2
    );
    assert!(
        nodes
            .iter()
            .any(|n| n.relative() == Path::new("index.md") && n.kind() == NodeKind::File)
    );
    assert!(nodes.iter().any(|n| n.relative()
        == Path::new(
            "pages/p-8ed3f6ad685b959ead7022518e1af76cd816f8e8ec7ccdda1ed4018e8f2223f8.md"
        )));
    for node in nodes
        .iter()
        .filter(|n| n.relative().parent() == Some(Path::new("pages")))
    {
        let file = node.relative().file_name().unwrap().to_str().unwrap();
        assert_eq!(file.len(), 69);
        assert!(file.is_ascii());
        assert!(file.starts_with("p-"));
        assert!(file.ends_with(".md"));
        assert!(node.digest().is_some());
    }
    documents.reverse();
    let reversed = stage_export(&ctx, &documents, m, limits()).unwrap();
    assert_eq!(plan.payload.fingerprint(), reversed.payload.fingerprint());
    assert_eq!(plan.communities, reversed.communities);
}

#[test]
fn owned_communities_are_deterministic_with_reordered_batches() {
    let ctx = context();
    let m = modes();
    let mut a = doc("a");
    a.title = "Página Á".into();
    a.resolved_wiki_targets = vec!["z".into(), "ghost".into()];
    let mut z = doc("z");
    z.resolved_wiki_targets = vec!["a".into()];
    let mut documents = vec![z, doc("alone"), a];
    let plan = stage_export(&ctx, &documents, m, limits()).unwrap();
    assert_eq!(plan.communities.groups.len(), 1);
    assert_eq!(plan.communities.groups[0].members, ["a", "z"]);
    assert_eq!(plan.communities.groups[0].label, "Página Á");
    assert_eq!(plan.communities.groups[0].cohesion, 1.0);
    documents.reverse();
    let repeated = stage_export(&ctx, &documents, m, limits()).unwrap();
    assert_eq!(repeated.communities, plan.communities);
    assert_eq!(repeated.payload.fingerprint(), plan.payload.fingerprint());
}

#[test]
fn file_backed_tree_matches_independent_markdown_bytes_and_core_fingerprint() {
    use graphit_knowledge::document_source::{DocumentSourceLimits, render_document_source};
    let ctx = context();
    let m = modes();
    let p = policy(m, 1_000_000, 100);
    let d = doc("alpha");
    let source = render_document_source(
        &ctx,
        &d,
        &graphit_core::text_slice::Request::default(),
        DocumentSourceLimits {
            max_input_bytes: n(65536),
            max_targets: n(100),
            max_virtual_bytes: n(65536),
            max_virtual_lines: n(1000),
            max_output_bytes: n(65536),
            max_matches: n(1000),
        },
    )
    .unwrap();
    // Existing Source is the metadata contract; the complete Markdown bytes below
    // are an independent fixture, without the exporter's filename/render helpers.
    let header = source.source.strip_suffix(&d.body).unwrap();
    let page = format!("{header}# Title alpha\n\n## Content\n\n{}", d.body).into_bytes();
    let index=b"# Knowledge index\n\n1 pages\n\n* [Title alpha](pages/p-8ed3f6ad685b959ead7022518e1af76cd816f8e8ec7ccdda1ed4018e8f2223f8.md)".to_vec();
    let page_digest: [u8; 32] = Sha256::digest(&page).into();
    let index_digest: [u8; 32] = Sha256::digest(&index).into();
    let expected = ValidatedPayloadPlan::generated(
        [
            GeneratedNode::directory(std::path::PathBuf::new(), m.directory).unwrap(),
            GeneratedNode::directory("pages".into(), m.directory).unwrap(),
            GeneratedNode::file("index.md".into(), index, m.file).unwrap(),
            GeneratedNode::file(
                "pages/p-8ed3f6ad685b959ead7022518e1af76cd816f8e8ec7ccdda1ed4018e8f2223f8.md"
                    .into(),
                page,
                m.file,
            )
            .unwrap(),
        ],
        &p,
        &ctx,
    )
    .unwrap();
    let actual = stage_export(&ctx, &[d], m, limits()).unwrap();
    assert_eq!(
        actual
            .payload
            .nodes()
            .find(|n| n.relative() == Path::new("index.md"))
            .unwrap()
            .digest(),
        Some(index_digest)
    );
    assert_eq!(
        actual
            .payload
            .nodes()
            .find(|n| n.relative().parent() == Some(Path::new("pages")))
            .unwrap()
            .digest(),
        Some(page_digest)
    );
    assert_eq!(actual.payload.fingerprint(), expected.fingerprint());
}

#[test]
fn staged_body_is_final_suffix_and_private_files_live_with_the_plan() {
    use pulldown_cmark::{Event, Options, Parser, Tag};
    let ctx = context();
    let m = modes();
    for literal in ["```\r\nopen fence\r\n", "<div>\nopen html\n", ""] {
        let mut a = doc("alpha");
        a.body = format!("\u{feff}---\r\ntitle: Original\r\n---\r\n{literal}");
        a.resolved_wiki_targets = vec!["beta".into(), "missing".into(), "beta".into()];
        let mut b = doc("beta");
        b.resolved_wiki_targets = vec!["alpha".into()];
        let plan = stage_export(&ctx, &[a, b], m, limits()).unwrap();
        let directory = plan.directory();
        let source =
            std::fs::read_to_string(directory.join(
                "pages/p-8ed3f6ad685b959ead7022518e1af76cd816f8e8ec7ccdda1ed4018e8f2223f8.md",
            ))
            .unwrap();
        let (_, content) = source.split_once("\n---\n\n").unwrap();
        assert!(content.ends_with(literal));
        let prefix = &content[..content.len() - literal.len()];
        assert!(prefix.ends_with("## Content\n\n"));
        assert!(!prefix.contains("title: Original"));
        assert!(prefix.contains("## References"));
        assert!(prefix.contains("## Backlinks"));
        assert!(prefix.contains("missing (missing)"));
        let links = Parser::new_ext(prefix, Options::empty())
            .filter_map(|event| match event {
                Event::Start(Tag::Link { dest_url, .. }) => Some(dest_url.into_string()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            links,
            vec!["p-f44e64e75f3948e9f73f8dfa94721c4ce8cbb4f265c4790c702b2d41cfbf2753.md"; 2]
        );

        let (communities, cleanup) = plan.close();
        assert!(cleanup.is_ok());
        assert_eq!(communities.groups[0].members, ["alpha", "beta"]);
        assert!(!directory.exists());
    }
}

#[test]
fn invalid_ids_duplicates_and_empty_are_rejected() {
    let ctx = context();
    let m = modes();
    for slug in ["", "../x", "a/b", "a\\b"] {
        assert!(matches!(
            stage_export(&ctx, &[doc(slug)], m, limits()),
            Err(E::References(ReferenceError::InvalidIdentifier))
        ));
    }
    assert!(matches!(
        stage_export(&ctx, &[doc("same"), doc("same")], m, limits()),
        Err(E::References(ReferenceError::DuplicatePage))
    ));
    let err = stage_export(&ctx, &[], m, limits()).err().unwrap();
    assert!(matches!(err, E::NoDocuments));
    assert!(err.source().is_none());
}

#[test]
fn individual_file_bound_preserves_complete_generated_tree() {
    let ctx = context();
    let m = modes();
    let mut a = doc("alpha");
    a.resolved_wiki_targets = vec!["beta".into(), "alpha".into(), "beta".into()];
    let mut b = doc("beta");
    b.resolved_wiki_targets = vec!["alpha".into()];
    let documents = [a, b];
    let baseline = stage_export(&ctx, &documents, m, limits()).unwrap();
    let file = baseline
        .payload
        .nodes()
        .map(|n| usize::try_from(n.bytes()).unwrap())
        .max()
        .unwrap();
    let mut exact = limits();
    exact.max_file_bytes = n(file);
    assert_eq!(
        stage_export(&ctx, &documents, m, exact)
            .unwrap()
            .payload
            .fingerprint(),
        baseline.payload.fingerprint()
    );
    let mut below = exact;
    below.max_file_bytes = n(file - 1);
    let error = stage_export(&ctx, &documents, m, below).err().unwrap();
    assert!(matches!(error, E::Limit("file bytes")));
}

#[test]
fn generated_directory_keeps_all_pages_beyond_the_old_snapshot_node_limit() {
    let original = context();
    let documents = (0..101)
        .map(|index| doc(&format!("page-{index}")))
        .collect::<Vec<_>>();
    let plan = stage_export(&original, &documents, modes(), limits()).unwrap();
    assert_eq!(plan.payload.nodes().len(), documents.len() + 3);
    assert_eq!(
        plan.payload
            .nodes()
            .filter(|node| node.kind() == NodeKind::File)
            .count(),
        documents.len() + 1
    );
    plan.payload.revalidate_content(&original).unwrap();
    let output = plan.directory();
    let (_, cleanup) = plan.close();
    cleanup.unwrap();
    assert!(!output.exists());
}

#[test]
fn raw_inputs_parser_and_reference_limits_are_not_bypassed() {
    let ctx = context();
    let m = modes();
    let mut a = doc("alpha");
    a.resolved_wiki_targets = vec!["alpha".into(); 3];
    let mut l = limits();
    stage_export(&ctx, &[a], m, l).unwrap();
    let a = doc("alpha");
    l = limits();
    l.document.max_document_bytes = n(a.body.len() - 1);
    let e = stage_export(&ctx, &[a], m, l).err().unwrap();
    assert!(matches!(e, E::Parse(_)));
    assert!(e.source().is_some());
    l = limits();
    l.references.max_identifier_bytes = n(4);
    assert!(matches!(
        stage_export(&ctx, &[doc("alpha")], m, l),
        Err(E::References(ReferenceError::LimitExceeded(_)))
    ));
    let mut invalid = doc("alpha");
    invalid.body = "---\ntitle: [\n---\nbody".into();
    let e = stage_export(&ctx, &[invalid], m, limits()).err().unwrap();
    assert!(matches!(e, E::Parse(_)));
    assert!(e.source().is_some());
    let mut nul = doc("alpha");
    nul.title = "invalid\0label".into();
    assert!(matches!(
        stage_export(&ctx, &[nul], m, limits()),
        Err(E::InvalidDisplayText)
    ));
}

#[test]
fn initial_cancel_and_deadline_precede_empty_or_invalid_content() {
    let m = modes();
    let cancelled = Cancellation::default();
    cancelled.cancel();
    for (ctx, kind) in [
        (
            SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancelled),
            io::ErrorKind::Interrupted,
        ),
        (
            SnapshotContext::new(Instant::now(), Cancellation::default()),
            io::ErrorKind::TimedOut,
        ),
    ] {
        for documents in [vec![], vec![doc("../invalid")]] {
            let e = stage_export(&ctx, &documents, m, limits()).err().unwrap();
            assert!(matches!(&e,E::Request(cause) if cause.kind()==kind));
            assert_eq!(
                e.source()
                    .unwrap()
                    .downcast_ref::<io::Error>()
                    .unwrap()
                    .kind(),
                kind
            );
        }
    }
}
