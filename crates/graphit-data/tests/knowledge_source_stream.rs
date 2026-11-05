//! Ordinary FULL18 streams under real PROJECT grants, Source and SDK effects.
#[allow(dead_code)] // Shared fixture also supports read-query targets.
#[path = "support/knowledge_reference_fixture.rs"]
mod support;
use arrow_array::{Array, RecordBatch, StringArray, UInt8Array};
use futures::{StreamExt, TryStreamExt, stream};
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::{
    artifact_store::{
        ArtifactStore, CapturedSourceReader, ProjectIgnoreCriteria, ProjectSelectionCriteria,
        ProjectSelectionLimits, SourceReadLimits,
    },
    file_io::SnapshotContext,
    file_lock::Cancellation,
};
use graphit_data::{
    knowledge_document_lifecycle::*,
    knowledge_document_projection as h,
    knowledge_document_update::*,
    knowledge_first_publication::*,
    knowledge_index::*,
    knowledge_references::*,
    lance_branch_store::BranchSnapshot,
    lance_module_store::{
        DataFusionError, LanceModuleStore, RecordBatchStreamAdapter, Resolution,
        SendableRecordBatchStream,
    },
};
use std::{error::Error, io, path::Path, sync::Arc, time::Duration};
use support::*;

async fn capture(
    f: &Fixture,
    original: &Arc<SnapshotContext>,
) -> Result<CapturedSourceReader, Box<dyn Error>> {
    let artifacts = ArtifactStore::new(f.project.clone(), write_limits().artifacts);
    let request = Arc::clone(original);
    drop(tokio::task::spawn_blocking(move || artifacts.writer_with_context(&request)).await??);
    let root = f.project.owner_root();
    std::fs::create_dir_all(root.join("docs"))?;
    for name in ["a", "b", "c"] {
        std::fs::write(
            root.join(format!("docs/{name}.md")),
            page(name, &[]).body.unwrap(),
        )?;
    }
    let protected = format!("{}/", f.project.brand().directory());
    Ok(CapturedSourceReader::capture_selected_project_metadata(
        &f.project,
        ProjectSelectionCriteria {
            subtree: Some(Path::new("docs")),
            extras: &[],
            include_readme: false,
            extensions: &[".md"],
            ignore: ProjectIgnoreCriteria {
                protected_defaults: &[&protected],
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
        SourceReadLimits::new(4096, 4096, 65536)?,
        original,
    )?)
}
fn frames(items: Vec<Result<RecordBatch, DataFusionError>>) -> SendableRecordBatchStream {
    Box::pin(RecordBatchStreamAdapter::new(
        native_knowledge_source_schema(),
        stream::iter(items),
    ))
}
fn batch(
    original: &Arc<SnapshotContext>,
    write: &graphit_auth::ProjectWriteAdmission<'_>,
    slug: &str,
    source_name: &str,
    authored: Option<&[KnowledgeAuthoredReference<'_>]>,
) -> Result<RecordBatch, Box<dyn Error>> {
    let relative = format!("docs/{source_name}.md");
    let identity =
        KnowledgeSourceIdentity::from_relative_path(Path::new(&relative), Some(slug), n(4096))?;
    let mut fields = page(slug, &["", "missing", "missing"]);
    fields.source_path = identity.source_path();
    let rows = h::prepare(original, &[fields], &[], hlimits())?;
    Ok(encode_native_knowledge_source_batch(
        original,
        write,
        &rows,
        &[KnowledgeDocumentReferenceUpdate {
            slug,
            authored: match authored {
                None => KnowledgeDocumentAuthoredUpdate::Preserve,
                Some(refs) => KnowledgeDocumentAuthoredUpdate::Replace(refs),
            },
        }],
        write_limits(),
    )?)
}
async fn no_publication(f: &Fixture) {
    let uri = url::Url::from_file_path(f.project.runtime_root().join("knowledge/index.lance"))
        .unwrap()
        .to_string();
    assert!(
        LanceModuleStore::open(&uri, native_knowledge_source_schema())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn three_owned_batches_and_explicit_empty_publish_default_named_and_real_keys()
-> Result<(), Box<dyn Error>> {
    for version in ["", "feature Ω"] {
        for empty in [false, true] {
            let cancel = Cancellation::default();
            let ctx = request(&cancel);
            let f = Fixture::with_version(&ctx, version);
            let issuer = f.issuer(&ctx);
            let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
            let auth = ProjectRecordAuthority::new(&issuer);
            let read = auth.admit_authenticated_read(
                &caller,
                &ctx,
                f.scope(),
                ProjectReadModule::Knowledge,
            )?;
            let write = auth.admit_authenticated_write(
                &caller,
                &ctx,
                f.scope(),
                ProjectWriteOperation::Knowledge,
            )?;
            let source = capture(&f, ctx.snapshot()).await?;
            assert_eq!(source.retained_bytes(), 0);
            let refs = [KnowledgeAuthoredReference {
                target_kind: KnowledgeReferenceTargetKind::Knowledge,
                scope: KnowledgeReferenceScope::User("owner β"),
                id: " literal ID ",
                context: Some("ctx β"),
                relation: " rel β ",
                field: Some(""),
            }];
            let batches = if empty {
                vec![Ok(RecordBatch::new_empty(native_knowledge_source_schema()))]
            } else {
                vec![
                    Ok(batch(ctx.snapshot(), &write, "a", "a", None)?),
                    Ok(batch(ctx.snapshot(), &write, "b", "b", Some(&[]))?),
                    Ok(batch(ctx.snapshot(), &write, "c", "c", Some(&refs))?),
                ]
            };
            let (publication, key) = publish_native_knowledge_source_stream(
                ctx.snapshot(),
                &read,
                &write,
                &source,
                frames(batches),
                write_limits(),
            )
            .await?;
            assert_eq!(publication.named.is_some(), !version.is_empty());
            let selected = publication.named.as_ref().unwrap_or(&publication.default);
            let store = LanceModuleStore::open(
                &publication.default.corpus_uri,
                native_knowledge_source_schema(),
            )
            .await?;
            assert_eq!(
                store.resolve(&key).await?,
                Resolution::Applied {
                    version: publication.default.version
                }
            );
            let pin = BranchSnapshot::capture_observed(
                &selected.corpus_uri,
                selected.physical_branch.as_deref(),
                None,
            )
            .await?;
            let mut scan = pin.scan();
            scan.batch_size(1);
            let mut rows = scan.try_into_stream().await?;
            let mut count = 0;
            while let Some(row) = rows.try_next().await? {
                assert_eq!(row.num_columns(), 18);
                if row.num_rows() == 0 {
                    continue;
                }
                assert_eq!(
                    row.column(16)
                        .as_any()
                        .downcast_ref::<UInt8Array>()
                        .unwrap()
                        .value(0),
                    KnowledgeDocumentState::Active as u8
                );
                let slug = row
                    .column(0)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .unwrap()
                    .value(0);
                let projection = row
                    .column(17)
                    .as_any()
                    .downcast_ref::<arrow_array::StructArray>()
                    .unwrap();
                assert_eq!(projection.is_null(0), slug == "a");
                count += row.num_rows();
            }
            assert_eq!(count, if empty { 0 } else { 3 });
        }
    }
    Ok(())
}

#[tokio::test]
async fn late_full18_integrity_failures_and_global_duplicates_never_publish_prefix()
-> Result<(), Box<dyn Error>> {
    for case in 0..6 {
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
        let source = capture(&f, ctx.snapshot()).await?;
        let first = batch(ctx.snapshot(), &write, "a", "a", Some(&[]))?;
        let tail = match case {
            0 => first.clone(),
            1 => batch(ctx.snapshot(), &write, "b", "a", None)?,
            _ => batch(ctx.snapshot(), &write, "b", "b", Some(&[]))?,
        };
        let tail = match case {
            2 => {
                let mut columns = tail.columns().to_vec();
                columns[16] = Arc::new(UInt8Array::from(vec![
                    KnowledgeDocumentState::Removed as u8,
                ]));
                RecordBatch::try_new(tail.schema(), columns)?
            }
            3 => {
                let mut columns = tail.columns().to_vec();
                columns[0] = Arc::new(StringArray::from(vec!["forged witness"]));
                RecordBatch::try_new(tail.schema(), columns)?
            }
            4 => {
                let mut columns = tail.columns().to_vec();
                columns[10] = Arc::new(StringArray::from(vec!["wrong recipe"]));
                RecordBatch::try_new(tail.schema(), columns)?
            }
            5 => {
                let schema = Arc::new(arrow_schema::Schema::new(vec![arrow_schema::Field::new(
                    "wrong",
                    arrow_schema::DataType::Utf8,
                    false,
                )]));
                RecordBatch::try_new(schema, vec![Arc::new(StringArray::from(vec!["x"]))])?
            }
            _ => tail,
        };
        let failed = publish_native_knowledge_source_stream(
            ctx.snapshot(),
            &read,
            &write,
            &source,
            frames(vec![Ok(first), Ok(tail)]),
            write_limits(),
        )
        .await
        .unwrap_err();
        assert_eq!(failed.phase(), KnowledgeSourcePublicationPhase::Image);
        assert_eq!(
            failed.effect(),
            KnowledgeSourcePublicationEffect::BeforeCommit
        );
        assert!(failed.known_publication().is_none());
        assert!(failed.recovery_key().is_none());
        if case == 0 {
            assert!(matches!(failed.cause(), KnowledgeIndexError::DuplicateSlug));
        }
        if case == 1 {
            assert!(matches!(
                failed.cause(),
                KnowledgeIndexError::InvalidField("duplicate_source_path")
            ));
        }
        no_publication(&f).await;
    }
    Ok(())
}

#[tokio::test]
async fn actual_tail_file_error_keeps_concrete_io_cause_across_sdk_abort()
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
    let source = capture(&f, ctx.snapshot()).await?;
    let first = batch(ctx.snapshot(), &write, "a", "a", None)?;
    let path = f.project.owner_root().join("docs/c.md");
    let input = Box::pin(RecordBatchStreamAdapter::new(
        native_knowledge_source_schema(),
        stream::once(async { Ok(first) }).chain(stream::once(async move {
            std::fs::remove_file(&path).unwrap();
            let cause = std::fs::read(&path).unwrap_err();
            Err(DataFusionError::External(Box::new(cause)))
        })),
    ));
    let failed = publish_native_knowledge_source_stream(
        ctx.snapshot(),
        &read,
        &write,
        &source,
        input,
        write_limits(),
    )
    .await
    .unwrap_err();
    let KnowledgeIndexError::InputStream(cause) = failed.cause() else {
        panic!("{failed:?}")
    };
    assert_eq!(
        cause
            .source()
            .unwrap()
            .downcast_ref::<io::Error>()
            .unwrap()
            .kind(),
        io::ErrorKind::NotFound
    );
    assert_eq!(
        failed.effect(),
        KnowledgeSourcePublicationEffect::BeforeCommit
    );
    no_publication(&f).await;
    Ok(())
}

#[tokio::test]
async fn pending_input_observes_original_cancellation_real_revocation_and_deadline()
-> Result<(), Box<dyn Error>> {
    for case in 0..3 {
        let revoke = case == 1;
        // Set up the real role before starting the short operation deadline.
        let setup_cancel = Cancellation::default();
        let setup = request(&setup_cancel);
        let f = Fixture::new(&setup);
        let issuer = f.issuer(&setup);
        let caller = issuer.authenticate_runtime(&f.secret(), &setup)?;
        let admin = issuer.bootstrap_admin(&caller)?;
        let parent = issuer.admin_parent(&caller, &admin)?;
        let (bearer, credential) = f.issue(&issuer, &parent);
        let cancel = Cancellation::default();
        let ctx = if case == 2 {
            request_deadline(&cancel, std::time::Instant::now() + Duration::from_secs(1))
        } else {
            request(&cancel)
        };
        let grant = issuer.verify(bearer.expose_for_transport(), &credential, f.scope(), &ctx)?;
        let auth = ProjectRecordAuthority::new(&issuer);
        let read = auth.admit_read(&grant, &ctx, ProjectReadModule::Knowledge)?;
        let write = auth.admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)?;
        let source = capture(&f, ctx.snapshot()).await?;
        let (entered, waiting) = tokio::sync::oneshot::channel();
        let mut entered = Some(entered);
        let input = Box::pin(RecordBatchStreamAdapter::new(
            native_knowledge_source_schema(),
            stream::poll_fn(move |_| {
                if let Some(entered) = entered.take() {
                    let _ = entered.send(());
                }
                std::task::Poll::<Option<Result<RecordBatch, DataFusionError>>>::Pending
            }),
        ));
        let publish = publish_native_knowledge_source_stream(
            ctx.snapshot(),
            &read,
            &write,
            &source,
            input,
            write_limits(),
        );
        let change = async {
            waiting
                .await
                .expect("producer must be polled before interruption");
            if case == 2 {
                return;
            }
            if revoke {
                issuer.revoke(&parent, &bearer).unwrap()
            } else {
                cancel.cancel()
            }
        };
        let (result, ()) = tokio::join!(publish, change);
        let failed = result.unwrap_err();
        assert_eq!(
            failed.effect(),
            KnowledgeSourcePublicationEffect::BeforeCommit
        );
        if revoke {
            assert!(matches!(
                failed.cause(),
                KnowledgeIndexError::Auth(graphit_auth::AuthError::Denied)
            ));
        } else {
            let expected = if case == 2 {
                io::ErrorKind::TimedOut
            } else {
                io::ErrorKind::Interrupted
            };
            let mut cause: Option<&(dyn Error + 'static)> = Some(&failed);
            let mut found = None;
            while let Some(error) = cause {
                if let Some(io) = error.downcast_ref::<io::Error>() {
                    found = Some(io.kind());
                    break;
                }
                cause = error.source();
            }
            assert_eq!(found, Some(expected), "case {case}: {failed:?}");
        }
        assert!(failed.source().is_some());
        no_publication(&f).await;
    }
    Ok(())
}

#[tokio::test]
async fn sdk_consumer_path_failure_keeps_real_store_cause_without_broken_pipe_substitute()
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
    let source = capture(&f, ctx.snapshot()).await?;
    let first = batch(ctx.snapshot(), &write, "a", "a", None)?;
    let path = f.project.runtime_root().join("knowledge/index.lance");
    let input = Box::pin(RecordBatchStreamAdapter::new(
        native_knowledge_source_schema(),
        stream::once(async move {
            if path.is_dir() {
                std::fs::remove_dir_all(&path).unwrap();
            }
            std::fs::write(path, b"actual path obstruction").unwrap();
            Ok(first)
        }),
    ));
    let failed = publish_native_knowledge_source_stream(
        ctx.snapshot(),
        &read,
        &write,
        &source,
        input,
        write_limits(),
    )
    .await
    .unwrap_err();
    assert_eq!(failed.phase(), KnowledgeSourcePublicationPhase::Prepare);
    assert_eq!(
        failed.effect(),
        KnowledgeSourcePublicationEffect::BeforeCommit
    );
    assert!(
        matches!(failed.cause(), KnowledgeIndexError::Store(_)),
        "{failed:?}"
    );
    assert!(failed.source().unwrap().source().is_some());
    no_publication(&f).await;
    Ok(())
}

#[tokio::test]
async fn schema_and_foreign_original_are_rejected_before_record_directory_birth()
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
    let source = capture(&f, ctx.snapshot()).await?;
    let foreign = request(&cancel);
    let failed = publish_native_knowledge_source_stream(
        foreign.snapshot(),
        &read,
        &write,
        &source,
        frames(vec![]),
        write_limits(),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        failed.cause(),
        KnowledgeIndexError::DifferentRequest
    ));
    let input = Box::pin(RecordBatchStreamAdapter::new(
        Arc::new(arrow_schema::Schema::empty()),
        stream::empty(),
    ));
    let failed = publish_native_knowledge_source_stream(
        ctx.snapshot(),
        &read,
        &write,
        &source,
        input,
        write_limits(),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        failed.cause(),
        KnowledgeIndexError::InvalidField("schema")
    ));
    assert!(!f.project.runtime_root().join("knowledge").exists());
    let failed = publish_native_knowledge_source_stream(
        ctx.snapshot(),
        &read,
        &write,
        &source,
        frames(vec![]),
        write_limits(),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        failed.cause(),
        KnowledgeIndexError::InvalidField("stream_empty")
    ));
    assert_eq!(
        failed.effect(),
        KnowledgeSourcePublicationEffect::BeforeCommit
    );
    no_publication(&f).await;
    Ok(())
}
