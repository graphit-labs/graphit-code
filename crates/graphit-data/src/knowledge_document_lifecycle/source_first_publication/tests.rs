use super::*;
#[path = "fixture.rs"]
mod fixture;
use crate::knowledge_document_projection as h;
use crate::knowledge_document_update::{
    KnowledgeDocumentAuthoredUpdate, KnowledgeDocumentReferenceUpdate,
};
use crate::lance_branch_store::BranchSnapshot;
use fixture::*;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::artifact_store::{
    ArtifactStore, ProjectIgnoreCriteria, ProjectSelectionCriteria, ProjectSelectionLimits,
    SourceReadLimits,
};
use graphit_core::file_lock::Cancellation;
use std::{error::Error, path::Path};

fn managed() -> h::AuthoredPageFields<'static> {
    let mut p = page("a", &[]);
    p.source_path = "knowledge-source/t/646f6373/7265616c2e6d64";
    p
}
fn decisions() -> [KnowledgeDocumentReferenceUpdate<'static>; 1] {
    [KnowledgeDocumentReferenceUpdate {
        slug: "a",
        authored: KnowledgeDocumentAuthoredUpdate::Preserve,
    }]
}
fn capture(f: &Fixture, original: &Arc<SnapshotContext>) -> CapturedSourceReader {
    let root = f.project.owner_root();
    std::fs::create_dir_all(root.join("docs")).unwrap();
    if !root.join("docs/real.md").exists() {
        std::fs::write(root.join("docs/real.md"), "# physical β body").unwrap();
    }
    std::fs::write(root.join(".wikiignore"), "excluded/\n").unwrap();
    let protected = format!("{}/", f.project.brand().directory());
    CapturedSourceReader::capture_selected_project(
        &f.project,
        ProjectSelectionCriteria {
            subtree: Some(Path::new("docs")),
            extras: &[],
            include_readme: false,
            extensions: &[".md"],
            ignore: ProjectIgnoreCriteria {
                protected_defaults: &[protected.as_str()],
                custom_control: Some(".wikiignore"),
                git_control: Some(".gitignore"),
            },
        },
        ProjectSelectionLimits {
            max_depth: 32,
            max_input_items: 128,
            max_input_bytes: 65536,
            max_observed_path_bytes: 4096,
            max_selected_file_bytes: 4096,
            max_control_file_bytes: 4096,
        },
        SourceReadLimits::new(4096, 4096, 65536).unwrap(),
        original,
    )
    .unwrap()
}
async fn early_writer(f: &Fixture, original: &Arc<SnapshotContext>) -> Result<(), Box<dyn Error>> {
    let artifacts = ArtifactStore::new(f.project.clone(), write_limits().artifacts);
    let original = original.clone();
    let writer =
        tokio::task::spawn_blocking(move || artifacts.writer_with_context(&original)).await??;
    drop(writer);
    Ok(())
}
async fn verify_observed(
    e: &KnowledgeSourceFirstPublicationError,
    named: bool,
) -> Result<(), Box<dyn Error>> {
    assert_eq!(
        e.effect(),
        KnowledgeSourcePublicationEffect::PublicationObserved
    );
    assert!(e.source().is_some());
    let publication = e.known_publication().expect("known actual commit");
    let key = e.recovery_key().expect("pre-commit Main key");
    assert_eq!(key.corpus_uri(), publication.default.corpus_uri);
    assert_eq!(key.base_version(), 0);
    let fresh =
        BranchSnapshot::capture_observed(&publication.default.corpus_uri, None, None).await?;
    assert_eq!(fresh.version(), publication.default.version);
    assert_eq!(publication.named.is_some(), named);
    if let Some(generation) = &publication.named {
        let pin = BranchSnapshot::capture_observed(
            &generation.corpus_uri,
            generation.physical_branch.as_deref(),
            None,
        )
        .await?;
        assert_eq!(pin.version(), generation.version);
        assert!(generation.inherited_target.is_none());
    }
    let store = LanceModuleStore::open(key.corpus_uri(), index::physical_schema()).await?;
    assert!(
        matches!(store.resolve(key).await?, crate::lance_module_store::Resolution::Applied { version } if version == publication.default.version)
    );
    Ok(())
}

fn history_read_limits() -> index::KnowledgeIndexReadLimits {
    let write = write_limits();
    index::KnowledgeIndexReadLimits {
        content: write.content,
        native: write.native,
        batch_rows: std::num::NonZeroU32::new(1).unwrap(),
    }
}

#[tokio::test]
async fn knowledge_sync_history_real_stream_delta_noop_cursor_cutoff_and_corruption()
-> Result<(), Box<dyn Error>> {
    use crate::knowledge_document_lifecycle::{
        KnowledgeDocumentLifecycleLimits, reconcile_native_knowledge_source_file,
    };
    use crate::knowledge_document_update::KnowledgeDocumentUpdateResult;
    use crate::knowledge_sync_history::*;
    use crate::lance_module_store::RecordBatchStreamAdapter;
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
    let query = |cursor| KnowledgeSyncHistoryQuery {
        limit: n(1),
        cursor,
    };
    assert!(matches!(
        read_native_knowledge_sync_history(
            ctx.snapshot(),
            &read,
            query(None),
            history_read_limits()
        )
        .await?,
        KnowledgeSyncHistoryRead::NoCorpus
    ));
    early_writer(&f, ctx.snapshot()).await?;
    let source = capture(&f, ctx.snapshot());
    let mut b = managed();
    b.slug = Some("b");
    b.source_path = "knowledge-source/t/646f6373/622e6d64";
    let rows = h::prepare(ctx.snapshot(), &[managed(), b], &[], hlimits())?;
    let refs = ["a", "b"].map(|slug| KnowledgeDocumentReferenceUpdate {
        slug,
        authored: KnowledgeDocumentAuthoredUpdate::Preserve,
    });
    let image = crate::knowledge_first_publication::encode_native_knowledge_source_batch(
        ctx.snapshot(),
        &write,
        &rows,
        &refs,
        write_limits(),
    )?;
    let input = Box::pin(RecordBatchStreamAdapter::new(
        index::physical_schema(),
        futures::stream::iter(vec![Ok(image)]),
    ));
    let (first, _) = publish_native_knowledge_source_stream(
        ctx.snapshot(),
        &read,
        &write,
        &source,
        input,
        write_limits(),
    )
    .await?;
    std::fs::write(f.project.owner_root().join("docs/real.md"), "updated")?;
    let source = capture(&f, ctx.snapshot());
    let mut a = managed();
    a.body = Some("updated");
    let desired = h::prepare(ctx.snapshot(), &[a], &[], hlimits())?;
    let image = crate::knowledge_first_publication::encode_native_knowledge_source_batch(
        ctx.snapshot(),
        &write,
        &desired,
        &decisions(),
        write_limits(),
    )?;
    let lifecycle = KnowledgeDocumentLifecycleLimits {
        read: history_read_limits(),
        write: write_limits(),
    };
    let KnowledgeDocumentUpdateResult::Updated {
        generation: second, ..
    } = reconcile_native_knowledge_source_file(
        ctx.snapshot(),
        &read,
        &write,
        &first.default,
        &source,
        image.clone(),
        lifecycle,
        |_, old, new| Ok(old == new),
    )
    .await?
    else {
        panic!("expected FILE delta")
    };
    assert!(
        matches!(reconcile_native_knowledge_source_file(ctx.snapshot(), &read, &write, &second, &source, image.clone(), lifecycle, |_, old, new| Ok(old == new)).await?, KnowledgeDocumentUpdateResult::Unchanged { generation } if generation == second)
    );
    let KnowledgeSyncHistoryRead::Published {
        entries,
        next_cursor: Some(cursor),
        generation,
    } = read_native_knowledge_sync_history(
        ctx.snapshot(),
        &read,
        query(None),
        history_read_limits(),
    )
    .await?
    else {
        panic!("two syncs")
    };
    assert_eq!(generation, second);
    assert_eq!(
        entries[0].changes,
        KnowledgeSyncChanges {
            added: vec![],
            updated: vec!["a".into()],
            removed: vec![]
        }
    );
    // A later SDK-only commit has no sync property and must not invalidate cutoff.
    let pin = BranchSnapshot::capture_observed(&second.corpus_uri, None, None).await?;
    let later = pin
        .prepare_merge(vec![image.clone()], vec!["slug".into()])
        .await?
        .commit()
        .await?;
    let KnowledgeSyncHistoryRead::Published {
        entries,
        next_cursor,
        generation,
    } = read_native_knowledge_sync_history(
        ctx.snapshot(),
        &read,
        query(Some(cursor.clone())),
        history_read_limits(),
    )
    .await?
    else {
        panic!("continuation")
    };
    assert_eq!(generation.version, second.version);
    assert_eq!(entries[0].version, first.default.version);
    assert_eq!(entries[0].changes.added, ["a", "b"]);
    assert!(next_cursor.is_none());
    let wrong_limit = KnowledgeSyncHistoryQuery {
        limit: n(2),
        cursor: Some(cursor),
    };
    assert!(matches!(
        read_native_knowledge_sync_history(
            ctx.snapshot(),
            &read,
            wrong_limit,
            history_read_limits()
        )
        .await,
        Err(KnowledgeSyncHistoryError::CursorBinding)
    ));
    let KnowledgeSyncHistoryRead::Published { entries, .. } = read_native_knowledge_sync_history(
        ctx.snapshot(),
        &read,
        KnowledgeSyncHistoryQuery {
            limit: n(8),
            cursor: None,
        },
        history_read_limits(),
    )
    .await?
    else {
        panic!("history")
    };
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|e| e.version != later.version()));
    let mut bad = later
        .prepare_merge(vec![image], vec!["slug".into()])
        .await?;
    bad.set_transaction_property(SYNC_PROPERTY.into(), "{broken".into());
    bad.commit().await?;
    assert!(matches!(
        read_native_knowledge_sync_history(
            ctx.snapshot(),
            &read,
            query(None),
            history_read_limits()
        )
        .await,
        Err(KnowledgeSyncHistoryError::MalformedProperty(_))
    ));
    cancel.cancel();
    assert!(matches!(
        read_native_knowledge_sync_history(
            ctx.snapshot(),
            &read,
            query(None),
            history_read_limits()
        )
        .await,
        Err(KnowledgeSyncHistoryError::Data(
            KnowledgeIndexError::Request(_)
        ))
    ));
    Ok(())
}

#[tokio::test]
async fn knowledge_sync_history_valid_empty_first_stream_and_named_inventory()
-> Result<(), Box<dyn Error>> {
    use crate::knowledge_sync_history::*;
    use crate::lance_module_store::RecordBatchStreamAdapter;
    for named in [false, true] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::with_version(&ctx, if named { "named history" } else { "" });
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
        early_writer(&f, ctx.snapshot()).await?;
        let source = capture(&f, ctx.snapshot());
        let empty = arrow_array::RecordBatch::new_empty(index::physical_schema());
        let input = Box::pin(RecordBatchStreamAdapter::new(
            index::physical_schema(),
            futures::stream::iter(vec![Ok(empty)]),
        ));
        let (first, _) = publish_native_knowledge_source_stream(
            ctx.snapshot(),
            &read,
            &write,
            &source,
            input,
            write_limits(),
        )
        .await?;
        let selected = first.named.as_ref().unwrap_or(&first.default);
        let KnowledgeSyncHistoryRead::Published {
            entries,
            next_cursor,
            generation,
        } = read_native_knowledge_sync_history(
            ctx.snapshot(),
            &read,
            KnowledgeSyncHistoryQuery {
                limit: n(4),
                cursor: None,
            },
            history_read_limits(),
        )
        .await?
        else {
            panic!("empty is a publication")
        };
        assert_eq!(&generation, selected);
        // Native materialization retains the actual branch transaction inventory;
        // assert its observed first event rather than fabricate a second event.
        assert_eq!(
            entries.len(),
            1,
            "first named Clone must carry its own real sync property once the SDK seam is composed"
        );
        if let Some(entry) = entries.first() {
            assert_eq!(entry.changes, KnowledgeSyncChanges::default());
        }
        assert!(next_cursor.is_none());
    }
    Ok(())
}

#[tokio::test]
async fn knowledge_sync_history_direct_update_and_named_actual_noncontiguous_refs()
-> Result<(), Box<dyn Error>> {
    use crate::knowledge_document_update::{
        KnowledgeDocumentUpdateLimits, KnowledgeDocumentUpdateResult,
        update_existing_native_knowledge_documents,
    };
    use crate::knowledge_sync_history::*;
    use crate::lance_branch_store::LanceBranchStore;
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
    early_writer(&f, ctx.snapshot()).await?;
    let source = capture(&f, ctx.snapshot());
    let rows = h::prepare(ctx.snapshot(), &[managed()], &[], hlimits())?;
    let image = crate::knowledge_first_publication::encode_native_knowledge_source_batch(
        ctx.snapshot(),
        &write,
        &rows,
        &decisions(),
        write_limits(),
    )?;
    let input = Box::pin(crate::lance_module_store::RecordBatchStreamAdapter::new(
        index::physical_schema(),
        futures::stream::iter([Ok(image)]),
    ));
    let (first, _) = publish_native_knowledge_source_stream(
        ctx.snapshot(),
        &read,
        &write,
        &source,
        input,
        write_limits(),
    )
    .await?;
    let mut changed = managed();
    changed.body = Some("direct update");
    let replacement = h::prepare(ctx.snapshot(), &[changed], &[], hlimits())?;
    let limits = KnowledgeDocumentUpdateLimits {
        read: history_read_limits(),
        write: write_limits(),
    };
    let KnowledgeDocumentUpdateResult::Updated {
        generation: second, ..
    } = update_existing_native_knowledge_documents(
        ctx.snapshot(),
        &read,
        &write,
        &first.default,
        &replacement,
        &decisions(),
        limits,
        |_, old, new| Ok(old == new),
    )
    .await?
    else {
        panic!("direct update")
    };
    let image = crate::knowledge_first_publication::encode_native_knowledge_source_batch(
        ctx.snapshot(),
        &write,
        &replacement,
        &decisions(),
        write_limits(),
    )?;
    // Advance default before creating another branch. Its first attached ID is
    // inherited from this actual version, leaving no manifests 1/2 on that branch.
    let default = BranchSnapshot::capture_observed(&second.corpus_uri, None, None).await?;
    let default = default
        .prepare_merge(vec![image.clone()], vec!["slug".into()])
        .await?
        .commit()
        .await?;
    assert!(default.version() >= 3);
    let named = f.named_version("history gaps");
    let encoded = named.lance_branch();
    let store = LanceBranchStore::open(&second.corpus_uri, index::physical_schema(), None).await?;
    let named_pin = store
        .capture_target(Some(&encoded))
        .await?
        .materialize()
        .await?;
    assert_eq!(
        named_pin
            .dataset()
            .version_refs()
            .await?
            .iter()
            .map(|r| r.version)
            .collect::<Vec<_>>(),
        vec![default.version()]
    );
    let named_read = auth.admit_authenticated_read(
        &caller,
        &ctx,
        f.scope_for(&named),
        ProjectReadModule::Knowledge,
    )?;
    let named_write = auth.admit_authenticated_write(
        &caller,
        &ctx,
        f.scope_for(&named),
        ProjectWriteOperation::Knowledge,
    )?;
    let named_generation = index::generation(&second.corpus_uri, &named_pin);
    let mut another = managed();
    another.body = Some("named update");
    let named_rows = h::prepare(ctx.snapshot(), &[another], &[], hlimits())?;
    let KnowledgeDocumentUpdateResult::Updated {
        generation: named_updated,
        ..
    } = update_existing_native_knowledge_documents(
        ctx.snapshot(),
        &named_read,
        &named_write,
        &named_generation,
        &named_rows,
        &decisions(),
        limits,
        |_, old, new| Ok(old == new),
    )
    .await?
    else {
        panic!("named update")
    };
    let KnowledgeSyncHistoryRead::Published {
        entries,
        next_cursor,
        ..
    } = read_native_knowledge_sync_history(
        ctx.snapshot(),
        &named_read,
        KnowledgeSyncHistoryQuery {
            limit: n(1),
            cursor: None,
        },
        history_read_limits(),
    )
    .await?
    else {
        panic!("named history")
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].version, named_updated.version);
    assert_eq!(entries[0].changes.updated, ["a"]);
    assert!(
        next_cursor.is_none(),
        "Clone has no sync event and lower IDs do not exist"
    );
    let KnowledgeSyncHistoryRead::Published { entries, .. } = read_native_knowledge_sync_history(
        ctx.snapshot(),
        &read,
        KnowledgeSyncHistoryQuery {
            limit: n(4),
            cursor: None,
        },
        history_read_limits(),
    )
    .await?
    else {
        panic!("default history")
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].version, second.version);
    assert_eq!(entries[0].changes.updated, ["a"]);
    assert_eq!(entries[1].changes.added, ["a"]);
    let KnowledgeSyncHistoryRead::Published {
        next_cursor: Some(cursor),
        ..
    } = read_native_knowledge_sync_history(
        ctx.snapshot(),
        &read,
        KnowledgeSyncHistoryQuery {
            limit: n(1),
            cursor: None,
        },
        history_read_limits(),
    )
    .await?
    else {
        panic!("default continuation")
    };
    assert!(matches!(
        read_native_knowledge_sync_history(
            ctx.snapshot(),
            &named_read,
            KnowledgeSyncHistoryQuery {
                limit: n(1),
                cursor: Some(cursor.clone())
            },
            history_read_limits()
        )
        .await,
        Err(KnowledgeSyncHistoryError::CursorBinding)
    ));
    // The lookahead promised an actual next event. SDK cleanup of that event
    // must be reported as lost coverage rather than being silently called EOF.
    let policy = lance::dataset::cleanup::CleanupPolicyBuilder::default()
        .versions(vec![first.default.version])?
        .build();
    default.dataset().cleanup_with_policy(policy).await?;
    assert!(matches!(
        read_native_knowledge_sync_history(
            ctx.snapshot(),
            &read,
            KnowledgeSyncHistoryQuery {
                limit: n(1),
                cursor: Some(cursor)
            },
            history_read_limits()
        )
        .await,
        Err(KnowledgeSyncHistoryError::HistoryChanged)
    ));
    Ok(())
}

#[tokio::test]
async fn knowledge_sync_history_published_after_error_and_late_stream_failure()
-> Result<(), Box<dyn Error>> {
    use crate::knowledge_sync_history::*;
    use crate::lance_module_store::{DataFusionError, RecordBatchStreamAdapter};
    for named in [false, true] {
        // Observe the exact confirmed default transaction after cancellation at delivery.
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::with_version(&ctx, if named { "named after error" } else { "" });
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
        early_writer(&f, ctx.snapshot()).await?;
        let source = capture(&f, ctx.snapshot());
        let rows = h::prepare(ctx.snapshot(), &[managed()], &[], hlimits())?;
        let image = crate::knowledge_first_publication::encode_native_knowledge_source_batch(
            ctx.snapshot(),
            &write,
            &rows,
            &decisions(),
            write_limits(),
        )?;
        let target = acquire_target(
            ctx.snapshot(),
            &read,
            &write,
            &source,
            write_limits(),
            &|_| {},
        )
        .await?;
        let changes =
            crate::knowledge_sync_history::KnowledgeSyncChanges::first_batch(&image, &|| {
                check_target(ctx.snapshot(), &read, &write, &source, &target)
            })?;
        let properties = changes.into_properties()?;
        let mut prepared =
            LanceModuleStore::prepare_create(&target.uri, index::physical_schema(), vec![image])
                .await?;
        prepared.set_transaction_properties(properties);
        let error = finish_prepared(
            ctx.snapshot(),
            &read,
            &write,
            &source,
            target,
            prepared,
            None,
            &|phase| {
                if phase == SourceFirstPhase::Delivery {
                    cancel.cancel();
                }
            },
        )
        .await
        .unwrap_err();
        verify_observed(&error, named).await?;
        let publication = error.known_publication().expect("confirmed publication");
        let pin =
            BranchSnapshot::capture_observed(&publication.default.corpus_uri, None, None).await?;
        let record = pin
            .dataset()
            .read_version_transaction(publication.default.version)
            .await?;
        let changes: KnowledgeSyncChanges = serde_json::from_str(
            record
                .transaction
                .as_ref()
                .unwrap()
                .transaction_properties
                .as_ref()
                .unwrap()
                .get(SYNC_PROPERTY)
                .unwrap(),
        )?;
        assert_eq!(changes.added, ["a"]);
        assert_eq!(
            record.transaction.as_ref().unwrap().uuid,
            error.recovery_key().unwrap().transaction_id()
        );
        if let Some(generation) = &publication.named {
            let named_pin = BranchSnapshot::capture_observed(
                &generation.corpus_uri,
                generation.physical_branch.as_deref(),
                None,
            )
            .await?;
            let named_record = named_pin
                .dataset()
                .read_version_transaction(generation.version)
                .await?;
            assert!(
                named_record
                    .transaction
                    .as_ref()
                    .unwrap()
                    .transaction_properties
                    .as_ref()
                    .is_none_or(|p| !p.contains_key(SYNC_PROPERTY))
            );
            assert!(matches!(
                named_record.transaction.as_ref().unwrap().operation,
                lance::dataset::transaction::Operation::Clone { ref_version: 1, .. }
            ));
            assert_ne!(
                named_record.transaction.as_ref().unwrap().uuid,
                record.transaction.as_ref().unwrap().uuid,
                "named event belongs to its actual Clone"
            );
        }
        assert_eq!(publication.named.is_some(), named);
    }
    // A rejected late frame must not publish the earlier valid prefix.
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
    early_writer(&f, ctx.snapshot()).await?;
    let source = capture(&f, ctx.snapshot());
    let rows = h::prepare(ctx.snapshot(), &[managed()], &[], hlimits())?;
    let image = crate::knowledge_first_publication::encode_native_knowledge_source_batch(
        ctx.snapshot(),
        &write,
        &rows,
        &decisions(),
        write_limits(),
    )?;
    let input = Box::pin(RecordBatchStreamAdapter::new(
        index::physical_schema(),
        futures::stream::iter(vec![
            Ok(image),
            Err(DataFusionError::Execution("late input".into())),
        ]),
    ));
    let error = publish_native_knowledge_source_stream(
        ctx.snapshot(),
        &read,
        &write,
        &source,
        input,
        write_limits(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.effect(),
        KnowledgeSourcePublicationEffect::BeforeCommit
    );
    assert!(
        !f.project
            .runtime_root()
            .join("knowledge/index.lance/_versions")
            .exists()
    );
    Ok(())
}
