use arrow_array::{
    ArrayRef, BooleanArray, DurationNanosecondArray, Int64Array, RecordBatch, StringArray,
    StructArray, TimestampNanosecondArray, UInt8Array, UInt64Array,
    builder::{FixedSizeBinaryBuilder, ListBuilder, StringBuilder},
    new_null_array,
};
use arrow_schema::{DataType, Field, Schema};
use graphit_core::{
    file_io::SnapshotContext, file_lock::Cancellation,
    native_document_file::NativeDocumentFileLimits,
};
use graphit_data::{
    knowledge_document_projection::AUTHORED_RECIPE_ID,
    knowledge_index::{
        KnowledgeHeadReadLimits, KnowledgeIndexContentLimits, KnowledgeIndexError,
        KnowledgeIndexReadLimits, physical_schema,
    },
    knowledge_snapshot::{
        KnowledgeSnapshotPage, read_knowledge_heads_snapshot_guarded, read_knowledge_snapshot,
        read_knowledge_snapshot_guarded,
    },
    lance_module_store::{LanceModuleStore, ModuleSnapshot},
};
use std::{
    error::Error,
    num::{NonZeroU32, NonZeroUsize},
    sync::Arc,
    time::{Duration, Instant},
};
fn n(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}
fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(120),
        Cancellation::default(),
    ))
}
fn limits() -> KnowledgeIndexReadLimits {
    KnowledgeIndexReadLimits {
        content: KnowledgeIndexContentLimits {
            max_identifier_bytes: n(64),
        },
        native: NativeDocumentFileLimits::new(65536, 65536, 256, 65536).unwrap(),
        batch_rows: NonZeroU32::new(1).unwrap(),
    }
}
fn batch(slugs: &[&str], bodies: &[&str], recipes: &[&str]) -> RecordBatch {
    let schema = physical_schema();
    let rows = slugs.len();
    assert_eq!(bodies.len(), rows);
    assert_eq!(recipes.len(), rows);
    let strings = |value: &str| -> ArrayRef {
        Arc::new(StringArray::from_iter_values(std::iter::repeat_n(
            value, rows,
        )))
    };
    let mut physical = FixedSizeBinaryBuilder::new(32);
    let mut compilation = FixedSizeBinaryBuilder::new(32);
    let DataType::List(item) = schema.field(15).data_type() else {
        unreachable!()
    };
    let mut targets = ListBuilder::new(StringBuilder::new()).with_field(Arc::clone(item));
    for _ in 0..rows {
        physical.append_null();
        compilation.append_value([1u8; 32]).unwrap();
        targets.values().append_value("target");
        targets.append(true);
    }
    let DataType::Struct(fields) = schema.field(11).data_type() else {
        unreachable!()
    };
    let parameters = StructArray::new(
        fields.clone(),
        vec![
            strings("title"),
            strings("manual"),
            Arc::new(UInt64Array::from(vec![128; rows])),
        ],
        None,
    );
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from_iter_values(slugs.iter().copied())),
            strings("title"),
            Arc::new(StringArray::from_iter_values(bodies.iter().copied())),
            strings("manual"),
            strings("summary"),
            Arc::new(StringArray::from(vec![None::<&str>; rows])),
            Arc::new(BooleanArray::from(vec![false; rows])),
            strings("docs/a.md"),
            Arc::new(physical.finish()),
            Arc::new(compilation.finish()),
            Arc::new(StringArray::from_iter_values(recipes.iter().copied())),
            Arc::new(parameters),
            Arc::new(TimestampNanosecondArray::from(vec![None; rows]).with_timezone("UTC")),
            Arc::new(TimestampNanosecondArray::from(vec![None; rows]).with_timezone("UTC")),
            Arc::new(DurationNanosecondArray::from(vec![None; rows])),
            Arc::new(targets.finish()),
            Arc::new(UInt8Array::from(vec![1; rows])),
            new_null_array(physical_schema().field(17).data_type(), rows),
        ],
    )
    .unwrap()
}
async fn create(
    root: &tempfile::TempDir,
    batches: Vec<RecordBatch>,
) -> Result<ModuleSnapshot, Box<dyn Error>> {
    let uri = root.path().join("index.lance");
    let store = LanceModuleStore::create(uri.to_str().unwrap(), physical_schema(), batches).await?;
    Ok(store.snapshot())
}

#[tokio::test]
async fn one_selected_document_survives_commit_with_its_original_sdk_pin()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let ctx = original();
    let old = create(
        &root,
        vec![batch(
            &["picked", "other"],
            &["old literal λ", "unselected"],
            &[AUTHORED_RECIPE_ID; 2],
        )],
    )
    .await?;
    let old_version = old.version();
    let newer = old
        .prepare_replace(vec![batch(&["picked"], &["new"], &[AUTHORED_RECIPE_ID])])
        .await?
        .commit()
        .await?;
    assert!(newer.snapshot().version() > old_version);
    let KnowledgeSnapshotPage::Found { document: doc, .. } =
        read_knowledge_snapshot(&ctx, &old, "picked", limits()).await?
    else {
        panic!("selected old page expected")
    };
    assert_eq!(doc.body, "old literal λ");
    assert_eq!(doc.description, None);
    assert_eq!(old.version(), old_version);
    let KnowledgeSnapshotPage::Found { document: doc, .. } =
        read_knowledge_snapshot(&ctx, &newer.snapshot(), "picked", limits()).await?
    else {
        panic!("selected new page expected")
    };
    assert_eq!(doc.body, "new");
    assert_eq!(
        read_knowledge_snapshot(&ctx, &old, "absent", limits()).await?,
        KnowledgeSnapshotPage::PageAbsent
    );
    Ok(())
}

#[tokio::test]
async fn valid_empty_is_distinct_and_requested_slug_is_validated() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let ctx = original();
    let empty = create(&root, vec![RecordBatch::new_empty(physical_schema())]).await?;
    assert_eq!(
        read_knowledge_snapshot(&ctx, &empty, "absent", limits()).await?,
        KnowledgeSnapshotPage::PublishedEmpty
    );
    assert!(matches!(
        read_knowledge_snapshot(&ctx, &empty, "../bad", limits()).await,
        Err(KnowledgeIndexError::InvalidField("slug"))
    ));
    assert!(matches!(
        read_knowledge_snapshot(&ctx, &empty, &"x".repeat(65), limits()).await,
        Err(KnowledgeIndexError::LimitExceeded("identifier_bytes"))
    ));
    Ok(())
}

#[tokio::test]
async fn selected_query_ignores_other_bodies_and_validates_its_complete_stream()
-> Result<(), Box<dyn Error>> {
    for recipes in [
        [AUTHORED_RECIPE_ID, "invalid"],
        [AUTHORED_RECIPE_ID, AUTHORED_RECIPE_ID],
    ] {
        let root = tempfile::tempdir()?;
        let ctx = original();
        let pin = create(
            &root,
            vec![
                batch(&["picked"], &["ok"], &[AUTHORED_RECIPE_ID]),
                batch(&["other"], &["late"], &[recipes[1]]),
            ],
        )
        .await?;
        assert!(matches!(
            read_knowledge_snapshot(&ctx, &pin, "picked", limits()).await?,
            KnowledgeSnapshotPage::Found { .. }
        ));
        assert_eq!(
            read_knowledge_snapshot(&ctx, &pin, "absent", limits()).await?,
            KnowledgeSnapshotPage::PageAbsent
        );
        if recipes[1] == "invalid" {
            assert!(matches!(
                read_knowledge_snapshot(&ctx, &pin, "other", limits()).await,
                Err(KnowledgeIndexError::InvalidField("recipe"))
            ));
        } else {
            let duplicated = pin
                .prepare_replace(vec![batch(
                    &["picked", "other", "other"],
                    &["ok", "a", "b"],
                    &[AUTHORED_RECIPE_ID; 3],
                )])
                .await?
                .commit()
                .await?;
            assert!(matches!(
                read_knowledge_snapshot(&ctx, &duplicated.snapshot(), "picked", limits()).await?,
                KnowledgeSnapshotPage::Found { .. }
            ));
            assert!(matches!(
                read_knowledge_snapshot(&ctx, &duplicated.snapshot(), "other", limits()).await,
                Err(KnowledgeIndexError::DuplicateSlug)
            ));
        }
    }
    Ok(())
}

#[tokio::test]
async fn long_document_content_is_complete_and_individual_slug_is_bounded()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let ctx = original();
    let large = "λ".repeat(350);
    let pin = create(
        &root,
        vec![batch(
            &["picked", "a", "b", "c"],
            &[large.as_str(); 4],
            &[AUTHORED_RECIPE_ID; 4],
        )],
    )
    .await?;
    let KnowledgeSnapshotPage::Found { document: doc, .. } =
        read_knowledge_snapshot(&ctx, &pin, "picked", limits()).await?
    else {
        panic!("selected page")
    };
    assert_eq!(doc.body, large);
    let KnowledgeSnapshotPage::Found {
        document: repeated, ..
    } = read_knowledge_snapshot(&ctx, &pin, "picked", limits()).await?
    else {
        panic!("selected page");
    };
    assert_eq!(repeated.body, large);
    let mut smaller_identifier = limits();
    smaller_identifier.content.max_identifier_bytes = n(3);
    assert!(matches!(
        read_knowledge_snapshot(&ctx, &pin, "picked", smaller_identifier).await,
        Err(KnowledgeIndexError::LimitExceeded("identifier_bytes"))
    ));
    Ok(())
}

#[tokio::test]
async fn wrong_schema_original_causes_and_sdk_failure_do_not_become_absence()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let ctx = original();
    let pin = create(
        &root,
        vec![batch(&["picked"], &["ok"], &[AUTHORED_RECIPE_ID])],
    )
    .await?;
    let cancel = Cancellation::default();
    cancel.cancel();
    let cancelled = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(120),
        cancel,
    ));
    assert!(
        matches!(read_knowledge_snapshot(&cancelled,&pin,"picked",limits()).await,Err(KnowledgeIndexError::Request(e)) if e.kind()==std::io::ErrorKind::Interrupted)
    );
    let expired = Arc::new(SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    ));
    assert!(
        matches!(read_knowledge_snapshot(&expired,&pin,"picked",limits()).await,Err(KnowledgeIndexError::Request(e)) if e.kind()==std::io::ErrorKind::TimedOut)
    );
    let bad = root.path().join("wrong.lance");
    let schema = Arc::new(Schema::new(vec![Field::new("slug", DataType::Int64, true)]));
    let store = LanceModuleStore::create(
        bad.to_str().unwrap(),
        Arc::clone(&schema),
        vec![RecordBatch::try_new(
            schema,
            vec![Arc::new(Int64Array::from(vec![None]))],
        )?],
    )
    .await?;
    assert!(matches!(
        read_knowledge_snapshot(&ctx, &store.snapshot(), "picked", limits()).await,
        Err(KnowledgeIndexError::InvalidField("schema"))
    ));
    std::fs::remove_dir_all(root.path().join("index.lance"))?;
    let error = read_knowledge_snapshot(&ctx, &pin, "picked", limits())
        .await
        .unwrap_err();
    assert!(matches!(error, KnowledgeIndexError::Store(_)));
    assert!(error.source().is_some());
    Ok(())
}

fn head_limits() -> KnowledgeHeadReadLimits {
    KnowledgeHeadReadLimits {
        max_slug_bytes: n(64),
        native: limits().native,
        batch_rows: NonZeroU32::new(1).unwrap(),
    }
}
#[tokio::test]
async fn guarded_heads_retain_the_pin_without_loading_bodies() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let ctx = original();
    let huge = "é".repeat(4096);
    let old = create(
        &root,
        vec![batch(
            &["b", "a"],
            &[&huge, &huge],
            &[AUTHORED_RECIPE_ID; 2],
        )],
    )
    .await?;
    let version = old.version();
    let heads = read_knowledge_heads_snapshot_guarded(&ctx, &old, head_limits(), || Ok(())).await?;
    assert_eq!(
        heads.iter().map(|h| h.slug.as_str()).collect::<Vec<_>>(),
        ["a", "b"]
    );
    let KnowledgeSnapshotPage::Found { document, .. } =
        read_knowledge_snapshot_guarded(&ctx, &old, "a", limits(), || Ok(())).await?
    else {
        panic!("the pinned document must be present");
    };
    assert_eq!(document.body, huge);
    let newer = old.prepare_replace(vec![]).await?.commit().await?;
    assert!(newer.snapshot().version() > version);
    assert_eq!(
        read_knowledge_heads_snapshot_guarded(&ctx, &old, head_limits(), || Ok(())).await?,
        heads
    );
    assert!(
        read_knowledge_heads_snapshot_guarded(&ctx, &newer.snapshot(), head_limits(), || Ok(()))
            .await?
            .is_empty()
    );
    Ok(())
}
#[tokio::test]
async fn guarded_page_selects_one_record_and_heads_validate_all_metadata()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let ctx = original();
    let first = batch(&["picked"], &["valid"], &[AUTHORED_RECIPE_ID]);
    let late = batch(&["late"], &["valid"], &[AUTHORED_RECIPE_ID]);
    let mut columns = late.columns().to_vec();
    columns[16] = Arc::new(UInt8Array::from(vec![9]));
    let late = RecordBatch::try_new(late.schema(), columns)?;
    let pin = create(&root, vec![first, late]).await?;
    assert!(matches!(
        read_knowledge_snapshot_guarded(&ctx, &pin, "picked", limits(), || Ok(())).await?,
        KnowledgeSnapshotPage::Found { .. }
    ));
    assert_eq!(
        read_knowledge_snapshot_guarded(&ctx, &pin, "missing", limits(), || Ok(())).await?,
        KnowledgeSnapshotPage::PageAbsent
    );
    assert!(matches!(
        read_knowledge_snapshot_guarded(&ctx, &pin, "late", limits(), || Ok(())).await,
        Err(KnowledgeIndexError::InvalidField("document_state"))
    ));
    assert!(matches!(
        read_knowledge_heads_snapshot_guarded(&ctx, &pin, head_limits(), || Ok(())).await,
        Err(KnowledgeIndexError::InvalidField("document_state"))
    ));
    // A typed external failure wins before the invalid native row; it never turns into absence.
    let denied = || Err(KnowledgeIndexError::Auth(graphit_auth::AuthError::Denied));
    assert!(matches!(
        read_knowledge_snapshot_guarded(&ctx, &pin, "picked", limits(), denied).await,
        Err(KnowledgeIndexError::Auth(graphit_auth::AuthError::Denied))
    ));
    assert!(matches!(
        read_knowledge_heads_snapshot_guarded(&ctx, &pin, head_limits(), denied).await,
        Err(KnowledgeIndexError::Auth(graphit_auth::AuthError::Denied))
    ));
    Ok(())
}

#[tokio::test]
async fn a_late_guard_failure_and_cancelled_original_discard_completed_scan_data()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let pin = create(
        &root,
        vec![batch(
            &["picked", "later"],
            &["valid"; 2],
            &[AUTHORED_RECIPE_ID; 2],
        )],
    )
    .await?;
    // Discover the terminal guard callback with a successful real SDK scan, rather
    // than binding this fixture to the collector's internal number of checks.
    for heads in [false, true] {
        let ctx = original();
        let mut terminal = 0;
        let observe = || {
            terminal += 1;
            Ok(())
        };
        if heads {
            assert_eq!(
                read_knowledge_heads_snapshot_guarded(&ctx, &pin, head_limits(), observe)
                    .await?
                    .len(),
                2
            );
        } else {
            assert!(matches!(
                read_knowledge_snapshot_guarded(&ctx, &pin, "picked", limits(), observe).await?,
                KnowledgeSnapshotPage::Found { .. }
            ));
        }
        assert!(terminal > 1);
        for cancel_original in [false, true] {
            let cancellation = Cancellation::default();
            let ctx = Arc::new(SnapshotContext::new(
                Instant::now() + Duration::from_secs(120),
                cancellation.clone(),
            ));
            let mut calls = 0;
            let late = || {
                calls += 1;
                if calls == terminal {
                    if cancel_original {
                        cancellation.cancel();
                        ctx.check()?;
                    } else {
                        return Err(KnowledgeIndexError::Auth(graphit_auth::AuthError::Denied));
                    }
                }
                Ok(())
            };
            let error = if heads {
                read_knowledge_heads_snapshot_guarded(&ctx, &pin, head_limits(), late)
                    .await
                    .unwrap_err()
            } else {
                read_knowledge_snapshot_guarded(&ctx, &pin, "picked", limits(), late)
                    .await
                    .unwrap_err()
            };
            if cancel_original {
                assert!(
                    matches!(error, KnowledgeIndexError::Request(ref cause) if cause.kind() == std::io::ErrorKind::Interrupted)
                );
            } else {
                assert!(matches!(
                    error,
                    KnowledgeIndexError::Auth(graphit_auth::AuthError::Denied)
                ));
            }
            assert_eq!(calls, terminal);
        }
    }
    Ok(())
}
