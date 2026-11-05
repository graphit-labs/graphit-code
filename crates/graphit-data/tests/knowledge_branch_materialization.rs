//! Real native admissions, canonical18 EOF and observed branch-reference creation.
#[path = "support/knowledge_reference_fixture.rs"]
mod support;
use arrow_array::{
    Array, RecordBatch, StringArray, StructArray, UInt8Array, builder::FixedSizeBinaryBuilder,
};
use futures::TryStreamExt;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::{
    artifact_store::ArtifactStore,
    file_lock::Cancellation,
    project_paths::{InstallationPaths, ProjectPaths},
    version_context::{Checkout, VersionContext},
};
use graphit_data::{
    knowledge_branch_materialization::*, knowledge_document_projection as h,
    knowledge_document_update::*, knowledge_index::*, knowledge_reference_query::*,
    knowledge_references::*, lance_branch_store::BranchSnapshot,
};
use std::{
    collections::BTreeMap,
    error::Error,
    io,
    time::{Duration, Instant},
};
use support::*;
fn limits() -> KnowledgeDocumentUpdateLimits {
    KnowledgeDocumentUpdateLimits {
        read: read_limits(),
        write: write_limits(),
    }
}
fn branch(f: &Fixture, name: &str) -> VersionContext {
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
        name,
        None,
        "",
    )
    .unwrap()
}
fn generation(uri: &str, pin: &BranchSnapshot) -> KnowledgeGeneration {
    KnowledgeGeneration {
        corpus_uri: uri.into(),
        version: pin.version(),
        physical_branch: pin.physical_branch().map(str::to_owned),
        inherited_target: pin.inherited_target().map(str::to_owned),
    }
}
async fn image(pin: &BranchSnapshot) -> Result<BTreeMap<String, RecordBatch>, Box<dyn Error>> {
    let mut scanner = pin.scan();
    scanner.batch_size(1);
    let mut stream = scanner.try_into_stream().await?;
    let mut rows = BTreeMap::new();
    while let Some(batch) = stream.try_next().await? {
        let slugs = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for row in 0..batch.num_rows() {
            assert!(
                rows.insert(slugs.value(row).to_owned(), batch.slice(row, 1))
                    .is_none()
            );
        }
    }
    Ok(rows)
}
#[tokio::test]
async fn mixed_full18_materializes_observed_pin_preserves_parent_then_public_update_is_independent()
-> Result<(), Box<dyn Error>> {
    let cancel = Cancellation::default();
    let ctx = request_deadline(&cancel, Instant::now() + Duration::from_secs(600));
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    let docs = [page("a", &["a", "b", "", "a"]), page("removed", &[])];
    let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits())?;
    let ns = KnowledgeReferenceNamespace {
        scope: KnowledgeReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW808".parse()?),
        context: None,
    };
    let authored = [KnowledgeAuthoredReference {
        target_kind: KnowledgeReferenceTargetKind::Memory,
        scope: KnowledgeReferenceScope::User("owner β"),
        id: "literal",
        context: None,
        relation: "supports",
        field: Some(""),
    }];
    let fields = [
        KnowledgeReferenceFields {
            slug: "a",
            state: KnowledgeDocumentState::Active,
            namespace: ns,
            authored: &authored,
        },
        KnowledgeReferenceFields {
            slug: "removed",
            state: KnowledgeDocumentState::Removed,
            namespace: ns,
            authored: &[],
        },
    ];
    let refs =
        prepare_knowledge_references(ctx.snapshot(), &rows, &fields, write_limits().content)?;
    let first = publish_native_authored_with_references(
        ctx.snapshot(),
        &write,
        &rows,
        &refs,
        write_limits(),
    )
    .await?
    .default;
    let initial = BranchSnapshot::capture_observed(&first.corpus_uri, None, None).await?;
    let mut all = image(&initial).await?;
    // Seed a real ordinary None row without depending on the new firstpub leaf.
    let unknown = Fixture::new(&ctx);
    let issuer2 = unknown.issuer(&ctx);
    let caller2 = issuer2.authenticate_runtime(&unknown.secret(), &ctx)?;
    let auth2 = ProjectRecordAuthority::new(&issuer2);
    let write2 = auth2.admit_authenticated_write(
        &caller2,
        &ctx,
        unknown.scope(),
        ProjectWriteOperation::Knowledge,
    )?;
    let b = h::prepare(ctx.snapshot(), &[page("b", &["a"])], &[], hlimits())?;
    let bfirst = publish_native_authored(ctx.snapshot(), &write2, &b, write_limits())
        .await?
        .default;
    let bpin = BranchSnapshot::capture_observed(&bfirst.corpus_uri, None, None).await?;
    all.extend(image(&bpin).await?);
    let committed = initial
        .prepare_overwrite(all.values().cloned().collect())
        .await?
        .commit()
        .await?;
    let parent = generation(&first.corpus_uri, &committed);
    let before = image(&committed).await?;
    assert!(before["b"].column(17).is_null(0));
    assert_eq!(
        before["removed"]
            .column(16)
            .as_any()
            .downcast_ref::<UInt8Array>()
            .unwrap()
            .value(0),
        KnowledgeDocumentState::Removed as u8
    );
    let selected = branch(&f, "feature β");
    let encoded = selected.lance_branch();
    let scope = f.scope_for(&selected);
    let read = auth.admit_authenticated_read(&caller, &ctx, scope, ProjectReadModule::Knowledge)?;
    let write =
        auth.admit_authenticated_write(&caller, &ctx, scope, ProjectWriteOperation::Knowledge)?;
    let inherited_pin =
        BranchSnapshot::capture_observed(&parent.corpus_uri, Some(&encoded), None).await?;
    let expected = generation(&parent.corpus_uri, &inherited_pin);
    let KnowledgeBranchMaterialization::Materialized {
        generation: actual,
        inherited,
    } = materialize_native_knowledge_branch(ctx.snapshot(), &read, &write, &expected, limits())
        .await?
    else {
        panic!()
    };
    assert_eq!(inherited, expected);
    assert_eq!(actual.version, parent.version);
    assert_eq!(actual.physical_branch.as_deref(), Some(encoded.as_str()));
    assert_eq!(actual.inherited_target, None);
    let named = BranchSnapshot::capture_observed(&parent.corpus_uri, Some(&encoded), None).await?;
    assert_eq!(image(&named).await?, before);
    assert_eq!(image(&committed).await?, before);
    assert_eq!(image(&inherited_pin).await?, before);
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
    assert!(!refs.coverage.knowledge_source_complete);
    assert_eq!(refs.warnings.len(), 1);
    assert_eq!(refs.outgoing.unwrap().total, 4);
    assert!(
        matches!(materialize_native_knowledge_branch(ctx.snapshot(),&read,&write,&actual,limits()).await?,KnowledgeBranchMaterialization::Unchanged{generation} if generation==actual)
    );
    let mut changed = page("b", &["a"]);
    changed.body = Some("named-only publication");
    let rows = h::prepare(ctx.snapshot(), &[changed], &[], hlimits())?;
    let result = update_existing_native_knowledge_documents(
        ctx.snapshot(),
        &read,
        &write,
        &actual,
        &rows,
        &[KnowledgeDocumentReferenceUpdate {
            slug: "b",
            authored: KnowledgeDocumentAuthoredUpdate::Preserve,
        }],
        limits(),
    )
    .await?;
    assert!(matches!(
        result,
        KnowledgeDocumentUpdateResult::Updated { .. }
    ));
    let fresh = BranchSnapshot::capture_observed(&parent.corpus_uri, None, None).await?;
    assert_eq!(fresh.version(), parent.version);
    assert_eq!(image(&fresh).await?, before);
    assert_eq!(image(&named).await?, before);
    assert_eq!(image(&committed).await?, before);
    Ok(())
}
#[tokio::test]
async fn no_corpus_true_empty_and_stale_expected_are_checked_even_for_noop()
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
    let expected = KnowledgeGeneration {
        corpus_uri: "ordinary observation".into(),
        version: 1,
        physical_branch: None,
        inherited_target: None,
    };
    assert!(matches!(
        materialize_native_knowledge_branch(ctx.snapshot(), &read, &write, &expected, limits())
            .await,
        Err(KnowledgeBranchMaterializationError::NoCorpus)
    ));
    assert!(
        !f.project
            .runtime_root()
            .join("knowledge/index.lance")
            .exists()
    );
    let rows = h::prepare(ctx.snapshot(), &[], &[], hlimits())?;
    let first = publish_native_authored(ctx.snapshot(), &write, &rows, write_limits())
        .await?
        .default;
    assert!(
        matches!(materialize_native_knowledge_branch(ctx.snapshot(),&read,&write,&first,limits()).await?,KnowledgeBranchMaterialization::Unchanged{generation} if generation==first)
    );
    for stale in [
        KnowledgeGeneration {
            version: first.version + 1,
            ..first.clone()
        },
        KnowledgeGeneration {
            corpus_uri: "wrong".into(),
            ..first.clone()
        },
        KnowledgeGeneration {
            physical_branch: Some("wrong".into()),
            ..first.clone()
        },
        KnowledgeGeneration {
            inherited_target: Some("wrong".into()),
            ..first.clone()
        },
    ] {
        assert!(
            matches!(materialize_native_knowledge_branch(ctx.snapshot(),&read,&write,&stale,limits()).await,
            Err(KnowledgeBranchMaterializationError::GenerationChanged{observed}) if observed==first)
        );
    }
    let selected = branch(&f, "empty");
    let name = selected.lance_branch();
    let scope = f.scope_for(&selected);
    let read = auth.admit_authenticated_read(&caller, &ctx, scope, ProjectReadModule::Knowledge)?;
    let write =
        auth.admit_authenticated_write(&caller, &ctx, scope, ProjectWriteOperation::Knowledge)?;
    let expected = generation(
        &first.corpus_uri,
        &BranchSnapshot::capture_observed(&first.corpus_uri, Some(&name), None).await?,
    );
    let copy = graphit_core::artifact_store::CopyLimits::new(1, 1, 1, 1)?;
    let mut pass_through = limits();
    pass_through.write.artifacts = graphit_core::artifact_store::ArtifactLimits::new(1, 1, copy)?;

    let KnowledgeBranchMaterialization::Materialized { generation, .. } =
        materialize_native_knowledge_branch(ctx.snapshot(), &read, &write, &expected, pass_through)
            .await?
    else {
        panic!()
    };
    assert!(
        image(&BranchSnapshot::capture_observed(&first.corpus_uri, Some(&name), None).await?)
            .await?
            .is_empty()
    );
    assert_eq!(generation.version, first.version);
    Ok(())
}
#[tokio::test]
async fn complete_eof_rejects_late_corruption_duplicate_removed_and_exact_limits()
-> Result<(), Box<dyn Error>> {
    for damage in ["witness", "state", "removed_refs", "duplicate", "none"] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
        let auth = ProjectRecordAuthority::new(&issuer);
        let write = auth.admit_authenticated_write(
            &caller,
            &ctx,
            f.scope(),
            ProjectWriteOperation::Knowledge,
        )?;
        let raw = "é".repeat(32);
        let targets = [raw.as_str(), "", "a"];
        let docs = [page("a", &targets), page("last", &[])];
        let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits())?;
        let ns = KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW808".parse()?),
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
                slug: "last",
                state: KnowledgeDocumentState::Active,
                namespace: ns,
                authored: &[],
            },
        ];
        let refs =
            prepare_knowledge_references(ctx.snapshot(), &rows, &fields, write_limits().content)?;
        let first = publish_native_authored_with_references(
            ctx.snapshot(),
            &write,
            &rows,
            &refs,
            write_limits(),
        )
        .await?
        .default;
        let pin = BranchSnapshot::capture_observed(&first.corpus_uri, None, None).await?;
        let mut rows = image(&pin).await?;
        if damage != "none" {
            let key = if damage == "removed_refs" {
                "a"
            } else {
                "last"
            };
            let last = &rows[key];
            let mut columns = last.columns().to_vec();
            match damage {
                "witness" => {
                    let projection = columns[17].as_any().downcast_ref::<StructArray>().unwrap();
                    let mut fields = projection.columns().to_vec();
                    let mut bad = FixedSizeBinaryBuilder::new(32);
                    bad.append_value([0; 32])?;
                    fields[4] = std::sync::Arc::new(bad.finish());
                    columns[17] = std::sync::Arc::new(StructArray::new(
                        projection.fields().clone(),
                        fields,
                        projection.nulls().cloned(),
                    ));
                }
                "state" => columns[16] = std::sync::Arc::new(UInt8Array::from(vec![3])),
                "removed_refs" => {
                    columns[16] = std::sync::Arc::new(UInt8Array::from(vec![
                        KnowledgeDocumentState::Removed as u8,
                    ]))
                }
                _ => columns[0] = std::sync::Arc::new(StringArray::from(vec!["a"])),
            }
            rows.insert(
                key.into(),
                RecordBatch::try_new(physical_schema(), columns)?,
            );
            pin.prepare_overwrite(rows.values().cloned().collect())
                .await?
                .commit()
                .await?;
        }
        let selected = branch(&f, damage);
        let name = selected.lance_branch();
        let scope = f.scope_for(&selected);
        let read =
            auth.admit_authenticated_read(&caller, &ctx, scope, ProjectReadModule::Knowledge)?;
        let write =
            auth.admit_authenticated_write(&caller, &ctx, scope, ProjectWriteOperation::Knowledge)?;
        let expected = generation(
            &first.corpus_uri,
            &BranchSnapshot::capture_observed(&first.corpus_uri, Some(&name), None).await?,
        );
        if damage == "none" {
            let mut below = limits();
            below.read.content.max_identifier_bytes = n(63);
            assert!(matches!(
                materialize_native_knowledge_branch(
                    ctx.snapshot(),
                    &read,
                    &write,
                    &expected,
                    below
                )
                .await,
                Err(KnowledgeBranchMaterializationError::Data(
                    KnowledgeIndexError::LimitExceeded("identifier_bytes")
                ))
            ));
            assert!(
                BranchSnapshot::capture_observed(&first.corpus_uri, Some(&name), None)
                    .await?
                    .inherited_target()
                    .is_some()
            );
            let mut l = limits();
            l.read.content.max_identifier_bytes = n(64);
            materialize_native_knowledge_branch(ctx.snapshot(), &read, &write, &expected, l)
                .await?;
        } else {
            let error = materialize_native_knowledge_branch(
                ctx.snapshot(),
                &read,
                &write,
                &expected,
                limits(),
            )
            .await
            .unwrap_err();
            assert!(matches!(
                error,
                KnowledgeBranchMaterializationError::Data(_)
            ));
            assert!(
                BranchSnapshot::capture_observed(&first.corpus_uri, Some(&name), None)
                    .await?
                    .inherited_target()
                    .is_some()
            );
        }
    }
    Ok(())
}
#[tokio::test]
async fn genuine_admissions_scopes_revoke_serialization_and_wait_cancellation()
-> Result<(), Box<dyn Error>> {
    use graphit_auth::{AuthError, Permission};
    use graphit_core::module_settings::Module;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer_permissions(
        &ctx,
        &[
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
            Permission::Read(Module::Task),
            Permission::WriteMemory,
            Permission::ManageAuthProfiles,
            Permission::ManageAccessGrants,
            Permission::Delegate,
        ],
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let admin = issuer.bootstrap_admin(&caller)?;
    let parent = issuer.admin_parent(&caller, &admin)?;
    let (bearer, reference) = f.issue(&issuer, &parent);
    let grant = issuer.verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let read = auth.admit_read(&grant, &ctx, ProjectReadModule::Knowledge)?;
    let write = auth.admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)?;
    let rows = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
    let first = publish_native_authored(ctx.snapshot(), &write, &rows, write_limits())
        .await?
        .default;
    let wrong_read =
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)?;
    assert!(matches!(
        materialize_native_knowledge_branch(ctx.snapshot(), &wrong_read, &write, &first, limits())
            .await,
        Err(KnowledgeBranchMaterializationError::Data(
            KnowledgeIndexError::WrongAdmission
        ))
    ));
    let wrong_write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Memory)?;
    assert!(matches!(
        materialize_native_knowledge_branch(ctx.snapshot(), &read, &wrong_write, &first, limits())
            .await,
        Err(KnowledgeBranchMaterializationError::Data(
            KnowledgeIndexError::WrongAdmission
        ))
    ));
    let other_cancel = Cancellation::default();
    let other = request(&other_cancel);
    assert!(matches!(
        materialize_native_knowledge_branch(other.snapshot(), &read, &write, &first, limits())
            .await,
        Err(KnowledgeBranchMaterializationError::Data(
            KnowledgeIndexError::DifferentRequest
        ))
    ));
    let foreign = Fixture::new(&ctx);
    let foreign_issuer = foreign.issuer(&ctx);
    let foreign_caller = foreign_issuer.authenticate_runtime(&foreign.secret(), &ctx)?;
    let foreign_auth = ProjectRecordAuthority::new(&foreign_issuer);
    let foreign_read = foreign_auth.admit_authenticated_read(
        &foreign_caller,
        &ctx,
        foreign.scope(),
        ProjectReadModule::Knowledge,
    )?;
    assert!(matches!(
        materialize_native_knowledge_branch(
            ctx.snapshot(),
            &foreign_read,
            &write,
            &first,
            limits()
        )
        .await,
        Err(KnowledgeBranchMaterializationError::ScopeMismatch)
    ));
    let install = InstallationPaths::resolve(
        f.project.brand().clone(),
        f.project.owner_root(),
        None,
        Some(f.realm().live.owner_root()),
    )?;
    let paths = ProjectPaths::at(f.project.owner_root(), &install)?;
    let other_project = graphit_core::artifact_store::CapturedPublicationScope::project(
        &paths,
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse()?,
        graphit_core::artifact_store::ScopeLimits::new(16, 4096, 128)?,
        ctx.snapshot(),
    )?;
    let other_record = other_project.binding()?;
    let mut other_scope = f.scope();
    other_scope.project = Some(graphit_auth::ProjectCapture {
        scope: graphit_auth::ScopeCapture {
            live: &other_project,
            record: &other_record,
        },
        version: f.scope().project.unwrap().version,
    });
    let different_id_read =
        auth.admit_authenticated_read(&caller, &ctx, other_scope, ProjectReadModule::Knowledge)?;
    assert!(matches!(
        materialize_native_knowledge_branch(
            ctx.snapshot(),
            &different_id_read,
            &write,
            &first,
            limits()
        )
        .await,
        Err(KnowledgeBranchMaterializationError::ScopeMismatch)
    ));
    let selected = branch(&f, "serial");
    let name = selected.lance_branch();
    let scope = f.scope_for(&selected);
    let named_read =
        auth.admit_authenticated_read(&caller, &ctx, scope, ProjectReadModule::Knowledge)?;
    let named_write =
        auth.admit_authenticated_write(&caller, &ctx, scope, ProjectWriteOperation::Knowledge)?;
    assert!(matches!(
        materialize_native_knowledge_branch(ctx.snapshot(), &named_read, &write, &first, limits())
            .await,
        Err(KnowledgeBranchMaterializationError::ScopeMismatch)
    ));
    let expected = generation(
        &first.corpus_uri,
        &BranchSnapshot::capture_observed(&first.corpus_uri, Some(&name), None).await?,
    );
    let (a, b) = tokio::join!(
        materialize_native_knowledge_branch(
            ctx.snapshot(),
            &named_read,
            &named_write,
            &expected,
            limits()
        ),
        materialize_native_knowledge_branch(
            ctx.snapshot(),
            &named_read,
            &named_write,
            &expected,
            limits()
        )
    );
    assert!(matches!(
        (&a, &b),
        (
            Ok(KnowledgeBranchMaterialization::Materialized { .. }),
            Err(KnowledgeBranchMaterializationError::GenerationChanged { .. })
        ) | (
            Err(KnowledgeBranchMaterializationError::GenerationChanged { .. }),
            Ok(KnowledgeBranchMaterialization::Materialized { .. })
        )
    ));
    for permissions in [
        vec![Permission::Read(Module::Knowledge)],
        vec![Permission::MutateModule(Module::Knowledge)],
    ] {
        let run = issuer.start_run(&parent, &f.scope(), Duration::from_secs(900))?;
        let bearer = issuer.issue(&parent, &run, &reference, &permissions, &f.scope())?;
        let g = issuer.verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)?;
        if permissions == [Permission::Read(Module::Knowledge)] {
            assert!(
                auth.admit_write(&g, &ctx, ProjectWriteOperation::Knowledge)
                    .is_err()
            );
        } else {
            assert!(
                auth.admit_read(&g, &ctx, ProjectReadModule::Knowledge)
                    .is_err()
            );
        }
    }
    issuer.revoke(&parent, &bearer)?;
    let error =
        materialize_native_knowledge_branch(ctx.snapshot(), &read, &write, &first, limits())
            .await
            .unwrap_err();
    assert!(matches!(
        error,
        KnowledgeBranchMaterializationError::Data(KnowledgeIndexError::Auth(AuthError::Denied))
    ));
    assert!(error.source().is_some());
    let read =
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    let l = write_limits();
    let artifacts = ArtifactStore::new(f.project.clone(), l.artifacts);
    let held = artifacts.writer_with_context(ctx.snapshot())?;
    let (result, _) = tokio::join!(
        materialize_native_knowledge_branch(ctx.snapshot(), &read, &write, &first, limits()),
        async {
            tokio::time::sleep(Duration::from_millis(25)).await;
            cancel.cancel();
        }
    );
    drop(held);
    assert!(
        matches!(result,Err(KnowledgeBranchMaterializationError::Data(KnowledgeIndexError::Artifact(ref cause))) if cause.stage()==graphit_core::artifact_store::ArtifactStage::Lock&&cause.kind()==io::ErrorKind::Interrupted)
    );
    assert_eq!(
        BranchSnapshot::capture_observed(&first.corpus_uri, None, None)
            .await?
            .version(),
        first.version
    );
    Ok(())
}

#[tokio::test]
async fn schema16_is_rejected_and_native_capture_path_budgets_apply_to_each_admission()
-> Result<(), Box<dyn Error>> {
    use graphit_core::artifact_store::ProjectRecordDirectory;
    use graphit_core::native_document_file::NativeDocumentFileLimits;
    use graphit_data::lance_module_store::LanceModuleStore;
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
    let l = write_limits();
    let artifacts = ArtifactStore::new(f.project.clone(), l.artifacts);
    let mut writer = artifacts.writer_with_context(ctx.snapshot())?;
    let guard = writer.create_record_directory(ProjectRecordDirectory::Knowledge, l.native)?;
    guard.revalidate(ctx.snapshot())?;
    let path = f.project.runtime_root().join("knowledge/index.lance");
    let uri = path.to_str().unwrap().to_owned();
    let full = physical_schema();
    let legacy = std::sync::Arc::new(arrow_schema::Schema::new(full.fields()[..16].to_vec()));
    let batch = RecordBatch::new_empty(std::sync::Arc::clone(&legacy));
    let store = LanceModuleStore::create(&uri, legacy, vec![batch]).await?;
    let version = store.snapshot().version();
    drop(store);
    drop(writer);
    let expected = KnowledgeGeneration {
        corpus_uri: uri,
        version,
        physical_branch: None,
        inherited_target: None,
    };
    let error =
        materialize_native_knowledge_branch(ctx.snapshot(), &read, &write, &expected, limits())
            .await
            .unwrap_err();
    assert!(matches!(
        error,
        KnowledgeBranchMaterializationError::Data(KnowledgeIndexError::InvalidField("schema"))
    ));
    assert!(error.source().is_some());
    // Another real canonical18 corpus is used to isolate Native capture budgets.
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let read =
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    let rows = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
    let expected = publish_native_authored(ctx.snapshot(), &write, &rows, write_limits())
        .await?
        .default;
    for side in [0, 1] {
        let (mut lo, mut hi) = (1usize, 65536usize);
        while lo < hi {
            let mid = (lo + hi) / 2;
            let cap = NativeDocumentFileLimits::new(65536, mid, 256, 65536)?;
            let mut l = limits();
            if side == 0 {
                l.read.native = cap;
            } else {
                l.write.native = cap;
            }
            match materialize_native_knowledge_branch(ctx.snapshot(), &read, &write, &expected, l)
                .await
            {
                Ok(KnowledgeBranchMaterialization::Unchanged { .. }) => hi = mid,
                Err(KnowledgeBranchMaterializationError::Data(KnowledgeIndexError::Request(
                    ref e,
                ))) if e.kind() == io::ErrorKind::FileTooLarge => lo = mid + 1,
                Err(KnowledgeBranchMaterializationError::Data(KnowledgeIndexError::Artifact(
                    ref e,
                ))) if e.kind() == io::ErrorKind::FileTooLarge => lo = mid + 1,
                other => panic!("unexpected Native budget outcome: {other:?}"),
            }
        }
        let mut l = limits();
        let exact = NativeDocumentFileLimits::new(65536, lo, 256, 65536)?;
        if side == 0 {
            l.read.native = exact;
        } else {
            l.write.native = exact;
        }
        assert!(matches!(
            materialize_native_knowledge_branch(ctx.snapshot(), &read, &write, &expected, l)
                .await?,
            KnowledgeBranchMaterialization::Unchanged { .. }
        ));
        let below = NativeDocumentFileLimits::new(65536, lo - 1, 256, 65536)?;
        if side == 0 {
            l.read.native = below;
        } else {
            l.write.native = below;
        }
        assert!(
            materialize_native_knowledge_branch(ctx.snapshot(), &read, &write, &expected, l)
                .await
                .is_err()
        );
    }
    assert_eq!(
        BranchSnapshot::capture_observed(&expected.corpus_uri, None, None)
            .await?
            .version(),
        expected.version
    );
    Ok(())
}
