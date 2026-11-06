//! Real admissions, physical pins and Markdown FTS; no heads-as-search substitute.
use crate::knowledge_reference_fixture as support;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::file_lock::Cancellation;
use graphit_data::{knowledge_document_projection as h, knowledge_index::*, knowledge_search::*};
use std::{collections::BTreeSet, error::Error};
use support::*;

// Ordinary formatter fixture: verifies Data borrowing/range validation, not
// Knowledge's production preview policy (qualified by Knowledge tests).
fn literal_preview(
    body: &str,
    _summary: &str,
    remaining: usize,
) -> Result<KnowledgeSearchPreview, KnowledgeSearchError> {
    if body.len() > remaining {
        return Err(KnowledgeSearchError::Data(
            KnowledgeIndexError::LimitExceeded("search_output_bytes"),
        ));
    }
    Ok(KnowledgeSearchPreview {
        text: body.to_owned(),
        source: KnowledgeSearchPreviewSource::Body,
        range: 0..body.len(),
        truncated: false,
    })
}
async fn query_native_knowledge_search(
    original: &std::sync::Arc<graphit_core::file_io::SnapshotContext>,
    read: &graphit_auth::ProjectReadAdmission<'_>,
    capture: &KnowledgeSearchCapture,
    query: KnowledgeLexicalQuery<'_>,
    limits: KnowledgeSearchLimits,
) -> Result<KnowledgeSearchPage, KnowledgeSearchError> {
    graphit_data::knowledge_search::query_native_knowledge_search(
        original,
        read,
        capture,
        query,
        limits,
        literal_preview,
    )
    .await
}

fn limits() -> KnowledgeSearchLimits {
    KnowledgeSearchLimits {
        read: read_limits(),
        max_query_bytes: n(4096),
        max_output_bytes: n(65536),
    }
}
fn query<'a>(
    cursor: Option<&'a KnowledgeSearchCursor>,
    preview: bool,
) -> KnowledgeLexicalQuery<'a> {
    KnowledgeLexicalQuery {
        text: "nebula",
        top_k: 0,
        page_size: n(3),
        cursor,
        caller_binding: "PROJECT:selected/version/session",
        preview,
    }
}

#[tokio::test]
async fn lexical_all_matches_are_paged_with_literal_body_and_missing_projection()
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
    assert!(matches!(
        capture_native_knowledge_search(ctx.snapshot(), &read, read_limits()).await?,
        KnowledgeSearchRead::NoCorpus
    ));
    let slugs: Vec<_> = (0..22).map(|i| format!("doc-{i:02}")).collect();
    let body = "# Markdown nebula\r\n\r\n**ação** 日本語 🦀\n\nbody-tail nebula\n";
    let fields: Vec<_> = slugs
        .iter()
        .map(|slug| {
            let mut row = page(slug, &[]);
            row.title = Some("Nebula title");
            row.body = Some(body);
            row
        })
        .collect();
    let mut hl = hlimits();
    hl.max_documents = n(32);
    let rows = h::prepare(ctx.snapshot(), &fields, &[], hl)?;
    let published = publish_native_authored(ctx.snapshot(), &write, &rows, write_limits()).await?;
    let KnowledgeSearchRead::Published(capture) =
        capture_native_knowledge_search(ctx.snapshot(), &read, read_limits()).await?
    else {
        panic!("actual corpus absent")
    };
    assert_eq!(capture.generation(), &published.default);
    let mut cursor = None;
    let mut seen = BTreeSet::new();
    let mut pages = 0;
    loop {
        let page = query_native_knowledge_search(
            ctx.snapshot(),
            &read,
            &capture,
            query(cursor.as_ref(), false),
            limits(),
        )
        .await?;
        assert_eq!(page.total, 22);
        assert_eq!(page.state, KnowledgeSearchState::Hits);
        assert!(page.hits.len() <= 3);
        for hit in page.hits {
            assert!(seen.insert(hit.source.slug));
            assert!(hit.score.is_finite());
            assert!(hit.preview.is_none());
            assert!(hit.source.namespace.is_none());
            assert!(hit.source.witness.is_none());
        }
        pages += 1;
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 22);
    assert_eq!(pages, 8);
    let preview =
        query_native_knowledge_search(ctx.snapshot(), &read, &capture, query(None, true), limits())
            .await?;
    assert_eq!(preview.hits[0].preview.as_ref().unwrap().text, body);
    let nohit = query_native_knowledge_search(
        ctx.snapshot(),
        &read,
        &capture,
        KnowledgeLexicalQuery {
            text: "nonmatchingzzzz",
            ..query(None, false)
        },
        limits(),
    )
    .await?;
    assert_eq!(nohit.state, KnowledgeSearchState::NoHits);
    assert_eq!(nohit.total, 0);
    assert!(nohit.next_cursor.is_none());
    let cut = query_native_knowledge_search(
        ctx.snapshot(),
        &read,
        &capture,
        KnowledgeLexicalQuery {
            top_k: 2,
            ..query(None, false)
        },
        limits(),
    )
    .await?;
    assert_eq!(cut.total, 2);
    assert_eq!(cut.hits.len(), 2);
    assert!(cut.next_cursor.is_none());
    Ok(())
}

#[tokio::test]
async fn cursors_bind_query_preview_selection_and_actual_generation() -> Result<(), Box<dyn Error>>
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
    let fields: Vec<_> = ["a", "b", "c", "d"]
        .iter()
        .map(|slug| {
            let mut row = page(slug, &[]);
            row.body = Some("nebula");
            row
        })
        .collect();
    let rows = h::prepare(ctx.snapshot(), &fields, &[], hlimits())?;
    publish_native_authored(ctx.snapshot(), &write, &rows, write_limits()).await?;
    let KnowledgeSearchRead::Published(capture) =
        capture_native_knowledge_search(ctx.snapshot(), &read, read_limits()).await?
    else {
        panic!("absent")
    };
    let first = query_native_knowledge_search(
        ctx.snapshot(),
        &read,
        &capture,
        query(None, false),
        limits(),
    )
    .await?;
    let cursor = first.next_cursor.as_ref().unwrap();
    for changed in [
        KnowledgeLexicalQuery {
            text: "different",
            ..query(Some(cursor), false)
        },
        KnowledgeLexicalQuery {
            caller_binding: "other selection",
            ..query(Some(cursor), false)
        },
        query(Some(cursor), true),
    ] {
        assert!(matches!(
            query_native_knowledge_search(ctx.snapshot(), &read, &capture, changed, limits()).await,
            Err(KnowledgeSearchError::InvalidCursor)
        ));
    }
    let encoded = serde_json::to_string(cursor)?;
    let decoded = serde_json::from_str(&encoded)?;
    let next = query_native_knowledge_search(
        ctx.snapshot(),
        &read,
        &capture,
        query(Some(&decoded), false),
        limits(),
    )
    .await?;
    assert_eq!(next.hits.len(), 1);
    assert!(next.next_cursor.is_none());
    let store = graphit_data::lance_module_store::LanceModuleStore::open(
        &capture.generation().corpus_uri,
        physical_schema(),
    )
    .await?;
    use futures::TryStreamExt;
    let batches: Vec<_> = store
        .snapshot()
        .scan()
        .try_into_stream()
        .await?
        .try_collect()
        .await?;
    let changed = store
        .snapshot()
        .prepare_replace(batches)
        .await?
        .commit()
        .await?;
    assert!(changed.snapshot().version() > capture.generation().version);
    let KnowledgeSearchRead::Published(current) =
        capture_native_knowledge_search(ctx.snapshot(), &read, read_limits()).await?
    else {
        panic!("absent")
    };
    assert!(matches!(
        query_native_knowledge_search(
            ctx.snapshot(),
            &read,
            &current,
            query(Some(cursor), false),
            limits()
        )
        .await,
        Err(KnowledgeSearchError::GenerationChanged)
    ));
    // Retained captures still read their real old pin; no reopening latest.
    assert_eq!(
        query_native_knowledge_search(
            ctx.snapshot(),
            &read,
            &capture,
            query(Some(cursor), false),
            limits()
        )
        .await?
        .hits
        .len(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn limits_and_original_cancellation_fail_without_a_healthy_page() -> Result<(), Box<dyn Error>>
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
    let mut field = page("a", &[]);
    field.body = Some("nebula body-tail");
    let fields = [field];
    let rows = h::prepare(ctx.snapshot(), &fields, &[], hlimits())?;
    publish_native_authored(ctx.snapshot(), &write, &rows, write_limits()).await?;
    let KnowledgeSearchRead::Published(capture) =
        capture_native_knowledge_search(ctx.snapshot(), &read, read_limits()).await?
    else {
        panic!("absent")
    };
    for bad in [
        KnowledgeLexicalQuery {
            text: " ",
            ..query(None, false)
        },
        KnowledgeLexicalQuery {
            page_size: n(101),
            ..query(None, false)
        },
    ] {
        assert!(matches!(
            query_native_knowledge_search(ctx.snapshot(), &read, &capture, bad, limits()).await,
            Err(KnowledgeSearchError::InvalidQuery(_))
        ));
    }
    assert!(matches!(
        query_native_knowledge_search(
            ctx.snapshot(),
            &read,
            &capture,
            query(None, true),
            KnowledgeSearchLimits {
                max_output_bytes: n(1),
                ..limits()
            }
        )
        .await,
        Err(KnowledgeSearchError::Data(
            KnowledgeIndexError::LimitExceeded("search_output_bytes")
        ))
    ));
    cancel.cancel();
    let error = query_native_knowledge_search(
        ctx.snapshot(),
        &read,
        &capture,
        query(None, false),
        limits(),
    )
    .await
    .err()
    .unwrap();
    let mut causal: Option<&(dyn Error + 'static)> = Some(&error);
    let mut interrupted = false;
    while let Some(e) = causal {
        interrupted |= e
            .downcast_ref::<std::io::Error>()
            .is_some_and(|e| e.kind() == std::io::ErrorKind::Interrupted);
        causal = e.source();
    }
    assert!(interrupted);
    Ok(())
}

#[tokio::test]
async fn corrupt_unmatched_tail_and_duplicate_slug_are_rejected_at_capture()
-> Result<(), Box<dyn Error>> {
    use futures::TryStreamExt;
    use graphit_data::lance_module_store::LanceModuleStore;
    for duplicate in [false, true] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
        let auth = ProjectRecordAuthority::new(&issuer);
        let read =
            auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
        let write = auth.admit_authenticated_write(
            &caller,
            &ctx,
            f.scope(),
            ProjectWriteOperation::Knowledge,
        )?;
        let mut a = page("a", &[]);
        a.body = Some("nebula valid match");
        let mut b = page("b", &[]);
        b.body = Some("unmatched tail");
        let fields = [a, b];
        let prepared = h::prepare(ctx.snapshot(), &fields, &[], hlimits())?;
        let published =
            publish_native_authored(ctx.snapshot(), &write, &prepared, write_limits()).await?;
        let store =
            LanceModuleStore::open(&published.default.corpus_uri, physical_schema()).await?;
        let batches: Vec<_> = store
            .snapshot()
            .scan()
            .try_into_stream()
            .await?
            .try_collect()
            .await?;
        let mut rows = Vec::new();
        for batch in batches {
            for i in 0..batch.num_rows() {
                rows.push(batch.slice(i, 1));
            }
        }
        rows.sort_by(|a, b| {
            a.column(0)
                .as_any()
                .downcast_ref::<arrow_array::StringArray>()
                .unwrap()
                .value(0)
                .cmp(
                    b.column(0)
                        .as_any()
                        .downcast_ref::<arrow_array::StringArray>()
                        .unwrap()
                        .value(0),
                )
        });
        if duplicate {
            rows.push(rows[0].clone());
        } else {
            let mut columns = rows[1].columns().to_vec();
            columns[16] = std::sync::Arc::new(arrow_array::UInt8Array::from(vec![255]));
            rows[1] = arrow_array::RecordBatch::try_new(physical_schema(), columns)?;
        }
        store
            .snapshot()
            .prepare_replace(rows)
            .await?
            .commit()
            .await?;
        // Preparing a context observes only the actual pin/schema/manifest.
        // Query admission still rejects corrupt document rows below.
        let context = graphit_data::knowledge_embeddings::capture_native_knowledge_context(
            ctx.snapshot(),
            &read,
            read_limits().native,
        )
        .await?
        .expect("Source context");
        context.recheck(ctx.snapshot(), &read)?;
        let error = capture_native_knowledge_search(ctx.snapshot(), &read, read_limits())
            .await
            .err()
            .unwrap();
        if duplicate {
            assert!(matches!(
                error,
                KnowledgeSearchError::Data(KnowledgeIndexError::DuplicateSlug)
            ));
        } else {
            assert!(matches!(
                error,
                KnowledgeSearchError::Data(KnowledgeIndexError::InvalidField("document_state"))
            ));
        }
    }
    Ok(())
}

#[tokio::test]
async fn named_and_inherited_search_preserve_actual_branch_observations()
-> Result<(), Box<dyn Error>> {
    use graphit_data::lance_branch_store::BranchSnapshot;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_version(&ctx, "search-named");
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let scope = f.scope();
    let branch = scope.project.unwrap().version.lance_branch();
    let read = auth.admit_authenticated_read(&caller, &ctx, scope, ProjectReadModule::Knowledge)?;
    let write =
        auth.admit_authenticated_write(&caller, &ctx, scope, ProjectWriteOperation::Knowledge)?;
    let mut field = page("a", &[]);
    field.body = Some("nebula branch body");
    let fields = [field];
    let prepared = h::prepare(ctx.snapshot(), &fields, &[], hlimits())?;
    let published =
        publish_native_authored(ctx.snapshot(), &write, &prepared, write_limits()).await?;
    let KnowledgeSearchRead::Published(capture) =
        capture_native_knowledge_search(ctx.snapshot(), &read, read_limits()).await?
    else {
        panic!()
    };
    assert_eq!(
        capture.generation().physical_branch.as_deref(),
        Some(branch.as_str())
    );
    assert!(capture.generation().inherited_target.is_none());
    let dataset = std::path::Path::new(&published.default.corpus_uri);
    assert!(dataset.is_absolute());
    let forbidden = dataset.join("must-not-stage.md");
    assert!(
        matches!(capture.validate_export_destination(ctx.snapshot(), &read, &forbidden),
        Err(KnowledgeSearchError::Data(KnowledgeIndexError::Request(error))) if error.kind() == std::io::ErrorKind::InvalidInput)
    );
    assert!(!forbidden.exists());
    let actual = query_native_knowledge_search(
        ctx.snapshot(),
        &read,
        &capture,
        query(None, false),
        limits(),
    )
    .await?;
    assert_eq!(actual.hits.len(), 1);
    let pin = BranchSnapshot::capture_observed(&published.default.corpus_uri, Some(&branch), None)
        .await?;
    pin.retire_named().await?;
    let KnowledgeSearchRead::Published(inherited) =
        capture_native_knowledge_search(ctx.snapshot(), &read, read_limits()).await?
    else {
        panic!()
    };
    assert!(inherited.generation().physical_branch.is_none());
    assert!(
        inherited
            .validate_export_destination(ctx.snapshot(), &read, &forbidden)
            .is_err()
    );
    assert_eq!(
        inherited.generation().inherited_target.as_deref(),
        Some(branch.as_str())
    );
    let actual = query_native_knowledge_search(
        ctx.snapshot(),
        &read,
        &inherited,
        query(None, false),
        limits(),
    )
    .await?;
    assert_eq!(actual.hits.len(), 1);
    assert_eq!(&actual.generation, inherited.generation());
    Ok(())
}

#[tokio::test]
async fn standalone_terminal_guard_cancellation_never_delivers_a_page() -> Result<(), Box<dyn Error>>
{
    use graphit_data::lance_module_store::LanceModuleStore;
    use std::sync::atomic::{AtomicUsize, Ordering};
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
    let prepared = h::prepare(ctx.snapshot(), &[], &[], hlimits())?;
    let published =
        publish_native_authored(ctx.snapshot(), &write, &prepared, write_limits()).await?;
    let store = LanceModuleStore::open(&published.default.corpus_uri, physical_schema()).await?;
    let snapshot = store.snapshot();
    let calls = AtomicUsize::new(0);
    let warm = search_knowledge_snapshot_guarded(
        ctx.snapshot(),
        &snapshot,
        &published.default,
        query(None, false),
        limits(),
        || {
            read.recheck()?;
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
        literal_preview,
    )
    .await?;
    assert_eq!(warm.state, KnowledgeSearchState::PublishedEmpty);
    let terminal = calls.swap(0, Ordering::SeqCst);
    let result = search_knowledge_snapshot_guarded(
        ctx.snapshot(),
        &snapshot,
        &published.default,
        query(None, false),
        limits(),
        || {
            read.recheck()?;
            if calls.fetch_add(1, Ordering::SeqCst) + 1 == terminal {
                cancel.cancel();
            }
            Ok(())
        },
        literal_preview,
    )
    .await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        terminal,
        "same empty SDK query reaches the terminal callback"
    );
    assert!(
        matches!(result,Err(KnowledgeSearchError::Data(KnowledgeIndexError::Request(error))) if error.kind()==std::io::ErrorKind::Interrupted)
    );
    Ok(())
}

#[tokio::test]
async fn source_only_cursor_keeps_generation_batches_removed_and_terminal_guards()
-> Result<(), Box<dyn Error>> {
    use graphit_data::knowledge_references::{
        KnowledgeDocumentState, KnowledgeReferenceFields, KnowledgeReferenceNamespace,
        KnowledgeReferenceScope, prepare_knowledge_references,
    };
    for all_removed in [false, true] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
        let auth = ProjectRecordAuthority::new(&issuer);
        let read =
            auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
        let write = auth.admit_authenticated_write(
            &caller,
            &ctx,
            f.scope(),
            ProjectWriteOperation::Knowledge,
        )?;
        let literal = "# Source\r\n\r\n**ação** 日本語 🦀\nbody-tail\n";
        let mut pages = [page("a", &[]), page("b", &[]), page("c", &[])];
        for page in &mut pages {
            page.body = Some(literal);
        }
        let prepared = h::prepare(ctx.snapshot(), &pages, &[], hlimits())?;
        let graphit_core::artifact_store::StoreOwner::Project { project_id } = f.project.owner()
        else {
            panic!()
        };
        let namespace = KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project(*project_id),
            context: None,
        };
        let fields: Vec<_> = ["a", "b", "c"]
            .iter()
            .map(|slug| KnowledgeReferenceFields {
                slug,
                state: if all_removed || *slug == "b" {
                    KnowledgeDocumentState::Removed
                } else {
                    KnowledgeDocumentState::Active
                },
                namespace,
                authored: &[],
            })
            .collect();
        let references = prepare_knowledge_references(
            ctx.snapshot(),
            &prepared,
            &fields,
            write_limits().content,
        )?;
        let published = publish_native_authored_with_references(
            ctx.snapshot(),
            &write,
            &prepared,
            &references,
            write_limits(),
        )
        .await?;
        let mut batch_limits = read_limits();
        batch_limits.batch_rows = std::num::NonZeroU32::new(1).unwrap();
        let KnowledgeSearchRead::Published(capture) =
            capture_native_knowledge_search(ctx.snapshot(), &read, batch_limits).await?
        else {
            panic!()
        };
        let KnowledgeSearchRead::Published(abandoned) =
            capture_native_knowledge_search(ctx.snapshot(), &read, batch_limits).await?
        else {
            panic!()
        };
        assert_eq!(capture.generation(), &published.default);
        let exports = tempfile::tempdir()?;
        let outside = exports.path().join("outside.md");
        capture.validate_export_destination(ctx.snapshot(), &read, &outside)?;
        assert!(!outside.exists());
        let dataset = std::path::Path::new(&published.default.corpus_uri);
        assert!(dataset.is_absolute());
        assert!(
            matches!(capture.validate_export_destination(ctx.snapshot(), &read, &dataset.join("inside.md")),
            Err(KnowledgeSearchError::Data(KnowledgeIndexError::Request(error))) if error.kind() == std::io::ErrorKind::InvalidInput)
        );
        #[cfg(unix)]
        {
            let alias = exports.path().join("alias");
            std::os::unix::fs::symlink(dataset, &alias)?;
            assert!(
                capture
                    .validate_export_destination(ctx.snapshot(), &read, &alias.join("inside.md"))
                    .is_err()
            );
            let final_link = exports.path().join("link.md");
            std::os::unix::fs::symlink(dataset, &final_link)?;
            assert!(
                capture
                    .validate_export_destination(ctx.snapshot(), &read, &final_link)
                    .is_err()
            );
        }
        std::fs::write(&outside, b"occupied file")?;
        capture.validate_export_destination(ctx.snapshot(), &read, &outside)?;
        assert_eq!(std::fs::read(&outside)?, b"occupied file");
        let occupied_directory = exports.path().join("occupied");
        std::fs::create_dir(&occupied_directory)?;
        std::fs::write(occupied_directory.join("sentinel"), b"unchanged")?;
        capture.validate_export_destination(ctx.snapshot(), &read, &occupied_directory)?;
        assert_eq!(
            std::fs::read(occupied_directory.join("sentinel"))?,
            b"unchanged"
        );
        assert!(
            matches!(capture.validate_export_destination(ctx.snapshot(), &read, &exports.path().join("absent/out.md")),
            Err(KnowledgeSearchError::Data(KnowledgeIndexError::Request(error))) if error.kind() == std::io::ErrorKind::NotFound)
        );
        assert!(
            !f.project
                .runtime_root()
                .join("knowledge/embeddings.lance")
                .exists()
        );
        let foreign_cancel = Cancellation::default();
        let foreign = request(&foreign_cancel);
        assert!(
            capture
                .validate_export_destination(foreign.snapshot(), &read, &outside)
                .is_err()
        );
        assert!(
            capture
                .next_documents(foreign.snapshot(), &read)
                .await
                .is_err()
        );
        let store = graphit_data::lance_module_store::LanceModuleStore::open(
            &published.default.corpus_uri,
            physical_schema(),
        )
        .await?;
        let changed = store
            .snapshot()
            .prepare_replace(vec![arrow_array::RecordBatch::new_empty(physical_schema())])
            .await?
            .commit()
            .await?;
        assert!(changed.snapshot().version() > capture.generation().version);
        let mut seen = BTreeSet::new();
        let mut batches = 0;
        let mut empty_batches = 0;
        while let Some(documents) = capture.next_documents(ctx.snapshot(), &read).await? {
            batches += 1;
            assert!(documents.len() <= 1);
            if documents.is_empty() {
                empty_batches += 1;
            }
            for document in documents {
                assert_eq!(document.body, literal);
                assert!(seen.insert(document.slug));
            }
        }
        assert_eq!(batches, 3);
        assert_eq!(empty_batches, if all_removed { 3 } else { 1 });
        assert_eq!(
            seen.into_iter().collect::<Vec<_>>(),
            if all_removed {
                vec![]
            } else {
                vec!["a".to_owned(), "c".to_owned()]
            }
        );
        assert!(
            capture
                .next_documents(ctx.snapshot(), &read)
                .await?
                .is_none()
        );
        let mut pending = Box::pin(abandoned.next_documents(ctx.snapshot(), &read));
        assert!(matches!(
            futures::poll!(&mut pending),
            std::task::Poll::Pending
        ));
        drop(pending);
        assert!(matches!(
            abandoned.next_documents(ctx.snapshot(), &read).await,
            Err(KnowledgeSearchError::Capture(
                graphit_data::knowledge_embeddings::KnowledgeEmbeddingError::InvalidInput(
                    "invalid document cursor"
                )
            ))
        ));
        assert!(
            !f.project
                .runtime_root()
                .join("knowledge/embeddings.lance")
                .exists()
        );
        cancel.cancel();
        assert!(capture.next_documents(ctx.snapshot(), &read).await.is_err());
        assert!(
            capture
                .validate_export_destination(ctx.snapshot(), &read, &outside)
                .is_err()
        );
    }
    Ok(())
}

#[tokio::test]
async fn knowledge_context_retains_source_manifest_without_rows_and_preserves_request_admission()
-> Result<(), Box<dyn Error>> {
    use graphit_data::knowledge_embeddings::{
        KnowledgeEmbeddingError, capture_native_knowledge_context,
    };
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_version(&ctx, "feature/live-context");
    let issuer = f.issuer_permissions(
        &ctx,
        &[
            graphit_auth::Permission::Read(graphit_core::module_settings::Module::Knowledge),
            graphit_auth::Permission::MutateModule(
                graphit_core::module_settings::Module::Knowledge,
            ),
            graphit_auth::Permission::Read(graphit_core::module_settings::Module::Task),
        ],
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority.admit_authenticated_read(
        &caller,
        &ctx,
        f.scope(),
        ProjectReadModule::Knowledge,
    )?;
    assert!(
        capture_native_knowledge_context(ctx.snapshot(), &read, read_limits().native)
            .await?
            .is_none()
    );
    let write = authority.admit_authenticated_write(
        &caller,
        &ctx,
        f.scope(),
        ProjectWriteOperation::Knowledge,
    )?;
    let rows = h::prepare(ctx.snapshot(), &[page("context", &[])], &[], hlimits())?;
    let first = publish_native_authored(ctx.snapshot(), &write, &rows, write_limits()).await?;
    let captured = capture_native_knowledge_context(ctx.snapshot(), &read, read_limits().native)
        .await?
        .expect("real Source pin");
    assert_eq!(
        captured.generation(),
        first.named.as_ref().expect("named branch pin")
    );
    assert_eq!(
        captured.generation().physical_branch.as_deref(),
        Some(f.scope().project.unwrap().version.lance_branch().as_str())
    );
    captured.recheck(ctx.snapshot(), &read)?;
    let selected = captured
        .page(ctx.snapshot(), &read, "context", read_limits())
        .await?;
    let graphit_data::knowledge_snapshot::KnowledgeSnapshotPage::Found { document: page, .. } =
        selected
    else {
        panic!("actual context page absent");
    };
    assert_eq!(page.slug, "context");
    let references = captured
        .references(
            ctx.snapshot(),
            &read,
            graphit_data::knowledge_reference_query::KnowledgeReferenceQuery {
                outgoing: Some("context"),
                incoming: None,
                page_size: n(4),
                outgoing_cursor: None,
                incoming_cursor: None,
            },
            read_limits(),
        )
        .await?;
    assert_eq!(
        references
            .focused_source
            .as_ref()
            .map(|source| source.slug.as_str()),
        Some("context")
    );
    let foreign_cancel = Cancellation::default();
    let foreign = request(&foreign_cancel);
    assert!(matches!(
        captured.recheck(foreign.snapshot(), &read),
        Err(KnowledgeEmbeddingError::Index(
            KnowledgeIndexError::DifferentRequest
        ))
    ));
    let wrong =
        authority.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)?;
    assert!(matches!(
        captured.recheck(ctx.snapshot(), &wrong),
        Err(KnowledgeEmbeddingError::Index(
            KnowledgeIndexError::WrongAdmission
        ))
    ));
    captured.recheck(ctx.snapshot(), &read)?;
    cancel.cancel();
    assert!(captured.recheck(ctx.snapshot(), &read).is_err());
    Ok(())
}
