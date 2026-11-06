//! Real first creation: sparse metadata, exact maps, independent grants and native effects.
use crate::knowledge_reference_fixture as support;
use arrow_array::{
    Array, FixedSizeBinaryArray, ListArray, RecordBatch, StringArray, StructArray, UInt8Array,
};
use futures::TryStreamExt;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::{artifact_store::ArtifactStore, file_lock::Cancellation};
use graphit_data::{
    knowledge_document_projection as h, knowledge_document_update::*,
    knowledge_first_publication::*, knowledge_index::*, knowledge_reference_query::*,
    knowledge_references::*, lance_branch_store::BranchSnapshot,
};
use std::{collections::BTreeMap, error::Error, io};
use support::*;

fn preserve(slug: &str) -> KnowledgeDocumentReferenceUpdate<'_> {
    KnowledgeDocumentReferenceUpdate {
        slug,
        authored: KnowledgeDocumentAuthoredUpdate::Preserve,
    }
}
fn replace<'a>(
    slug: &'a str,
    refs: &'a [KnowledgeAuthoredReference<'a>],
) -> KnowledgeDocumentReferenceUpdate<'a> {
    KnowledgeDocumentReferenceUpdate {
        slug,
        authored: KnowledgeDocumentAuthoredUpdate::Replace(refs),
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
fn projection(batch: &RecordBatch) -> &StructArray {
    batch.column(17).as_any().downcast_ref().unwrap()
}
fn entries() -> [KnowledgeAuthoredReference<'static>; 5] {
    let r = KnowledgeAuthoredReference {
        target_kind: KnowledgeReferenceTargetKind::Task,
        scope: KnowledgeReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap()),
        id: " literal ",
        context: None,
        relation: " supports ",
        field: Some(""),
    };
    [
        r,
        KnowledgeAuthoredReference {
            target_kind: KnowledgeReferenceTargetKind::Session,
            ..r
        },
        KnowledgeAuthoredReference {
            target_kind: KnowledgeReferenceTargetKind::Memory,
            scope: KnowledgeReferenceScope::User("owner β"),
            id: "line\nbreak",
            ..r
        },
        KnowledgeAuthoredReference {
            target_kind: KnowledgeReferenceTargetKind::Knowledge,
            context: Some("alias β"),
            id: "Café",
            ..r
        },
        KnowledgeAuthoredReference {
            target_kind: KnowledgeReferenceTargetKind::Knowledge,
            context: Some("alias β"),
            id: "Café",
            ..r
        },
    ]
}
#[tokio::test]
async fn mixed_first_image_preserves_full16_literal_order_duplicates_and_truthful_coverage()
-> Result<(), Box<dyn Error>> {
    for version in ["", "topic"] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::with_version(&ctx, version);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
        let auth = ProjectRecordAuthority::new(&issuer);
        let write = auth.admit_authenticated_write(
            &caller,
            &ctx,
            f.scope(),
            ProjectWriteOperation::Knowledge,
        )?;
        let docs = [
            page("unknown", &["unknown"]),
            page("empty", &[]),
            page("literal", &["literal", "", "empty", "literal"]),
        ];
        let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits())?;
        let entries = entries();
        let updates = [
            replace("literal", &entries),
            preserve("unknown"),
            replace("empty", &[]),
        ];
        let result = publish_native_authored_with_reference_updates(
            ctx.snapshot(),
            &write,
            &rows,
            &updates,
            write_limits(),
        )
        .await?;
        let default =
            BranchSnapshot::capture_observed(&result.default.corpus_uri, None, None).await?;
        let actual = image(&default).await?;
        assert_eq!(actual.len(), 3);
        assert_eq!(actual["literal"].num_columns(), 18);
        assert!(projection(&actual["unknown"]).is_null(0));
        assert!(!projection(&actual["empty"]).is_null(0));
        assert!(!projection(&actual["literal"]).is_null(0));
        for batch in actual.values() {
            assert_eq!(
                batch
                    .column(16)
                    .as_any()
                    .downcast_ref::<UInt8Array>()
                    .unwrap()
                    .value(0),
                KnowledgeDocumentState::Active as u8
            );
        }
        let projected = projection(&actual["literal"]);
        assert_eq!(
            projected
                .column(3)
                .as_any()
                .downcast_ref::<ListArray>()
                .unwrap()
                .value_length(0),
            5
        );
        assert_eq!(
            projected
                .column(4)
                .as_any()
                .downcast_ref::<FixedSizeBinaryArray>()
                .unwrap()
                .value(0)
                .len(),
            32
        );
        if let Some(named) = result.named {
            let pin = BranchSnapshot::capture_observed(
                &named.corpus_uri,
                named.physical_branch.as_deref(),
                None,
            )
            .await?;
            assert_eq!(image(&pin).await?, actual);
            assert_eq!(named.version, result.default.version);
        }
        // A fresh independent read, never inferred from the write, validates all fields/witnesses.
        let read =
            auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
        let KnowledgeIndexRead::Published { documents, .. } =
            read_native_knowledge_index(ctx.snapshot(), &read, read_limits()).await?
        else {
            panic!()
        };
        for input in &docs {
            let d = documents
                .iter()
                .find(|d| d.slug == input.slug.unwrap())
                .unwrap();
            assert_eq!(d.body, input.body.unwrap());
            assert_eq!(d.source_path, input.source_path);
            assert_eq!(d.compilation_digest, input.compilation_digest);
            assert_eq!(d.updated_at, input.updated_at);
            assert_eq!(d.recipe_parameters.fallback_title, "fallback");
            assert_eq!(d.resolved_wiki_targets, input.resolved_wiki_targets);
        }
        let KnowledgeReferencesRead::Published { read, .. } =
            read_native_knowledge_reference_query(
                ctx.snapshot(),
                &read,
                KnowledgeReferenceQuery {
                    outgoing: Some("literal"),
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
        assert!(!read.coverage.knowledge_source_complete);
        assert_eq!(read.warnings.len(), 1);
        let out = read.outgoing.unwrap();
        assert_eq!(out.total, 8);
        assert_eq!(out.results[3].payload, out.results[4].payload);
        assert!(
            matches!(&out.results[2].payload,KnowledgeReferencePayload::Authored{target,..}
            if target.id=="line\nbreak" && target.namespace.scope==KnowledgeReferenceOwnedScope::User("owner β".into()))
        );
        assert!(
            matches!(&out.results[3].payload,KnowledgeReferencePayload::Authored{target,..} if target.namespace.context.as_deref()==Some("alias β"))
        );
        assert_eq!(
            out.results[5].provenance,
            KnowledgeReferenceProvenance::WikiLink
        );
        // An exclusive creator never adopts or overwrites the same corpus.
        assert!(
            publish_native_authored_with_reference_updates(
                ctx.snapshot(),
                &write,
                &rows,
                &updates,
                write_limits()
            )
            .await
            .is_err()
        );
        assert_eq!(
            BranchSnapshot::capture_observed(&result.default.corpus_uri, None, None)
                .await?
                .version(),
            result.default.version
        );
        assert_eq!(image(&default).await?, actual);
    }
    Ok(())
}
#[tokio::test]
async fn empty_first_publication_is_published_and_scans_to_eof() -> Result<(), Box<dyn Error>> {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    let rows = h::prepare(ctx.snapshot(), &[], &[], hlimits())?;
    let first = publish_native_authored_with_reference_updates(
        ctx.snapshot(),
        &write,
        &rows,
        &[],
        write_limits(),
    )
    .await?;
    assert!(
        image(&BranchSnapshot::capture_observed(&first.default.corpus_uri, None, None).await?)
            .await?
            .is_empty()
    );
    let read =
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
    assert!(
        matches!(read_native_knowledge_index(ctx.snapshot(),&read,read_limits()).await?,KnowledgeIndexRead::Published{documents,..} if documents.is_empty())
    );
    Ok(())
}
#[tokio::test]
async fn invalid_exact_maps_tuples_and_individual_identifiers_cannot_birth_corpus()
-> Result<(), Box<dyn Error>> {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    let docs = [page("a", &["a", "", "a"]), page("b", &[])];
    let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits())?;
    for updates in [
        vec![],
        vec![preserve("a")],
        vec![preserve("a"), preserve("a")],
        vec![preserve("a"), preserve("wrong")],
    ] {
        assert!(matches!(
            publish_native_authored_with_reference_updates(
                ctx.snapshot(),
                &write,
                &rows,
                &updates,
                write_limits()
            )
            .await,
            Err(KnowledgeIndexError::InvalidField("reference_document_map"))
        ));
    }
    let mut bad = entries()[0];
    bad.context = Some("context on task");
    assert!(matches!(
        publish_native_authored_with_reference_updates(
            ctx.snapshot(),
            &write,
            &rows,
            &[replace("a", &[bad]), preserve("b")],
            write_limits()
        )
        .await,
        Err(KnowledgeIndexError::InvalidField("reference_context"))
    ));
    let utf8 = KnowledgeAuthoredReference {
        scope: KnowledgeReferenceScope::User("owner β"),
        id: " literal β ",
        relation: " supports β ",
        ..entries()[0]
    };
    let tuples = [utf8, utf8];
    let updates = [replace("a", &tuples), replace("b", &[])];
    // H text plus source namespace for both Some rows plus both literal tuple strings.
    let mut exact = write_limits();
    exact.content.max_identifier_bytes = n(26);
    let mut smaller = exact;
    smaller.content.max_identifier_bytes = n(25);
    assert!(matches!(
        publish_native_authored_with_reference_updates(
            ctx.snapshot(),
            &write,
            &rows,
            &updates,
            smaller
        )
        .await,
        Err(KnowledgeIndexError::LimitExceeded("identifier_bytes"))
    ));
    assert!(
        !f.project
            .runtime_root()
            .join("knowledge/index.lance")
            .exists()
    );
    let observed = h::prepare(
        ctx.snapshot(),
        &docs,
        &[h::CompilationObservationFields {
            path: "observed.md",
            state: h::CompilationObservationState::Removed,
        }],
        hlimits(),
    )?;
    assert!(matches!(
        publish_native_authored_with_reference_updates(
            ctx.snapshot(),
            &write,
            &observed,
            &updates,
            write_limits()
        )
        .await,
        Err(KnowledgeIndexError::InvalidField("compilation_observation"))
    ));
    let duplicate = h::prepare(
        ctx.snapshot(),
        &[page("a", &[]), page("a", &[])],
        &[],
        hlimits(),
    );
    if let Ok(duplicate) = duplicate {
        assert!(matches!(
            publish_native_authored_with_reference_updates(
                ctx.snapshot(),
                &write,
                &duplicate,
                &[preserve("a"), preserve("a")],
                write_limits()
            )
            .await,
            Err(KnowledgeIndexError::DuplicateSlug)
        ));
    }
    assert!(
        !f.project
            .runtime_root()
            .join("knowledge/index.lance")
            .exists()
    );
    let mut invalid_time = page("a", &[]);
    invalid_time.stale_after = Some(std::time::Duration::from_secs(u64::MAX));
    let time = h::prepare(ctx.snapshot(), &[invalid_time], &[], hlimits())?;
    assert!(
        publish_native_authored_with_reference_updates(
            ctx.snapshot(),
            &write,
            &time,
            &[preserve("a")],
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(
        !f.project
            .runtime_root()
            .join("knowledge/index.lance")
            .exists()
    );
    publish_native_authored_with_reference_updates(ctx.snapshot(), &write, &rows, &updates, exact)
        .await?;
    Ok(())
}
#[tokio::test]
async fn wrong_operation_original_and_writer_wait_cancellation_have_no_effect()
-> Result<(), Box<dyn Error>> {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    use graphit_auth::Permission;
    use graphit_core::module_settings::Module;
    let issuer = f.issuer_permissions(
        &ctx,
        &[
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
            Permission::WriteMemory,
            Permission::ManageAuthProfiles,
            Permission::ManageAccessGrants,
            Permission::Delegate,
        ],
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    let rows = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
    let updates = [preserve("a")];
    let wrong =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Memory)?;
    assert!(matches!(
        publish_native_authored_with_reference_updates(
            ctx.snapshot(),
            &wrong,
            &rows,
            &updates,
            write_limits()
        )
        .await,
        Err(KnowledgeIndexError::WrongAdmission)
    ));
    let other_cancel = Cancellation::default();
    let other = request(&other_cancel);
    assert!(matches!(
        publish_native_authored_with_reference_updates(
            other.snapshot(),
            &write,
            &rows,
            &updates,
            write_limits()
        )
        .await,
        Err(KnowledgeIndexError::DifferentRequest)
    ));
    let other_rows = h::prepare(other.snapshot(), &[page("a", &[])], &[], hlimits())?;
    assert!(
        publish_native_authored_with_reference_updates(
            ctx.snapshot(),
            &write,
            &other_rows,
            &updates,
            write_limits()
        )
        .await
        .is_err()
    );
    let limits = write_limits();
    let artifacts = ArtifactStore::new(f.project.clone(), limits.artifacts);
    let holder = artifacts.writer_with_context(ctx.snapshot())?;
    let interrupter = cancel.clone();
    let trigger = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        interrupter.cancel();
    });
    let error = publish_native_authored_with_reference_updates(
        ctx.snapshot(),
        &write,
        &rows,
        &updates,
        limits,
    )
    .await
    .unwrap_err();
    trigger.await?;
    drop(holder);
    assert!(error.source().is_some());
    assert!(
        matches!(error, KnowledgeIndexError::Artifact(ref cause)
        if cause.stage() == graphit_core::artifact_store::ArtifactStage::Lock
            && cause.kind() == io::ErrorKind::Interrupted),
        "{error:?}"
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
async fn real_revoked_bearer_is_denied_and_write_alone_can_create() -> Result<(), Box<dyn Error>> {
    use graphit_auth::{AuthError, Permission};
    use graphit_core::module_settings::Module;
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
    let rows = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
    issuer.revoke(&parent, &bearer)?;
    let error = publish_native_authored_with_reference_updates(
        ctx.snapshot(),
        &write,
        &rows,
        &[preserve("a")],
        write_limits(),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        KnowledgeIndexError::Auth(AuthError::Denied)
    ));
    assert!(error.source().is_some());
    assert!(
        !f.project
            .runtime_root()
            .join("knowledge/index.lance")
            .exists()
    );
    let write_only = f.issuer_permissions(
        &ctx,
        &[
            Permission::MutateModule(Module::Knowledge),
            Permission::ManageAuthProfiles,
            Permission::ManageAccessGrants,
            Permission::Delegate,
        ],
    );
    let caller = write_only.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&write_only);
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    assert!(
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
            .is_err()
    );
    let result = publish_native_authored_with_reference_updates(
        ctx.snapshot(),
        &write,
        &rows,
        &[replace("a", &[])],
        write_limits(),
    )
    .await?;
    assert_eq!(
        image(&BranchSnapshot::capture_observed(&result.default.corpus_uri, None, None).await?)
            .await?
            .len(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn ordinary_codec_matches_authorized_full18_and_rejects_bad_reference_map()
-> Result<(), Box<dyn Error>> {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    let fields = [page("a", &[]), page("b", &[])];
    let rows = h::prepare(ctx.snapshot(), &fields, &[], hlimits())?;
    let updates = [preserve("a"), replace("b", &[])];
    let graphit_core::artifact_store::StoreOwner::Project { project_id } = f.project.owner() else {
        panic!()
    };
    let authorized = encode_native_knowledge_source_batch(
        ctx.snapshot(),
        &write,
        &rows,
        &updates,
        write_limits(),
    )?;
    let ordinary = encode_knowledge_source_batch(
        ctx.snapshot(),
        *project_id,
        &rows,
        &updates,
        write_limits().content,
    )?;
    assert_eq!(ordinary, authorized);
    assert!(projection(&ordinary).is_null(0));
    assert!(!projection(&ordinary).is_null(1));
    assert!(matches!(
        encode_knowledge_source_batch(
            ctx.snapshot(),
            *project_id,
            &rows,
            &[preserve("a")],
            write_limits().content
        ),
        Err(KnowledgeIndexError::InvalidField("reference_document_map"))
    ));
    let duplicate = [preserve("a"), preserve("a")];
    assert!(matches!(
        encode_knowledge_source_batch(
            ctx.snapshot(),
            *project_id,
            &rows,
            &duplicate,
            write_limits().content
        ),
        Err(KnowledgeIndexError::InvalidField("reference_document_map"))
    ));
    cancel.cancel();
    assert!(
        matches!(encode_knowledge_source_batch(ctx.snapshot(),*project_id,&rows,&updates,write_limits().content),Err(KnowledgeIndexError::Request(error)) if error.kind()==io::ErrorKind::Interrupted)
    );
    Ok(())
}

#[tokio::test]
async fn ordinary_codec_project_b_batch_cannot_publish_under_project_a()
-> Result<(), Box<dyn Error>> {
    use graphit_core::artifact_store::{
        CapturedSourceReader, ProjectIgnoreCriteria, ProjectSelectionCriteria,
        ProjectSelectionLimits, SourceReadLimits, StoreOwner,
    };
    use graphit_data::{
        knowledge_document_lifecycle::{
            KnowledgeSourcePublicationEffect, KnowledgeSourcePublicationPhase,
            publish_native_knowledge_source_stream,
        },
        lance_module_store::{LanceModuleStore, RecordBatchStreamAdapter},
    };
    use std::{path::Path, sync::Arc};
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let a = Fixture::new(&ctx);
    let b = Fixture::new(&ctx);
    let issuer = a.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&a.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let read =
        auth.admit_authenticated_read(&caller, &ctx, a.scope(), ProjectReadModule::Knowledge)?;
    let write =
        auth.admit_authenticated_write(&caller, &ctx, a.scope(), ProjectWriteOperation::Knowledge)?;
    let artifacts = ArtifactStore::new(a.project.clone(), write_limits().artifacts);
    let request = Arc::clone(ctx.snapshot());
    drop(tokio::task::spawn_blocking(move || artifacts.writer_with_context(&request)).await??);
    std::fs::create_dir(a.project.owner_root().join("docs"))?;
    std::fs::write(
        a.project.owner_root().join("docs/a.md"),
        "# Actual source A\n",
    )?;
    let protected = format!("{}/", a.project.brand().directory());
    let source = CapturedSourceReader::capture_selected_project_metadata(
        &a.project,
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
        ctx.snapshot(),
    )?;
    let installation_b = graphit_core::project_paths::InstallationPaths::resolve(
        b.project.brand().clone(),
        b.project.owner_root(),
        None,
        Some(b.realm().live.owner_root()),
    )?;
    let paths_b =
        graphit_core::project_paths::ProjectPaths::at(b.project.owner_root(), &installation_b)?;
    let scope_b = graphit_core::artifact_store::CapturedPublicationScope::project(
        &paths_b,
        "01KSH1CRFFG8Z74B5ZS78WW809".parse()?,
        graphit_core::artifact_store::ScopeLimits::new(16, 4096, 128)?,
        ctx.snapshot(),
    )?;
    let StoreOwner::Project {
        project_id: project_b,
    } = scope_b.owner()
    else {
        panic!()
    };
    let StoreOwner::Project {
        project_id: project_a,
    } = a.project.owner()
    else {
        panic!()
    };
    assert_ne!(project_a, project_b);
    let fields = [page("a", &[])];
    let rows = h::prepare(ctx.snapshot(), &fields, &[], hlimits())?;
    // Replace(empty) carries an actual B namespace and witness; Preserve(None)
    // would have no namespace and could not prove this boundary.
    let batch = encode_knowledge_source_batch(
        ctx.snapshot(),
        *project_b,
        &rows,
        &[replace("a", &[])],
        write_limits().content,
    )?;
    assert!(!projection(&batch).is_null(0));
    let input = Box::pin(RecordBatchStreamAdapter::new(
        native_knowledge_source_schema(),
        futures::stream::iter([Ok(batch)]),
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
    assert_eq!(error.phase(), KnowledgeSourcePublicationPhase::Image);
    assert_eq!(
        error.effect(),
        KnowledgeSourcePublicationEffect::BeforeCommit
    );
    assert!(error.recovery_key().is_none());
    assert!(error.known_publication().is_none());
    assert!(
        matches!(
            error.cause(),
            KnowledgeIndexError::InvalidField("source_namespace")
        ),
        "{error:?}"
    );
    let uri = url::Url::from_file_path(a.project.runtime_root().join("knowledge/index.lance"))
        .unwrap()
        .to_string();
    assert!(
        LanceModuleStore::open(&uri, native_knowledge_source_schema())
            .await
            .is_err()
    );
    Ok(())
}
