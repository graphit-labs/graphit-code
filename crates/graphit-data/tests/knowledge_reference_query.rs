//! Actual Native admissions, physical schema18 publication and SDK query pins.
#[path = "support/knowledge_reference_fixture.rs"]
mod support;
use futures::TryStreamExt;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::{file_lock::Cancellation, project_identity::ProjectId};
use graphit_data::{
    knowledge_document_projection as h,
    knowledge_index::{
        KnowledgeGeneration, KnowledgeIndexError, physical_schema, publish_native_authored,
        publish_native_authored_with_references,
    },
    knowledge_reference_query::*,
    knowledge_references::*,
    lance_module_store::LanceModuleStore,
};
use std::{error::Error, sync::Arc};
use support::*;

fn project() -> ProjectId {
    "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap()
}
fn limits() -> graphit_data::knowledge_index::KnowledgeIndexReadLimits {
    read_limits()
}
fn query<'a>(
    outgoing: Option<&'a str>,
    incoming: Option<KnowledgeReferenceTarget<'a>>,
) -> KnowledgeReferenceQuery<'a> {
    KnowledgeReferenceQuery {
        outgoing,
        incoming,
        page_size: n(100),
        outgoing_cursor: None,
        incoming_cursor: None,
    }
}
fn memory_target<'a>(owner: &'a str) -> KnowledgeReferenceTarget<'a> {
    KnowledgeReferenceTarget {
        target_kind: KnowledgeReferenceTargetKind::Memory,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::User(owner),
            context: None,
        },
        id: "same-id",
    }
}
fn authored<'a>(owner: &'a str) -> KnowledgeAuthoredReference<'a> {
    KnowledgeAuthoredReference {
        target_kind: KnowledgeReferenceTargetKind::Memory,
        scope: KnowledgeReferenceScope::User(owner),
        id: "same-id",
        context: None,
        relation: "supports",
        field: Some(""),
    }
}
fn uri(f: &Fixture) -> String {
    url::Url::from_file_path(f.project.runtime_root().join("knowledge/index.lance"))
        .unwrap()
        .to_string()
}
fn published(
    read: KnowledgeReferencesRead,
) -> (KnowledgeGeneration, KnowledgeReferenceSnapshotRead) {
    match read {
        KnowledgeReferencesRead::Published { generation, read } => (generation, *read),
        KnowledgeReferencesRead::NoCorpus => panic!("publication expected"),
    }
}

#[tokio::test]
async fn native_raw_provenance_and_literal_targets_paginate_without_source_loss()
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
    let entries = [authored("owner β"), authored("owner β")];
    let docs = [
        page("b", &[]),
        page("a", &["b", "", "a", "b", "ghost"]),
        page("removed", &[]),
    ];
    let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits())?;
    let ns = KnowledgeReferenceNamespace {
        scope: KnowledgeReferenceScope::Project(project()),
        context: None,
    };
    let fields = [
        KnowledgeReferenceFields {
            slug: "b",
            state: KnowledgeDocumentState::Active,
            namespace: ns,
            authored: &[],
        },
        KnowledgeReferenceFields {
            slug: "a",
            state: KnowledgeDocumentState::Active,
            namespace: ns,
            authored: &entries,
        },
        KnowledgeReferenceFields {
            slug: "removed",
            state: KnowledgeDocumentState::Removed,
            namespace: ns,
            authored: &[],
        },
    ];
    let refs = prepare_knowledge_references(ctx.snapshot(), &rows, &fields, read_limits().content)?;
    publish_native_authored_with_references(ctx.snapshot(), &write, &rows, &refs, write_limits())
        .await?;
    // Reference queries keep their C8 projection through the real nullable
    // evolution, without inventing any vector or requiring a Models service.
    let identity = graphit_data::knowledge_embeddings::KnowledgeEmbeddingIdentity::new(
        [7; 32],
        std::num::NonZeroU32::new(2).unwrap(),
        "knowledge-document-json-v1".into(),
    )?;
    let vector_input =
        graphit_data::knowledge_embeddings::native_knowledge_embedding_schema(&identity);
    let extension = Arc::new(arrow_schema::Schema::new(
        vector_input.fields()[1..]
            .iter()
            .map(|field| Arc::new(field.as_ref().clone().with_nullable(true)))
            .collect::<Vec<_>>(),
    ));
    let pin =
        graphit_data::lance_branch_store::BranchSnapshot::capture_observed(&uri(&f), None, None)
            .await?;
    let extended = pin.add_nullable_columns(extension, ctx.snapshot()).await?;
    assert_eq!(extended.schema().fields().len(), 23);
    let (generation, all) = published(
        read_native_knowledge_reference_query(
            ctx.snapshot(),
            &read,
            query(Some("a"), Some(memory_target("owner β"))),
            limits(),
        )
        .await?,
    );
    let out = all.outgoing.unwrap();
    assert_eq!(all.focused_source.as_ref(), Some(&out.results[0].source));
    let (_, zero_edges) = published(
        read_native_knowledge_reference_query(
            ctx.snapshot(),
            &read,
            query(Some("b"), None),
            limits(),
        )
        .await?,
    );
    let focus = zero_edges.focused_source.as_ref().unwrap();
    assert_eq!(focus.slug, "b");
    assert_eq!(
        focus.namespace.as_ref().unwrap().scope,
        KnowledgeReferenceOwnedScope::Project(project())
    );
    assert_eq!(focus.namespace.as_ref().unwrap().context, None);
    assert!(focus.witness.is_some());
    let zero_page = zero_edges.outgoing.unwrap();
    assert!(zero_page.complete);
    assert_eq!(zero_page.total, 0);
    assert!(zero_page.results.is_empty());
    assert!(zero_page.next_cursor.is_none());
    assert_eq!(out.total, 6);
    assert_eq!(out.results.len(), 6);
    assert!(out.complete);
    assert_eq!(all.incoming.unwrap().total, 2);
    assert!(all.coverage.knowledge_source_complete);
    assert!(all.warnings.is_empty());
    assert_eq!(
        out.results
            .iter()
            .map(|e| (e.provenance, e.ordinal))
            .collect::<Vec<_>>(),
        [
            (KnowledgeReferenceProvenance::Authored, 0),
            (KnowledgeReferenceProvenance::Authored, 1),
            (KnowledgeReferenceProvenance::WikiLink, 0),
            (KnowledgeReferenceProvenance::WikiLink, 2),
            (KnowledgeReferenceProvenance::WikiLink, 3),
            (KnowledgeReferenceProvenance::WikiLink, 4)
        ]
    );
    assert!(
        matches!(&out.results[0].payload,KnowledgeReferencePayload::Authored{relation,field,..} if relation=="supports" && field.as_deref()==Some(""))
    );
    assert!(
        matches!(&out.results[2].target_observation,KnowledgeReferenceObservation::KnowledgeHead{slug,title} if slug=="b" && title.is_empty())
    );
    assert_eq!(
        out.results[5].target_observation,
        KnowledgeReferenceObservation::Unresolved
    );
    assert!(out.results.iter().all(|e| {
        e.source
            .namespace
            .as_ref()
            .is_some_and(|ns| ns.scope == KnowledgeReferenceOwnedScope::Project(project()))
            && e.source.witness.is_some()
    }));
    let (_, foreign) = published(
        read_native_knowledge_reference_query(
            ctx.snapshot(),
            &read,
            query(None, Some(memory_target("another owner"))),
            limits(),
        )
        .await?,
    );
    assert!(foreign.focused_source.is_none());
    assert_eq!(foreign.incoming.unwrap().total, 0);
    let mut cursor = None;
    let mut collected = vec![];
    loop {
        let mut q = query(Some("a"), None);
        q.page_size = n(1);
        q.outgoing_cursor = cursor.as_ref();
        let (g, result) = published(
            read_native_knowledge_reference_query(ctx.snapshot(), &read, q, limits()).await?,
        );
        assert_eq!(g, generation);
        let page = result.outgoing.unwrap();
        assert_eq!(page.total, 6);
        assert!(page.complete);
        collected.extend(page.results);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(collected, out.results);
    let mut first = query(Some("a"), None);
    first.page_size = n(1);
    let (_, result) = published(
        read_native_knowledge_reference_query(ctx.snapshot(), &read, first, limits()).await?,
    );
    let continuation = result.outgoing.unwrap().next_cursor.unwrap();
    first.outgoing_cursor = Some(&continuation);
    let mut fresh = limits();
    fresh.native = graphit_core::native_document_file::NativeDocumentFileLimits::new(
        131072, 65536, 512, 131072,
    )?;
    let (_, next) = published(
        read_native_knowledge_reference_query(ctx.snapshot(), &read, first, fresh).await?,
    );
    assert_eq!(
        next.outgoing.unwrap().results.len(),
        1,
        "fresh physical caps are not semantic cursor bindings"
    );
    let mut physical_too_small = fresh;
    physical_too_small.native =
        graphit_core::native_document_file::NativeDocumentFileLimits::new(131072, 1, 512, 131072)?;
    let error =
        read_native_knowledge_reference_query(ctx.snapshot(), &read, first, physical_too_small)
            .await
            .unwrap_err();
    assert!(matches!(&error, KnowledgeReferenceQueryError::Data(_)));
    assert!(error.source().is_some());
    let corpus = f.project.runtime_root().join("knowledge/index.lance");
    std::fs::rename(&corpus, corpus.with_file_name("held-away.lance"))?;
    assert!(matches!(
        read_native_knowledge_reference_query(ctx.snapshot(), &read, first, fresh).await,
        Err(KnowledgeReferenceQueryError::InvalidCursor)
    ));
    Ok(())
}

#[tokio::test]
async fn projection_absence_empty_and_missing_focus_are_distinct() -> Result<(), Box<dyn Error>> {
    for empty in [false, true] {
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
        assert_eq!(
            read_native_knowledge_reference_query(
                ctx.snapshot(),
                &read,
                query(None, Some(memory_target("owner"))),
                limits()
            )
            .await?,
            KnowledgeReferencesRead::NoCorpus
        );
        let docs = if empty {
            vec![]
        } else {
            vec![page("ordinary", &["other"])]
        };
        let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits())?;
        publish_native_authored(ctx.snapshot(), &write, &rows, write_limits()).await?;
        let (_, result) = published(
            read_native_knowledge_reference_query(
                ctx.snapshot(),
                &read,
                query(None, Some(memory_target("owner"))),
                limits(),
            )
            .await?,
        );
        assert!(result.focused_source.is_none());
        assert_eq!(result.coverage.knowledge_source_complete, empty);
        assert_eq!(result.incoming.unwrap().complete, empty);
        assert_eq!(result.warnings.len(), usize::from(!empty));
        if !empty {
            let KnowledgeReferencesWarning::MissingCurrentProjection { source } =
                &result.warnings[0];
            assert_eq!(source.namespace, None);
            assert_eq!(source.witness, None);
            let exact = limits();
            let (_, bounded) = published(
                read_native_knowledge_reference_query(
                    ctx.snapshot(),
                    &read,
                    query(Some("ordinary"), None),
                    exact,
                )
                .await?,
            );
            let focus = bounded.focused_source.as_ref().unwrap();
            assert_eq!(focus, source);
            assert_eq!(focus.slug, "ordinary");
            assert_eq!(focus.namespace, None);
            assert_eq!(focus.witness, None);
            assert!(!bounded.outgoing.as_ref().unwrap().complete);
            assert_eq!(bounded.warnings.len(), 1);
        }
        assert!(matches!(
            read_native_knowledge_reference_query(
                ctx.snapshot(),
                &read,
                query(Some("missing"), None),
                limits()
            )
            .await,
            Err(KnowledgeReferenceQueryError::UnknownSource)
        ));
    }
    Ok(())
}

#[tokio::test]
async fn real_pin_uri_generation_input_and_stale_continuations_are_checked()
-> Result<(), Box<dyn Error>> {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    let docs = [page("a", &["a", "a", "a"])];
    let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits())?;
    let fields = [KnowledgeReferenceFields {
        slug: "a",
        state: KnowledgeDocumentState::Active,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project(project()),
            context: None,
        },
        authored: &[],
    }];
    let refs = prepare_knowledge_references(ctx.snapshot(), &rows, &fields, read_limits().content)?;
    publish_native_authored_with_references(ctx.snapshot(), &write, &rows, &refs, write_limits())
        .await?;
    let store = LanceModuleStore::open(&uri(&f), physical_schema()).await?;
    let pin = store.snapshot();
    let generation = KnowledgeGeneration {
        corpus_uri: uri(&f),
        version: pin.version(),
        physical_branch: None,
        inherited_target: None,
    };
    assert!(generation.corpus_uri.len() > 64);
    let mut q = query(Some("a"), None);
    q.page_size = n(1);
    let result = read_knowledge_reference_snapshot_guarded(
        ctx.snapshot(),
        &pin,
        &generation,
        q,
        limits(),
        || Ok(()),
    )
    .await?;
    let cursor = result.outgoing.unwrap().next_cursor.unwrap();
    q.outgoing_cursor = Some(&cursor);
    let other = tempfile::tempdir()?;
    let other_uri = url::Url::from_file_path(other.path().join("different.lance"))
        .unwrap()
        .to_string();
    let other_store = LanceModuleStore::create(&other_uri, physical_schema(), vec![]).await?;
    assert_eq!(other_store.snapshot().version(), pin.version());
    assert!(matches!(
        read_knowledge_reference_snapshot_guarded(
            ctx.snapshot(),
            &other_store.snapshot(),
            &generation,
            q,
            limits(),
            || Ok(())
        )
        .await,
        Err(KnowledgeReferenceQueryError::InvalidCursor)
    ));
    let wrong_uri = url::Url::from_file_path(other.path().join("wrong-schema.lance"))
        .unwrap()
        .to_string();
    let wrong_schema = Arc::new(arrow_schema::Schema::new(
        physical_schema()
            .fields()
            .iter()
            .take(16)
            .cloned()
            .collect::<Vec<_>>(),
    ));
    let wrong_store = LanceModuleStore::create(&wrong_uri, wrong_schema, vec![]).await?;
    let wrong_generation = KnowledgeGeneration {
        corpus_uri: wrong_uri,
        version: wrong_store.snapshot().version(),
        physical_branch: None,
        inherited_target: None,
    };
    assert!(matches!(
        read_knowledge_reference_snapshot_guarded(
            ctx.snapshot(),
            &wrong_store.snapshot(),
            &wrong_generation,
            query(Some("a"), None),
            limits(),
            || Ok(())
        )
        .await,
        Err(KnowledgeReferenceQueryError::Data(
            KnowledgeIndexError::InvalidField("schema")
        ))
    ));
    let mut huge = generation.clone();
    huge.physical_branch = Some("β".repeat(40000));
    assert!(matches!(
        read_knowledge_reference_snapshot_guarded(ctx.snapshot(), &pin, &huge, q, limits(), || Ok(
            ()
        ))
        .await,
        Err(KnowledgeReferenceQueryError::InvalidCursor)
    ));
    let mut encoded = serde_json::to_value(&cursor)?;
    encoded["extra"] = serde_json::json!(true);
    assert!(serde_json::from_value::<KnowledgeReferencesCursor>(encoded).is_err());
    let mut encoded = serde_json::to_value(&cursor)?;
    encoded["binding"]["uri"] = serde_json::json!("β".repeat(40000));
    let oversized: KnowledgeReferencesCursor = serde_json::from_value(encoded)?;
    let mut oversized_query = q;
    oversized_query.outgoing_cursor = Some(&oversized);
    assert!(matches!(
        read_knowledge_reference_snapshot_guarded(
            ctx.snapshot(),
            &pin,
            &generation,
            oversized_query,
            limits(),
            || Ok(())
        )
        .await,
        Err(KnowledgeReferenceQueryError::InvalidCursor)
    ));
    let mut changed_focus = q;
    changed_focus.incoming = Some(memory_target("owner"));
    assert!(matches!(
        read_knowledge_reference_snapshot_guarded(
            ctx.snapshot(),
            &pin,
            &generation,
            changed_focus,
            limits(),
            || Ok(())
        )
        .await,
        Err(KnowledgeReferenceQueryError::InvalidCursor)
    ));
    let mut encoded = serde_json::to_value(&cursor)?;
    encoded["offset"] = serde_json::json!(usize::MAX);
    let beyond: KnowledgeReferencesCursor = serde_json::from_value(encoded)?;
    let mut beyond_query = q;
    beyond_query.outgoing_cursor = Some(&beyond);
    assert!(matches!(
        read_knowledge_reference_snapshot_guarded(
            ctx.snapshot(),
            &pin,
            &generation,
            beyond_query,
            limits(),
            || Ok(())
        )
        .await,
        Err(KnowledgeReferenceQueryError::InvalidCursor)
    ));
    let mut both = query(
        Some("a"),
        Some(KnowledgeReferenceTarget {
            target_kind: KnowledgeReferenceTargetKind::Knowledge,
            namespace: KnowledgeReferenceNamespace {
                scope: KnowledgeReferenceScope::Project(project()),
                context: None,
            },
            id: "a",
        }),
    );
    both.page_size = n(1);
    let result = read_knowledge_reference_snapshot_guarded(
        ctx.snapshot(),
        &pin,
        &generation,
        both,
        limits(),
        || Ok(()),
    )
    .await?;
    let outgoing = result.outgoing.unwrap().next_cursor.unwrap();
    assert!(result.incoming.unwrap().next_cursor.is_some());
    both.incoming_cursor = Some(&outgoing);
    assert!(matches!(
        read_knowledge_reference_snapshot_guarded(
            ctx.snapshot(),
            &pin,
            &generation,
            both,
            limits(),
            || Ok(())
        )
        .await,
        Err(KnowledgeReferenceQueryError::InvalidCursor)
    ));
    let mut changed = limits();
    changed.batch_rows = std::num::NonZeroU32::new(2).unwrap();
    let continued = read_knowledge_reference_snapshot_guarded(
        ctx.snapshot(),
        &pin,
        &generation,
        q,
        changed,
        || Ok(()),
    )
    .await?;
    assert!(continued.outgoing.is_some());
    let new = pin.prepare_replace(vec![]).await?.commit().await?;
    let new_generation = KnowledgeGeneration {
        version: new.snapshot().version(),
        ..generation.clone()
    };
    assert!(matches!(
        read_knowledge_reference_snapshot_guarded(
            ctx.snapshot(),
            &new.snapshot(),
            &new_generation,
            q,
            limits(),
            || Ok(())
        )
        .await,
        Err(KnowledgeReferenceQueryError::InvalidCursor)
    ));
    let old = read_knowledge_reference_snapshot_guarded(
        ctx.snapshot(),
        &pin,
        &generation,
        q,
        limits(),
        || Ok(()),
    )
    .await?;
    assert_eq!(old.outgoing.unwrap().results.len(), 1);
    Ok(())
}

#[tokio::test]
async fn late_native_corruption_and_warning_coverage_never_return_a_prefix()
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
    let docs = [page("a", &["a"]), page("z", &[])];
    let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits())?;
    let ns = KnowledgeReferenceNamespace {
        scope: KnowledgeReferenceScope::Project(project()),
        context: None,
    };
    let fields = [
        KnowledgeReferenceFields {
            slug: "a",
            state: KnowledgeDocumentState::Active,
            namespace: ns,
            authored: &[],
        },
        KnowledgeReferenceFields {
            slug: "z",
            state: KnowledgeDocumentState::Active,
            namespace: ns,
            authored: &[],
        },
    ];
    let refs = prepare_knowledge_references(ctx.snapshot(), &rows, &fields, read_limits().content)?;
    publish_native_authored_with_references(ctx.snapshot(), &write, &rows, &refs, write_limits())
        .await?;
    let (_, complete) = published(
        read_native_knowledge_reference_query(
            ctx.snapshot(),
            &read,
            query(Some("a"), None),
            limits(),
        )
        .await?,
    );
    assert!(complete.outgoing.is_some());
    let store = LanceModuleStore::open(&uri(&f), physical_schema()).await?;
    let old = store.snapshot();
    let scan = old.scan();
    let mut stream = scan.try_into_stream().await?;
    let mut batches = vec![];
    while let Some(batch) = stream.try_next().await? {
        batches.push(batch);
    }
    drop(stream);
    drop(scan);
    let batch = &batches[0];
    let slugs = batch
        .column(0)
        .as_any()
        .downcast_ref::<arrow_array::StringArray>()
        .unwrap();
    let row = (0..batch.num_rows())
        .find(|i| slugs.value(*i) == "z")
        .unwrap();
    let late = batch.slice(row, 1);
    let first = batch.slice(
        (0..batch.num_rows())
            .find(|i| slugs.value(*i) == "a")
            .unwrap(),
        1,
    );
    let mut current = old;
    for field in [
        "document_state",
        "reference_witness",
        "removed_references",
        "duplicate_slug",
    ] {
        let mut cols = late.columns().to_vec();
        let changed = match field {
            "document_state" => {
                cols[16] = Arc::new(arrow_array::UInt8Array::from(vec![9]));
                arrow_array::RecordBatch::try_new(late.schema(), cols)?
            }
            "reference_witness" => {
                let title = late.schema().index_of("title")?;
                cols[title] = Arc::new(arrow_array::StringArray::from(vec!["changed late title"]));
                arrow_array::RecordBatch::try_new(late.schema(), cols)?
            }
            "removed_references" => {
                cols[16] = Arc::new(arrow_array::UInt8Array::from(vec![2]));
                let wiki = late.schema().index_of("resolved_wiki_targets")?;
                cols[wiki] = first.column(wiki).clone();
                arrow_array::RecordBatch::try_new(late.schema(), cols)?
            }
            _ => first.clone(),
        };
        let committed = current
            .prepare_replace(vec![first.clone(), changed])
            .await?
            .commit()
            .await?;
        current = committed.snapshot();
        let error = read_native_knowledge_reference_query(
            ctx.snapshot(),
            &read,
            query(Some("a"), None),
            limits(),
        )
        .await
        .unwrap_err();
        if field == "duplicate_slug" {
            assert!(matches!(
                error,
                KnowledgeReferenceQueryError::Data(KnowledgeIndexError::DuplicateSlug)
            ));
        } else {
            assert!(
                matches!(error,KnowledgeReferenceQueryError::Data(KnowledgeIndexError::InvalidField(name)) if name==field)
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn raw_wiki_identifier_boundary_counts_utf8_bytes() -> Result<(), Box<dyn Error>> {
    for extra in [false, true] {
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
        let target = if extra {
            format!("{}a", "β".repeat(32))
        } else {
            "β".repeat(32)
        };
        let targets = [target.as_str(), ""];
        let docs = [page("a", &targets)];
        let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits())?;
        let fields = [KnowledgeReferenceFields {
            slug: "a",
            state: KnowledgeDocumentState::Active,
            namespace: KnowledgeReferenceNamespace {
                scope: KnowledgeReferenceScope::Project(project()),
                context: None,
            },
            authored: &[],
        }];
        let refs =
            prepare_knowledge_references(ctx.snapshot(), &rows, &fields, read_limits().content)?;
        publish_native_authored_with_references(
            ctx.snapshot(),
            &write,
            &rows,
            &refs,
            write_limits(),
        )
        .await?;
        let mut cap = limits();
        cap.content.max_identifier_bytes = n(64);
        let result = read_native_knowledge_reference_query(
            ctx.snapshot(),
            &read,
            query(Some("a"), None),
            cap,
        )
        .await;
        if extra {
            assert!(matches!(
                result,
                Err(KnowledgeReferenceQueryError::Data(
                    KnowledgeIndexError::LimitExceeded("identifier_bytes")
                ))
            ));
        } else {
            let (_, result) = published(result?);
            let out = result.outgoing.unwrap();
            assert_eq!(out.total, 1);
            assert!(
                matches!(&out.results[0].payload,KnowledgeReferencePayload::WikiLink{target:t} if t.id==target)
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn both_scans_and_terminal_delivery_preserve_guard_failure_and_cancellation()
-> Result<(), Box<dyn Error>> {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    let docs = [page("a", &["a", "a"])];
    let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits())?;
    let fields = [KnowledgeReferenceFields {
        slug: "a",
        state: KnowledgeDocumentState::Active,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project(project()),
            context: None,
        },
        authored: &[],
    }];
    let refs = prepare_knowledge_references(ctx.snapshot(), &rows, &fields, read_limits().content)?;
    publish_native_authored_with_references(ctx.snapshot(), &write, &rows, &refs, write_limits())
        .await?;
    let store = LanceModuleStore::open(&uri(&f), physical_schema()).await?;
    let pin = store.snapshot();
    let generation = KnowledgeGeneration {
        corpus_uri: uri(&f),
        version: pin.version(),
        physical_branch: None,
        inherited_target: None,
    };
    let mut calls = 0;
    read_knowledge_reference_snapshot_guarded(
        ctx.snapshot(),
        &pin,
        &generation,
        query(Some("a"), None),
        limits(),
        || {
            calls += 1;
            Ok(())
        },
    )
    .await?;
    assert!(
        calls > 4,
        "guard must cover both streams and terminal delivery"
    );
    for fail_at in [calls / 2, calls] {
        let mut observed = 0;
        let error = read_knowledge_reference_snapshot_guarded(
            ctx.snapshot(),
            &pin,
            &generation,
            query(Some("a"), None),
            limits(),
            || {
                observed += 1;
                if observed == fail_at {
                    Err(KnowledgeIndexError::Auth(graphit_auth::AuthError::Denied))
                } else {
                    Ok(())
                }
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(
            &error,
            KnowledgeReferenceQueryError::Data(KnowledgeIndexError::Auth(
                graphit_auth::AuthError::Denied
            ))
        ));
        assert!(error.source().is_some());
    }
    let mut observed = 0;
    let error = read_knowledge_reference_snapshot_guarded(
        ctx.snapshot(),
        &pin,
        &generation,
        query(Some("a"), None),
        limits(),
        || {
            observed += 1;
            if observed == calls {
                cancel.cancel();
            }
            Ok(())
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&error,KnowledgeReferenceQueryError::Data(KnowledgeIndexError::Request(e)) if e.kind()==std::io::ErrorKind::Interrupted)
    );
    Ok(())
}

#[tokio::test]
async fn reference_projection_does_not_decode_or_budget_document_body() -> Result<(), Box<dyn Error>>
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
    let docs = [page("a", &["a"])];
    let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits())?;
    let fields = [KnowledgeReferenceFields {
        slug: "a",
        state: KnowledgeDocumentState::Active,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project(project()),
            context: None,
        },
        authored: &[],
    }];
    let refs = prepare_knowledge_references(ctx.snapshot(), &rows, &fields, read_limits().content)?;
    publish_native_authored_with_references(ctx.snapshot(), &write, &rows, &refs, write_limits())
        .await?;
    let store = LanceModuleStore::open(&uri(&f), physical_schema()).await?;
    let pin = store.snapshot();
    let scan = pin.scan();
    let mut stream = scan.try_into_stream().await?;
    let mut batches = vec![];
    while let Some(batch) = stream.try_next().await? {
        let body_index = batch.schema().index_of("body")?;
        let mut cols = batch.columns().to_vec();
        cols[body_index] = Arc::new(arrow_array::StringArray::from(vec![
            "β".repeat(131072);
            batch.num_rows()
        ]));
        batches.push(arrow_array::RecordBatch::try_new(batch.schema(), cols)?);
    }
    drop(stream);
    drop(scan);
    pin.prepare_replace(batches).await?.commit().await?;
    let (_, result) = published(
        read_native_knowledge_reference_query(
            ctx.snapshot(),
            &read,
            query(Some("a"), None),
            limits(),
        )
        .await?,
    );
    let out = result.outgoing.unwrap();
    assert!(out.complete);
    assert_eq!(out.total, 1);
    let graphit_data::knowledge_index::KnowledgeIndexPageRead::Published {
        page: graphit_data::knowledge_snapshot::KnowledgeSnapshotPage::Found { document, .. },
        ..
    } = graphit_data::knowledge_index::read_native_knowledge_page(
        ctx.snapshot(),
        &read,
        "a",
        read_limits(),
    )
    .await?
    else {
        panic!("the complete long document must remain readable");
    };
    assert_eq!(document.body, "β".repeat(131072));
    Ok(())
}

#[tokio::test]
async fn actual_role_revocation_and_capture_replacement_never_yield_success()
-> Result<(), Box<dyn Error>> {
    for replace_capture in [false, true] {
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
        let docs = [page("a", &["a", "a"])];
        let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits())?;
        let fields = [KnowledgeReferenceFields {
            slug: "a",
            state: KnowledgeDocumentState::Active,
            namespace: KnowledgeReferenceNamespace {
                scope: KnowledgeReferenceScope::Project(project()),
                context: None,
            },
            authored: &[],
        }];
        let refs =
            prepare_knowledge_references(ctx.snapshot(), &rows, &fields, read_limits().content)?;
        publish_native_authored_with_references(
            ctx.snapshot(),
            &write,
            &rows,
            &refs,
            write_limits(),
        )
        .await?;
        let (_, result) = published(
            read_native_knowledge_reference_query(
                ctx.snapshot(),
                &read,
                query(Some("a"), None),
                limits(),
            )
            .await?,
        );
        assert_eq!(result.outgoing.unwrap().total, 2);
        if replace_capture {
            let corpus = f.project.runtime_root().join("knowledge/index.lance");
            let (result, ()) = tokio::join!(
                read_native_knowledge_reference_query(
                    ctx.snapshot(),
                    &read,
                    query(Some("a"), None),
                    limits()
                ),
                async {
                    tokio::task::yield_now().await;
                    std::fs::rename(&corpus, corpus.with_file_name("observed-old.lance")).unwrap();
                    std::fs::create_dir(&corpus).unwrap();
                }
            );
            let error = result.unwrap_err();
            assert!(matches!(&error, KnowledgeReferenceQueryError::Data(_)));
            assert!(error.source().is_some());
        } else {
            issuer.revoke(&parent, &bearer)?;
            assert!(matches!(
                read_native_knowledge_reference_query(
                    ctx.snapshot(),
                    &read,
                    query(None, None),
                    limits()
                )
                .await,
                Err(KnowledgeReferenceQueryError::Data(
                    KnowledgeIndexError::Auth(graphit_auth::AuthError::Denied)
                ))
            ));
        }
    }
    Ok(())
}

#[tokio::test]
async fn removed_rows_empty_edges_and_long_utf8_metadata_keep_query_semantics()
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
    let title = "β".repeat(125);
    let mut a = page("a", &["a", ""]);
    a.title = Some(&title);
    let mut b = page("b", &[]);
    b.title = Some(&title);
    let rows = h::prepare(ctx.snapshot(), &[a, b], &[], hlimits())?;
    let ns = KnowledgeReferenceNamespace {
        scope: KnowledgeReferenceScope::Project(project()),
        context: None,
    };
    let fields = [
        KnowledgeReferenceFields {
            slug: "a",
            state: KnowledgeDocumentState::Active,
            namespace: ns,
            authored: &[],
        },
        KnowledgeReferenceFields {
            slug: "b",
            state: KnowledgeDocumentState::Removed,
            namespace: ns,
            authored: &[],
        },
    ];
    let refs = prepare_knowledge_references(ctx.snapshot(), &rows, &fields, read_limits().content)?;
    publish_native_authored_with_references(ctx.snapshot(), &write, &rows, &refs, write_limits())
        .await?;
    let exact = limits();
    let (_, result) = published(
        read_native_knowledge_reference_query(ctx.snapshot(), &read, query(Some("a"), None), exact)
            .await?,
    );
    assert_eq!(result.outgoing.unwrap().total, 1);
    let mut too_many = query(Some("a"), None);
    too_many.page_size = n(101);
    assert!(matches!(
        read_native_knowledge_reference_query(ctx.snapshot(), &read, too_many, exact).await,
        Err(KnowledgeReferenceQueryError::InvalidQuery("page_size"))
    ));
    Ok(())
}
