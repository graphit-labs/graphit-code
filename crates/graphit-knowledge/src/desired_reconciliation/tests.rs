use super::*;
use crate::desired_reconciliation_fixture as fixture;
use fixture::*;
use graphit_core::file_lock::Cancellation;
use std::time::{Duration, Instant};

#[tokio::test]
async fn relative_file_targets_reconcile_without_source_edits_or_vector_loss()
-> std::result::Result<(), Box<dyn Error>> {
    use crate::embedding_projection::prepare_knowledge_embedding_document;
    use graphit_data::{
        knowledge_document_lifecycle::KnowledgeSourceIdentity, knowledge_embeddings::*,
        knowledge_reference_query::*, lance_branch_store::BranchSnapshot,
    };
    use std::{collections::BTreeSet, path::Path};
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = graphit_auth::ProjectRecordAuthority::new(&issuer);
    let read =
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    let body = "---\nreferences: []\n---\n# Source\n[target](z.md#part) [again](./z.md?raw=1) [[literal.md]] [self](#local) [hidden](ignored.md) [url](https://example.test/z.md)\n";
    put(&f, "docs/a.md", body);
    put(&f, "docs/z.md", "---\nid: 'Manual Ω'\n---\n# Target\n");
    put(&f, "docs/ignored.md", "---\nid: [invalid]\n---\n");
    put(&f, ".wikiignore", "docs/ignored.md\n");
    let config = config(&[("knowledge.include_readme", serde_json::json!(false))]);
    let on_file = |_: &str| {};
    let before_file = || Ok(());
    let run = || {
        reconcile_knowledge_source_changes(
            ctx.snapshot(),
            &read,
            &write,
            &config,
            "page",
            None,
            &on_file,
            &before_file,
            limits(),
        )
    };
    run().await?;
    let source =
        KnowledgeSourceIdentity::from_relative_path(Path::new("docs/a.md"), None, n(4096))?;
    let hidden =
        KnowledgeSourceIdentity::from_relative_path(Path::new("docs/ignored.md"), None, n(4096))?;
    let KnowledgeEmbeddingRead::Published(capture) = capture_knowledge_embedding_selection(
        ctx.snapshot(),
        &read,
        &BTreeSet::from([source.slug().to_owned()]),
        limits().index.read,
    )
    .await?
    else {
        panic!("published selected Source");
    };
    let mut docs = Vec::new();
    while let Some(batch) = capture.next_documents(ctx.snapshot(), &read).await? {
        docs.extend(batch);
    }
    assert_eq!(docs.len(), 1);
    assert_eq!(
        docs[0].resolved_wiki_targets,
        [
            "Manual Ω",
            "Manual Ω",
            "literal.md",
            source.slug(),
            hidden.slug()
        ]
    );
    let identity = KnowledgeEmbeddingIdentity::new(
        [7; 32],
        std::num::NonZeroU32::new(2).unwrap(),
        "knowledge-document-json-v1".into(),
    )?;
    let input =
        prepare_knowledge_embedding_document(ctx.snapshot(), &capture, &docs[0], &identity)?;
    let fingerprint = input.input_fingerprint();
    publish_knowledge_embeddings(
        ctx.snapshot(),
        &write,
        &capture,
        &identity,
        &[KnowledgeEmbeddingRow {
            input: KnowledgeEmbeddingInput {
                slug: source.slug(),
                input_fingerprint: fingerprint,
            },
            vector: &[-0.0, 0.125],
        }],
        KnowledgeEmbeddingWriteMode::Attach,
        limits().index.write,
    )
    .await?;
    let uri = capture.source_generation().corpus_uri.clone();
    let before = image(&BranchSnapshot::capture_observed(&uri, None, None).await?).await?;
    let source_before = &before[source.slug()];
    assert_eq!(source_before.num_columns(), 23);

    // The source bytes are untouched. Resolving a changed authored target ID
    // must bypass digest-only no-op logic and retain the source's model input.
    put(&f, "docs/z.md", "---\nid: 'Manual β'\n---\n# Target\n");
    run().await?;
    let after = image(&BranchSnapshot::capture_observed(&uri, None, None).await?).await?;
    let source_after = &after[source.slug()];
    assert_eq!(text(source_after, 2), body);
    assert_eq!(
        source_before.column(9).as_ref(),
        source_after.column(9).as_ref()
    );
    for column in 18..23 {
        assert_eq!(
            source_before.column(column).as_ref(),
            source_after.column(column).as_ref()
        );
    }
    let bits = |row: &arrow_array::RecordBatch| {
        let vector = row
            .column(22)
            .as_any()
            .downcast_ref::<arrow_array::FixedSizeListArray>()
            .unwrap()
            .value(0);
        vector
            .as_any()
            .downcast_ref::<arrow_array::Float32Array>()
            .unwrap()
            .values()
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>()
    };
    assert_eq!(bits(source_before), bits(source_after));
    assert_eq!(
        bits(source_after),
        [(-0.0f32).to_bits(), 0.125f32.to_bits()]
    );
    let KnowledgeEmbeddingRead::Published(current) = capture_knowledge_embedding_selection(
        ctx.snapshot(),
        &read,
        &BTreeSet::from([source.slug().to_owned()]),
        limits().index.read,
    )
    .await?
    else {
        panic!();
    };
    let mut docs = Vec::new();
    while let Some(batch) = current.next_documents(ctx.snapshot(), &read).await? {
        docs.extend(batch);
    }
    let projected =
        prepare_knowledge_embedding_document(ctx.snapshot(), &current, &docs[0], &identity)?;
    assert_eq!(projected.input_fingerprint(), fingerprint);
    assert!(!projected.is_pending());
    assert_eq!(
        docs[0].resolved_wiki_targets,
        [
            "Manual β",
            "Manual β",
            "literal.md",
            source.slug(),
            hidden.slug()
        ]
    );
    let KnowledgeReferencesRead::Published { read: refs, .. } =
        read_native_knowledge_reference_query(
            ctx.snapshot(),
            &read,
            KnowledgeReferenceQuery {
                outgoing: Some(source.slug()),
                incoming: None,
                page_size: n(10),
                outgoing_cursor: None,
                incoming_cursor: None,
            },
            limits().index.read,
        )
        .await?
    else {
        panic!();
    };
    assert!(refs.outgoing.unwrap().complete);
    assert_eq!(refs.warnings.len(), 1);
    let KnowledgeReferencesWarning::MissingCurrentProjection { source: incomplete } =
        &refs.warnings[0];
    assert_eq!(incomplete.slug, "Manual β");
    assert!(incomplete.witness.is_none());
    assert!(!after.contains_key(hidden.slug()));
    let unchanged = run().await?;
    assert!(matches!(
        unchanged,
        KnowledgeDesiredReconciliationResult::Existing(
            KnowledgeDocumentUpdateResult::Unchanged { .. }
        )
    ));
    Ok(())
}

#[tokio::test]
async fn relative_link_identity_reads_only_frontmatter_and_preserves_failed_tail_receipt()
-> std::result::Result<(), Box<dyn Error>> {
    use graphit_data::knowledge_index::{KnowledgeIndexRead, read_native_knowledge_index};
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = graphit_auth::ProjectRecordAuthority::new(&issuer);
    let read =
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    put(
        &f,
        "docs/a.md",
        "---\nreferences: []\n---\n# Source\n[target](z.md)\n",
    );
    let mut target = format!(
        "\u{feff}---\r\ndescription: '{}'\r\nid: 'Literal Ω'\r\n---\r\n",
        "x".repeat(300)
    )
    .into_bytes();
    target.push(0xff);
    put(&f, "docs/z.md", target);
    let error = reconcile_knowledge_source_changes(
        ctx.snapshot(),
        &read,
        &write,
        &config(&[("knowledge.include_readme", serde_json::json!(false))]),
        "page",
        None,
        &|_| {},
        &|| Ok(()),
        limits(),
    )
    .await
    .unwrap_err();
    let KnowledgeDesiredReconciliationError::Published { pending_paths, .. } = error else {
        panic!("the source committed before the target body was parsed");
    };
    assert_eq!(
        pending_paths,
        std::collections::BTreeSet::from([std::path::PathBuf::from("docs/z.md")])
    );
    let KnowledgeIndexRead::Published { documents, .. } =
        read_native_knowledge_index(ctx.snapshot(), &read, limits().index.read).await?
    else {
        panic!();
    };
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].resolved_wiki_targets, ["Literal Ω"]);
    Ok(())
}

#[tokio::test]
async fn relative_link_frontmatter_cap_is_not_reported_as_corrupt_utf8()
-> std::result::Result<(), Box<dyn Error>> {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = graphit_auth::ProjectRecordAuthority::new(&issuer);
    let read =
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    put(&f, "docs/a.md", "[target](z.md)\n");
    put(&f, "docs/z.md", "---\nid: 'aaaaaaΩ'\n---\n");
    let mut bounds = limits();
    bounds.preparation.parser.max_frontmatter_bytes = n(16);
    let error = reconcile_knowledge_source_changes(
        ctx.snapshot(),
        &read,
        &write,
        &config(&[("knowledge.include_readme", serde_json::json!(false))]),
        "page",
        None,
        &|_| {},
        &|| Ok(()),
        bounds,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, KnowledgeDesiredReconciliationError::Source(cause)
        if matches!(*cause, crate::desired_corpus::DesiredCorpusError::Parser(crate::document::DocumentParseError::LimitExceeded("frontmatter bytes"))))
    );
    assert!(
        !f.project
            .runtime_root()
            .join("knowledge/index.lance")
            .exists()
    );
    // The first 256-byte chunk ends in exactly "---", but the actual
    // line continues. It must not certify a nonexistent closing delimiter.
    let mut target = b"---\nid: target\n#".to_vec();
    target.resize(252, b'x');
    target.push(b'\n');
    target.extend_from_slice(b"---not a closing delimiter\n---\n");
    put(&f, "docs/z.md", target);
    let error = reconcile_knowledge_source_changes(
        ctx.snapshot(),
        &read,
        &write,
        &config(&[("knowledge.include_readme", serde_json::json!(false))]),
        "page",
        None,
        &|_| {},
        &|| Ok(()),
        limits(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, KnowledgeDesiredReconciliationError::Source(cause)
        if matches!(*cause, crate::desired_corpus::DesiredCorpusError::Parser(crate::document::DocumentParseError::Yaml { .. })))
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
async fn relative_target_captures_do_not_accumulate_checkout_fds_across_files()
-> std::result::Result<(), Box<dyn Error>> {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = graphit_auth::ProjectRecordAuthority::new(&issuer);
    let read =
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    for index in 0..24 {
        put(
            &f,
            format!("docs/a{index:02}.md"),
            format!(
                "---\nid: source-{index:02}\nreferences: []\n---\n[target](targets/t{index:02}.md)\n"
            ),
        );
        put(
            &f,
            format!("docs/targets/t{index:02}.md"),
            format!("---\nid: target-{index:02}\n---\n# Target\n"),
        );
    }
    let observed = std::sync::Mutex::new(Vec::new());
    let root = f.project.owner_root();
    let runtime = f.project.runtime_root();
    let on_file = |slug: &str| {
        if slug.starts_with("source-") {
            let count = std::fs::read_dir("/proc/self/fd")
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter_map(|fd| match std::fs::read_link(fd) {
                    Ok(path) => Some(path),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => panic!("inspect checkout descriptors: {error}"),
                })
                .filter(|path| path.starts_with(root) && !path.starts_with(runtime))
                .count();
            observed.lock().unwrap().push(count);
        }
    };
    reconcile_knowledge_source_changes(
        ctx.snapshot(),
        &read,
        &write,
        &config(&[("knowledge.include_readme", serde_json::json!(false))]),
        "page",
        None,
        &on_file,
        &|| Ok(()),
        limits(),
    )
    .await?;
    let observed = observed.into_inner()?;
    assert_eq!(observed.len(), 24);
    let min = *observed.iter().min().unwrap();
    let max = *observed.iter().max().unwrap();
    assert!(
        max <= min + 4,
        "checkout FD ownership grew with distinct targets: {observed:?}"
    );
    Ok(())
}

#[tokio::test]
async fn progressive_files_remain_searchable_after_cancelled_tail_and_resume_only_delta()
-> std::result::Result<(), Box<dyn Error>> {
    use graphit_data::{
        knowledge_index::{KnowledgeIndexRead, read_native_knowledge_index},
        knowledge_search::{
            KnowledgeLexicalQuery, KnowledgeSearchLimits, KnowledgeSearchRead,
            capture_native_knowledge_search, query_native_knowledge_search,
        },
    };
    use std::collections::BTreeSet;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = graphit_auth::ProjectRecordAuthority::new(&issuer);
    let read =
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    put(
        &f,
        "docs/a.md",
        "---\nid: a\n---\n# Immediate\nVisiblealpha\n",
    );
    put(&f, "docs/z.md", "---\nid: z\n---\n# Later\nVisiblezeta\n");
    let config = config(&[("knowledge.include_readme", serde_json::json!(false))]);
    let error = reconcile_knowledge_source_changes(
        ctx.snapshot(),
        &read,
        &write,
        &config,
        "page",
        None,
        &|slug| {
            assert_eq!(slug, "a");
            cancel.cancel();
        },
        &|| Ok(()),
        limits(),
    )
    .await
    .unwrap_err();
    let KnowledgeDesiredReconciliationError::Published {
        publication,
        pending_paths,
        ..
    } = error
    else {
        panic!("a cancelled later FILE must retain the confirmed publication");
    };
    assert_eq!(
        pending_paths,
        BTreeSet::from([std::path::PathBuf::from("docs/z.md")])
    );
    let KnowledgeDesiredReconciliationResult::First { publication, .. } = *publication else {
        panic!("only the first FILE committed before cancellation");
    };
    let first = publication
        .named
        .as_ref()
        .unwrap_or(&publication.default)
        .clone();
    let live = Cancellation::default();
    let observe = request(&live);
    let issuer = f.issuer(&observe);
    let caller = issuer.authenticate_runtime(&f.secret(), &observe)?;
    let auth = graphit_auth::ProjectRecordAuthority::new(&issuer);
    let read =
        auth.admit_authenticated_read(&caller, &observe, f.scope(), ProjectReadModule::Knowledge)?;
    let write = auth.admit_authenticated_write(
        &caller,
        &observe,
        f.scope(),
        ProjectWriteOperation::Knowledge,
    )?;
    let KnowledgeIndexRead::Published {
        generation,
        documents,
    } = read_native_knowledge_index(observe.snapshot(), &read, limits().index.read).await?
    else {
        panic!("FILE A is already published");
    };
    assert_eq!(generation, first);
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].slug, "a");
    let a = documents[0].clone();
    let KnowledgeSearchRead::Published(capture) =
        capture_native_knowledge_search(observe.snapshot(), &read, limits().index.read).await?
    else {
        panic!("FTS is available before corpus EOF");
    };
    let page = query_native_knowledge_search(
        observe.snapshot(),
        &read,
        &capture,
        KnowledgeLexicalQuery {
            text: "Visiblealpha",
            top_k: 5,
            page_size: n(5),
            cursor: None,
            caller_binding: "progressive-source",
            preview: false,
        },
        KnowledgeSearchLimits {
            read: limits().index.read,
            max_query_bytes: n(1024),
            max_output_bytes: n(65536),
        },
        |_, _, _| unreachable!("preview is disabled"),
    )
    .await?;
    assert_eq!(page.hits.len(), 1);
    let delta = BTreeSet::from([std::path::PathBuf::from("docs/z.md")]);
    let changed = reconcile_knowledge_source_changes(
        observe.snapshot(),
        &read,
        &write,
        &config,
        "page",
        Some(&delta),
        &|slug| assert_eq!(slug, "z"),
        &|| Ok(()),
        limits(),
    )
    .await?;
    assert!(matches!(
        changed,
        KnowledgeDesiredReconciliationResult::Existing(
            KnowledgeDocumentUpdateResult::Updated { .. }
        )
    ));
    let KnowledgeIndexRead::Published { documents, .. } =
        read_native_knowledge_index(observe.snapshot(), &read, limits().index.read).await?
    else {
        panic!("the delta remains published");
    };
    assert_eq!(documents.len(), 2);
    assert_eq!(
        documents.iter().find(|document| document.slug == "a"),
        Some(&a)
    );
    let old_z = documents
        .iter()
        .find(|document| document.slug == "z")
        .unwrap();
    let old_origin = old_z.source_path.clone();
    std::fs::rename(
        f.project.owner_root().join("docs/z.md"),
        f.project.owner_root().join("docs/renamed.md"),
    )?;
    let rename = BTreeSet::from([
        std::path::PathBuf::from("docs/z.md"),
        std::path::PathBuf::from("docs/renamed.md"),
    ]);
    reconcile_knowledge_source_changes(
        observe.snapshot(),
        &read,
        &write,
        &config,
        "page",
        Some(&rename),
        &|slug| assert_eq!(slug, "z"),
        &|| Ok(()),
        limits(),
    )
    .await?;
    let KnowledgeIndexRead::Published { documents, .. } =
        read_native_knowledge_index(observe.snapshot(), &read, limits().index.read).await?
    else {
        panic!("renamed FILE remains readable");
    };
    assert_eq!(documents.len(), 2);
    assert_eq!(
        documents.iter().find(|document| document.slug == "a"),
        Some(&a)
    );
    let z = documents
        .iter()
        .find(|document| document.slug == "z")
        .unwrap();
    assert_ne!(z.source_path, old_origin);
    assert_eq!(z.body, old_z.body);
    Ok(())
}

#[tokio::test]
async fn original_auth_request_race_preserves_cancel_deadline_and_denied_causes()
-> std::result::Result<(), Box<dyn Error>> {
    #[derive(Clone, Copy, Debug)]
    enum Guard {
        Cancel,
        Deadline,
        Revoke,
    }
    for guard in [Guard::Cancel, Guard::Deadline, Guard::Revoke] {
        let cancel = Cancellation::default();
        let deadline = Instant::now()
            + Duration::from_secs(if matches!(guard, Guard::Deadline) {
                2
            } else {
                180
            });
        let ctx = request_deadline(&cancel, deadline);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
        let admin = issuer.bootstrap_admin(&caller)?;
        let parent = issuer.admin_parent(&caller, &admin)?;
        let (bearer, reference) = f.issue(&issuer, &parent);
        let grant = issuer.verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)?;
        let auth = graphit_auth::ProjectRecordAuthority::new(&issuer);
        let read = auth.admit_read(&grant, &ctx, ProjectReadModule::Knowledge)?;
        let write = auth.admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)?;
        assert!(matches!(
            auth_error(ctx.snapshot(), graphit_auth::AuthError::Request),
            KnowledgeDesiredReconciliationError::Auth(graphit_auth::AuthError::Request)
        ));
        let reached = std::cell::Cell::new(false);
        let error = check_inner(ctx.snapshot(), &read, &write, &|| {
            // Genuine Original check already succeeded; change the real caller
            // state before its next Auth accessor, not a mocked Auth result.
            reached.set(true);
            match guard {
                Guard::Cancel => cancel.cancel(),
                Guard::Deadline => std::thread::sleep(
                    deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
                ),
                Guard::Revoke => issuer.revoke(&parent, &bearer).unwrap(),
            }
        })
        .unwrap_err();
        assert!(reached.get());
        match guard {
            Guard::Cancel => assert!(
                matches!(error,KnowledgeDesiredReconciliationError::Request(ref io) if io.kind()==io::ErrorKind::Interrupted),
                "{error:?}"
            ),
            Guard::Deadline => assert!(
                matches!(error,KnowledgeDesiredReconciliationError::Request(ref io) if io.kind()==io::ErrorKind::TimedOut),
                "{error:?}"
            ),
            Guard::Revoke => assert!(
                matches!(
                    error,
                    KnowledgeDesiredReconciliationError::Auth(graphit_auth::AuthError::Denied)
                ),
                "{error:?}"
            ),
        }
        assert!(matches!(
            auth_error(ctx.snapshot(), graphit_auth::AuthError::Denied),
            KnowledgeDesiredReconciliationError::Auth(graphit_auth::AuthError::Denied)
        ));
        assert_no_birth(&f);
    }
    Ok(())
}

#[tokio::test]
async fn full_capture_opens_each_file_only_when_its_publication_starts()
-> std::result::Result<(), Box<dyn Error>> {
    use graphit_data::knowledge_index::{KnowledgeIndexRead, read_native_knowledge_index};
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let authority = graphit_auth::ProjectRecordAuthority::new(&issuer);
    let read = authority.admit_authenticated_read(
        &caller,
        &ctx,
        f.scope(),
        ProjectReadModule::Knowledge,
    )?;
    let write = authority.admit_authenticated_write(
        &caller,
        &ctx,
        f.scope(),
        ProjectWriteOperation::Knowledge,
    )?;
    put(&f, "docs/a.md", "---\nid: a\n---\n# Available first\n");
    put(&f, "docs/z.md", "---\nid: z\n---\n# Old tail\n");
    let current_tail = "---\nid: z\n---\n# Changed before its turn\nFresh tail content\n";
    reconcile_knowledge_source_changes(
        ctx.snapshot(),
        &read,
        &write,
        &config(&[("knowledge.include_readme", serde_json::json!(false))]),
        "page",
        None,
        &|slug| {
            if slug == "a" {
                // A is committed already. Z has not been opened or hashed;
                // its own admitted capture must observe this complete update.
                put(&f, "docs/z.md", current_tail);
            }
        },
        &|| Ok(()),
        limits(),
    )
    .await?;
    let KnowledgeIndexRead::Published { documents, .. } =
        read_native_knowledge_index(ctx.snapshot(), &read, limits().index.read).await?
    else {
        panic!("both independently captured FILEs must be published");
    };
    assert_eq!(documents.len(), 2);
    assert_eq!(
        documents
            .iter()
            .find(|document| document.slug == "z")
            .unwrap()
            .body,
        current_tail
    );
    Ok(())
}

#[tokio::test]
async fn background_yield_before_preparation_skips_capture_and_terminal_takes_precedence()
-> std::result::Result<(), Box<dyn Error>> {
    let cancellation = Cancellation::default();
    let background = Cancellation::default();
    let ctx = request(&cancellation);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let authority = graphit_auth::ProjectRecordAuthority::new(&issuer);
    let read = authority.admit_authenticated_read(
        &caller,
        &ctx,
        f.scope(),
        ProjectReadModule::Knowledge,
    )?;
    let write = authority.admit_authenticated_write(
        &caller,
        &ctx,
        f.scope(),
        ProjectWriteOperation::Knowledge,
    )?;
    put(&f, "docs/a.md", "---\nid: a\n---\n# Not admitted\n");
    let config = config(&[("knowledge.include_readme", serde_json::json!(false))]);
    assert!(
        !f.project.runtime_root().exists(),
        "fixture has no prepared runtime"
    );
    background.cancel();
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let before_file = || {
        calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        ctx.snapshot().check()?;
        if background.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "yield before preparation",
            ));
        }
        Ok(())
    };
    let empty = std::collections::BTreeSet::new();
    for changes in [None, Some(&empty)] {
        let error = reconcile_knowledge_source_changes(
            ctx.snapshot(),
            &read,
            &write,
            &config,
            "page",
            changes,
            &|_| panic!("superseded work cannot emit a FILE"),
            &before_file,
            limits(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, KnowledgeDesiredReconciliationError::Request(ref error)
            if error.kind() == io::ErrorKind::Interrupted)
        );
        ctx.check()?;
        assert!(
            !f.project.runtime_root().exists(),
            "yield must precede runtime preparation and capture"
        );
    }
    let yielded_calls = calls.load(std::sync::atomic::Ordering::Relaxed);
    cancellation.cancel();
    let error = reconcile_knowledge_source_changes(
        ctx.snapshot(),
        &read,
        &write,
        &config,
        "page",
        None,
        &|_| panic!("terminal cancellation cannot emit a FILE"),
        &before_file,
        limits(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, KnowledgeDesiredReconciliationError::Request(ref error)
        if error.kind() == io::ErrorKind::Interrupted)
    );
    assert_eq!(
        calls.load(std::sync::atomic::Ordering::Relaxed),
        yielded_calls,
        "terminal Original is checked before the background boundary"
    );
    assert!(!f.project.runtime_root().exists());
    Ok(())
}

#[tokio::test]
async fn background_yield_preserves_committed_prefix_and_pending_tail()
-> std::result::Result<(), Box<dyn Error>> {
    use graphit_data::knowledge_index::{KnowledgeIndexRead, read_native_knowledge_index};
    let cancellation = Cancellation::default();
    let background = Cancellation::default();
    let ctx = request(&cancellation);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let authority = graphit_auth::ProjectRecordAuthority::new(&issuer);
    let read = authority.admit_authenticated_read(
        &caller,
        &ctx,
        f.scope(),
        ProjectReadModule::Knowledge,
    )?;
    let write = authority.admit_authenticated_write(
        &caller,
        &ctx,
        f.scope(),
        ProjectWriteOperation::Knowledge,
    )?;
    put(&f, "docs/a.md", "---\nid: a\n---\n# Available first\n");
    put(&f, "docs/z.md", "---\nid: z\n---\n# Pending\n");
    let config = config(&[("knowledge.include_readme", serde_json::json!(false))]);
    let error = reconcile_knowledge_source_changes(
        ctx.snapshot(),
        &read,
        &write,
        &config,
        "page",
        None,
        &|slug| {
            assert_eq!(slug, "a");
            background.cancel();
        },
        &|| {
            if background.is_cancelled() {
                Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "yield to foreground delta",
                ))
            } else {
                Ok(())
            }
        },
        limits(),
    )
    .await
    .unwrap_err();
    let KnowledgeDesiredReconciliationError::Published {
        pending_paths,
        cause,
        ..
    } = error
    else {
        panic!("yield must preserve committed prefix and pending paths");
    };
    assert!(
        matches!(*cause, KnowledgeDesiredReconciliationError::Request(ref error)
        if error.kind() == io::ErrorKind::Interrupted)
    );
    assert_eq!(
        pending_paths,
        std::collections::BTreeSet::from([std::path::PathBuf::from("docs/z.md")])
    );
    ctx.check()?;
    let KnowledgeIndexRead::Published { documents, .. } =
        read_native_knowledge_index(ctx.snapshot(), &read, limits().index.read).await?
    else {
        panic!("foreground yield must leave A readable");
    };
    assert_eq!(documents.len(), 1);
    let a = documents[0].clone();
    put(&f, "docs/z.md", "---\nid: z\n---\n# Foreground change\n");
    reconcile_knowledge_source_changes(
        ctx.snapshot(),
        &read,
        &write,
        &config,
        "page",
        Some(&pending_paths),
        &|slug| assert_eq!(slug, "z"),
        &|| Ok(()),
        limits(),
    )
    .await?;
    let KnowledgeIndexRead::Published {
        generation: before_resume,
        documents,
    } = read_native_knowledge_index(ctx.snapshot(), &read, limits().index.read).await?
    else {
        panic!("foreground delta must publish without revisiting A");
    };
    assert_eq!(documents.len(), 2);
    assert_eq!(
        documents.iter().find(|document| document.slug == "a"),
        Some(&a)
    );
    let metadata_background = Cancellation::default();
    let emitted = std::sync::Mutex::new(Vec::new());
    let empty = std::collections::BTreeSet::new();
    let error = reconcile_knowledge_source_changes(
        ctx.snapshot(),
        &read,
        &write,
        &config,
        "page",
        Some(&empty),
        &|slug| {
            emitted.lock().unwrap().push(slug.to_owned());
            metadata_background.cancel();
        },
        &|| {
            ctx.snapshot().check()?;
            if metadata_background.is_cancelled() {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "yield during metadata resume",
                ));
            }
            Ok(())
        },
        limits(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, KnowledgeDesiredReconciliationError::Request(ref error)
        if error.kind() == io::ErrorKind::Interrupted)
    );
    assert_eq!(
        emitted.lock().unwrap().len(),
        1,
        "resume must yield before the next head"
    );
    ctx.check()?;
    let KnowledgeIndexRead::Published {
        generation: after_resume,
        documents: unchanged,
    } = read_native_knowledge_index(ctx.snapshot(), &read, limits().index.read).await?
    else {
        panic!("metadata yield must preserve the durable corpus");
    };
    assert_eq!(after_resume, before_resume);
    assert_eq!(unchanged, documents);
    let resumed = std::sync::Mutex::new(Vec::new());
    reconcile_knowledge_source_changes(
        ctx.snapshot(),
        &read,
        &write,
        &config,
        "page",
        Some(&empty),
        &|slug| resumed.lock().unwrap().push(slug.to_owned()),
        &|| Ok(()),
        limits(),
    )
    .await?;
    let mut resumed = resumed.into_inner().unwrap();
    resumed.sort();
    assert_eq!(resumed, ["a", "z"]);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn first_publication_race_reopens_branch_and_preserves_concurrent_files()
-> std::result::Result<(), Box<dyn Error>> {
    use graphit_data::{
        knowledge_document_lifecycle::read_native_knowledge_source_origins,
        knowledge_document_projection as projection,
        knowledge_document_update::{
            KnowledgeDocumentAuthoredUpdate, KnowledgeDocumentReferenceUpdate,
            KnowledgeDocumentUpdateLimits, update_existing_native_knowledge_documents,
        },
        knowledge_index::{KnowledgeIndexError, KnowledgeIndexRead, read_native_knowledge_index},
    };
    use std::{
        collections::BTreeSet,
        path::PathBuf,
        sync::{Arc, Mutex},
    };
    let temporal = (
        Some(std::time::UNIX_EPOCH - Duration::from_nanos(17)),
        Some(std::time::UNIX_EPOCH + Duration::from_nanos(29)),
        Some(Duration::from_secs(60)),
    );
    // A branch fork may retain a different compilation. A materialized target
    // must not silently replace a concurrent FILE or certify an obsolete empty view.
    for case in [
        "branch",
        "branch-missing",
        "same-file",
        "changed-file",
        "empty",
    ] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Arc::new(Fixture::new(&ctx));
        put(
            &f,
            "docs/a.md",
            "---\nid: a\n---\n# A\nUnchanged physical bytes.\n",
        );
        if case == "branch" {
            put(&f, "docs/b.md", "---\nid: b\n---\n# B\nCompeting FILE.\n");
        }
        std::fs::create_dir(f.project.owner_root().join("empty"))?;
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
        let authority = graphit_auth::ProjectRecordAuthority::new(&issuer);
        let version = f.named_version(if case.starts_with("branch") {
            "feature"
        } else {
            ""
        });
        let read = authority.admit_authenticated_read(
            &caller,
            &ctx,
            f.scope_for(&version),
            ProjectReadModule::Knowledge,
        )?;
        let write = authority.admit_authenticated_write(
            &caller,
            &ctx,
            f.scope_for(&version),
            ProjectWriteOperation::Knowledge,
        )?;
        let config = config(&[
            ("knowledge.include_readme", serde_json::json!(false)),
            (
                "knowledge.docs_dir",
                serde_json::json!(if case == "empty" { "empty" } else { "docs" }),
            ),
        ]);
        let selection = crate::desired_corpus::capture_selection_worker(
            ctx.snapshot(),
            &read,
            &config,
            None,
            limits().capture,
        )
        .await?;
        let (start, started) = tokio::sync::oneshot::channel();
        let (finished, finish) = std::sync::mpsc::channel();
        let winner_fixture = Arc::clone(&f);
        let winner = tokio::spawn(async move {
            let outcome: std::result::Result<(), String> = async {
                started.await.map_err(|error| error.to_string())?;
                let cancel = Cancellation::default();
                let ctx = request(&cancel);
                let issuer = winner_fixture.issuer(&ctx);
                let caller = issuer
                    .authenticate_runtime(&winner_fixture.secret(), &ctx)
                    .map_err(|e| e.to_string())?;
                let auth = graphit_auth::ProjectRecordAuthority::new(&issuer);
                let read = auth
                    .admit_authenticated_read(
                        &caller,
                        &ctx,
                        winner_fixture.scope(),
                        ProjectReadModule::Knowledge,
                    )
                    .map_err(|e| e.to_string())?;
                let write = auth
                    .admit_authenticated_write(
                        &caller,
                        &ctx,
                        winner_fixture.scope(),
                        ProjectWriteOperation::Knowledge,
                    )
                    .map_err(|e| e.to_string())?;
                if case == "branch-missing" {
                    put(
                        &winner_fixture,
                        "docs/c.md",
                        "---\nid: c\n---\n# C\nInherited FILE.\n",
                    );
                }
                let changes = BTreeSet::from([PathBuf::from(match case {
                    "branch" => "docs/b.md",
                    "branch-missing" => "docs/c.md",
                    _ => "docs/a.md",
                })]);
                reconcile_knowledge_source_changes(
                    ctx.snapshot(),
                    &read,
                    &write,
                    &fixture::config(&[("knowledge.include_readme", serde_json::json!(false))]),
                    if case == "same-file" {
                        "page"
                    } else {
                        "reference"
                    },
                    Some(&changes),
                    &|_| {},
                    &|| Ok(()),
                    limits(),
                )
                .await
                .map_err(|e| e.to_string())?;
                let KnowledgeIndexRead::Published {
                    generation,
                    documents,
                } = read_native_knowledge_index(ctx.snapshot(), &read, limits().index.read)
                    .await
                    .map_err(|e| e.to_string())?
                else {
                    return Err("winner did not publish".to_owned());
                };
                let targets: Vec<Vec<_>> = documents
                    .iter()
                    .map(|document| {
                        document
                            .resolved_wiki_targets
                            .iter()
                            .map(String::as_str)
                            .collect()
                    })
                    .collect();
                let fields: Vec<_> = documents
                    .iter()
                    .zip(&targets)
                    .map(|(document, targets)| projection::AuthoredPageFields {
                        slug: Some(&document.slug),
                        title: Some(&document.title),
                        body: Some(&document.body),
                        kind: Some(&document.kind),
                        summary: Some(&document.summary),
                        description: document.description.as_deref(),
                        summary_truncated: document.summary_truncated,
                        source_path: &document.source_path,
                        physical_digest: document.physical_digest,
                        compilation_digest: document.compilation_digest,
                        recipe: projection::RecipeFields {
                            id: &document.recipe,
                            parameters: Some(projection::AuthoredParameters {
                                fallback_title: &document.recipe_parameters.fallback_title,
                                kind_on_absent: &document.recipe_parameters.kind_on_absent,
                                max_summary_bytes: document.recipe_parameters.max_summary_bytes,
                            }),
                        },
                        updated_at: temporal.0,
                        stale_since: temporal.1,
                        stale_after: temporal.2,
                        resolved_wiki_targets: targets,
                    })
                    .collect();
                let rows = projection::prepare(ctx.snapshot(), &fields, &[], limits().projection)
                    .map_err(|e| e.to_string())?;
                let references: Vec<_> = documents
                    .iter()
                    .map(|document| KnowledgeDocumentReferenceUpdate {
                        slug: &document.slug,
                        authored: KnowledgeDocumentAuthoredUpdate::Preserve,
                    })
                    .collect();
                update_existing_native_knowledge_documents(
                    ctx.snapshot(),
                    &read,
                    &write,
                    &generation,
                    &rows,
                    &references,
                    KnowledgeDocumentUpdateLimits {
                        read: limits().index.read,
                        write: limits().index.write,
                    },
                )
                .await
                .map_err(|e| e.to_string())?;
                if case == "branch-missing" {
                    // The default baseline retains this FILE; the selected
                    // branch checkout no longer has it before its first parse.
                    std::fs::remove_file(winner_fixture.project.owner_root().join("docs/c.md"))
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            }
            .await;
            let _ = finished.send(outcome.clone());
            outcome
        });
        let start = Mutex::new(Some(start));
        let finish = Mutex::new(finish);
        let before_file = || {
            // This existing callback runs after origins are captured and before
            // FILE preparation. Hold only the test worker, not a product writer.
            let signal = start.lock().unwrap().take();
            if let Some(signal) = signal {
                signal
                    .send(())
                    .map_err(|_| io::Error::other("competitor stopped"))?;
                finish
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(30))
                    .map_err(io::Error::other)?
                    .map_err(io::Error::other)?;
            }
            Ok(())
        };
        let result = crate::source_stream::publish_source_changes(
            ctx.snapshot(),
            &read,
            &write,
            &selection,
            "page",
            None,
            &|_| {},
            &before_file,
            limits(),
        )
        .await;
        winner.await??;
        let current =
            read_native_knowledge_source_origins(ctx.snapshot(), &read, limits().index.read)
                .await?;
        assert!(matches!(
            current,
            graphit_data::knowledge_document_lifecycle::KnowledgeSourceOrigins::Published { .. }
        ));
        let KnowledgeIndexRead::Published { documents, .. } =
            read_native_knowledge_index(ctx.snapshot(), &read, limits().index.read).await?
        else {
            panic!("published competitor");
        };
        if case.starts_with("branch") {
            result?;
            assert_eq!(documents.len(), if case == "branch" { 2 } else { 1 });
            assert!(documents.iter().all(|document| document.kind == "page"));
            if case == "branch" {
                let retained = documents
                    .iter()
                    .find(|document| document.slug == "b")
                    .unwrap();
                assert_eq!(
                    (
                        retained.updated_at,
                        retained.stale_since,
                        retained.stale_after
                    ),
                    temporal
                );
            }
            if case == "branch-missing" {
                assert_eq!(documents[0].slug, "a");
            }
            let default = authority.admit_authenticated_read(
                &caller,
                &ctx,
                f.scope(),
                ProjectReadModule::Knowledge,
            )?;
            let KnowledgeIndexRead::Published { documents, .. } =
                read_native_knowledge_index(ctx.snapshot(), &default, limits().index.read).await?
            else {
                panic!("default preserved");
            };
            assert_eq!(documents.len(), 1);
            assert_eq!(documents[0].slug, if case == "branch" { "b" } else { "c" });
            assert_eq!(documents[0].kind, "reference");
            assert_eq!(
                (
                    documents[0].updated_at,
                    documents[0].stale_since,
                    documents[0].stale_after
                ),
                temporal
            );
        } else if case == "same-file" {
            result?;
            assert_eq!(documents.len(), 1);
            assert_eq!(documents[0].kind, "page");
            assert_eq!(
                (
                    documents[0].updated_at,
                    documents[0].stale_since,
                    documents[0].stale_after
                ),
                temporal
            );
        } else {
            assert!(
                matches!(
                    result,
                    Err(KnowledgeDesiredReconciliationError::DataImage(
                        KnowledgeIndexError::InitialTargetChanged
                    ))
                ),
                "{case}: {result:?}"
            );
            assert_eq!(documents.len(), 1);
            assert_eq!(documents[0].slug, "a");
            assert_eq!(documents[0].kind, "reference");
            assert_eq!(
                (
                    documents[0].updated_at,
                    documents[0].stale_since,
                    documents[0].stale_after
                ),
                temporal
            );
        }
    }
    Ok(())
}
