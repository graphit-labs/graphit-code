use super::*;

#[path = "fixture.rs"]
mod fixture;
use crate::knowledge_document_projection as h;
use crate::lance_branch_store::BranchSnapshot;
use fixture::*;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::artifact_store::{
    ProjectIgnoreCriteria, ProjectSelectionCriteria, ProjectSelectionLimits, SourceReadLimits,
};
use graphit_core::file_lock::Cancellation;
use std::error::Error;
use std::path::Path;

fn limits() -> KnowledgeDocumentLifecycleLimits {
    KnowledgeDocumentLifecycleLimits {
        read: read_limits(),
        write: write_limits(),
    }
}
fn changes() -> [KnowledgeDocumentReferenceUpdate<'static>; 1] {
    [KnowledgeDocumentReferenceUpdate {
        slug: "a",
        authored: KnowledgeDocumentAuthoredUpdate::Preserve,
    }]
}

fn managed_page() -> h::AuthoredPageFields<'static> {
    let mut p = page("a", &[]);
    p.source_path = "knowledge-source/t/646f6373/612e6d64";
    p
}
fn capture(f: &Fixture, original: &Arc<SnapshotContext>) -> CapturedSourceReader {
    let root = f.project.owner_root();
    std::fs::create_dir_all(root.join("docs")).unwrap();
    if !root.join("docs/real.md").exists() {
        std::fs::write(root.join("docs/real.md"), "# Captured body").unwrap();
    }
    CapturedSourceReader::capture_selected_project(
        &f.project,
        ProjectSelectionCriteria {
            subtree: Some(Path::new("docs")),
            extras: &[],
            include_readme: false,
            extensions: &[".md"],
            ignore: ProjectIgnoreCriteria {
                protected_defaults: &[],
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

fn document_batch(
    ctx: &graphit_hub::policy::RequestContext<'_>,
    write: &graphit_auth::ProjectWriteAdmission<'_>,
    page: h::AuthoredPageFields<'_>,
) -> arrow_array::RecordBatch {
    let rows = h::prepare(ctx.snapshot(), &[page], &[], hlimits()).unwrap();
    crate::knowledge_first_publication::encode_native_knowledge_source_batch(
        ctx.snapshot(),
        write,
        &rows,
        &changes(),
        write_limits(),
    )
    .unwrap()
}

#[tokio::test]
async fn progressive_file_publication_preserves_intact_origins_and_explicit_removals()
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
    let read = auth.admit_read(&grant, &ctx, ProjectReadModule::Knowledge)?;
    let write = auth.admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)?;
    let mut b = page("b", &[]);
    b.source_path = "knowledge-source/t/646f6373/622e6d64";
    let authored = page("authored", &[]);
    let initial = h::prepare(
        ctx.snapshot(),
        &[managed_page(), b, authored],
        &[],
        hlimits(),
    )?;
    let first = index::publish_native_authored(ctx.snapshot(), &write, &initial, write_limits())
        .await?
        .default;
    std::fs::create_dir_all(f.project.owner_root().join("docs"))?;
    std::fs::write(
        f.project.owner_root().join("docs/a.md"),
        "immediately searchable new A",
    )?;
    let source = capture(&f, ctx.snapshot());
    let mut changed = managed_page();
    changed.body = Some("immediately searchable new A");
    let updated = reconcile_native_knowledge_source_file(
        ctx.snapshot(),
        &read,
        &write,
        &first,
        &source,
        document_batch(&ctx, &write, changed),
        limits(),
    )
    .await?;
    let generation = match updated {
        KnowledgeDocumentUpdateResult::Updated { generation, .. } => generation,
        _ => panic!("changed file not published"),
    };
    let membership =
        read_native_knowledge_source_origins(ctx.snapshot(), &read, read_limits()).await?;
    let KnowledgeSourceOrigins::Published {
        origins,
        heads,
        generation: actual,
        ..
    } = membership
    else {
        panic!("missing corpus")
    };
    assert_eq!(actual, generation);
    assert_eq!(origins.len(), 2);
    assert_eq!(heads.len(), 2);
    assert!(heads.iter().any(|head| head.slug == "b"));
    let missing = HashSet::from([b.source_path.to_owned()]);
    let deleted = remove_native_knowledge_source_origins(
        ctx.snapshot(),
        &read,
        &write,
        &generation,
        &source,
        &missing,
        limits(),
    )
    .await?;
    let generation = match deleted {
        KnowledgeDocumentUpdateResult::Updated { generation, .. } => generation,
        _ => panic!("removed file not published"),
    };
    let KnowledgeSourceOrigins::Published {
        origins,
        generation: actual,
        ..
    } = read_native_knowledge_source_origins(ctx.snapshot(), &read, read_limits()).await?
    else {
        panic!("missing corpus")
    };
    assert_eq!(actual, generation);
    assert_eq!(
        origins,
        HashSet::from([managed_page().source_path.to_owned()])
    );
    let pin = BranchSnapshot::capture_observed(&first.corpus_uri, None, None).await?;
    let authored = stream::lookup(&pin, "authored", ctx.snapshot(), &|| Ok(())).await?;
    assert_eq!(
        index::array::<UInt8Array>(&authored, 16)?.value(0),
        KnowledgeDocumentState::Active as u8
    );
    let intact = stream::lookup(&pin, "a", ctx.snapshot(), &|| Ok(())).await?;
    assert_eq!(
        index::array::<StringArray>(&intact, 2)?.value(0),
        "immediately searchable new A"
    );
    assert!(matches!(
        reconcile_native_knowledge_source_file(
            ctx.snapshot(),
            &read,
            &write,
            &first,
            &source,
            document_batch(&ctx, &write, managed_page()),
            limits()
        )
        .await,
        Err(KnowledgeDocumentLifecycleError::GenerationChanged { .. })
    ));
    // An actual rename may retain an authored slug when the old FILE is absent.
    std::fs::rename(
        f.project.owner_root().join("docs/a.md"),
        f.project.owner_root().join("docs/moved.md"),
    )?;
    let renamed_source = capture(&f, ctx.snapshot());
    let mut moved = changed;
    moved.source_path = "knowledge-source/t/646f6373/6d6f7665642e6d64";
    let moved_result = reconcile_native_knowledge_source_file(
        ctx.snapshot(),
        &read,
        &write,
        &generation,
        &renamed_source,
        document_batch(&ctx, &write, moved),
        limits(),
    )
    .await?;
    let moved_generation = match moved_result {
        KnowledgeDocumentUpdateResult::Updated { generation, .. } => generation,
        _ => panic!("rename not published"),
    };
    let KnowledgeSourceOrigins::Published { origins, .. } =
        read_native_knowledge_source_origins(ctx.snapshot(), &read, read_limits()).await?
    else {
        panic!("missing renamed corpus")
    };
    assert_eq!(origins, HashSet::from([moved.source_path.to_owned()]));
    // Two still-selected FILEs cannot claim the same authored identity.
    std::fs::write(
        f.project.owner_root().join("docs/a.md"),
        changed.body.unwrap(),
    )?;
    let colliding_source = capture(&f, ctx.snapshot());
    assert!(matches!(
        reconcile_native_knowledge_source_file(
            ctx.snapshot(),
            &read,
            &write,
            &moved_generation,
            &colliding_source,
            document_batch(&ctx, &write, changed),
            limits()
        )
        .await,
        Err(KnowledgeDocumentLifecycleError::ForeignSlugConflict { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn scoped_rename_requires_authoritative_old_origin_coverage() -> Result<(), Box<dyn Error>> {
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
    let mut a = managed_page();
    a.body = Some("A intact");
    let mut b = page("b", &[]);
    b.source_path = "knowledge-source/t/646f6373/622e6d64";
    b.body = Some("B intact");
    let initial = h::prepare(ctx.snapshot(), &[a, b], &[], hlimits())?;
    let first = index::publish_native_authored(ctx.snapshot(), &write, &initial, write_limits())
        .await?
        .default;
    std::fs::create_dir_all(f.project.owner_root().join("docs"))?;
    std::fs::write(f.project.owner_root().join("docs/a.md"), a.body.unwrap())?;
    std::fs::write(f.project.owner_root().join("docs/b.md"), b.body.unwrap())?;
    std::fs::write(
        f.project.owner_root().join("docs/moved.md"),
        "A replacement",
    )?;
    let capture_paths = |extras: &[&Path]| {
        CapturedSourceReader::capture_selected_project_metadata(
            &f.project,
            ProjectSelectionCriteria {
                subtree: None,
                extras,
                include_readme: false,
                extensions: &[".md"],
                ignore: ProjectIgnoreCriteria {
                    protected_defaults: &[],
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
        )
    };
    let mut moved = a;
    moved.source_path = "knowledge-source/t/646f6373/6d6f7665642e6d64";
    moved.body = Some("A replacement");
    let pin = BranchSnapshot::capture_observed(&first.corpus_uri, None, None).await?;
    let original_a = stream::lookup(&pin, "a", ctx.snapshot(), &|| Ok(())).await?;
    let original_b = stream::lookup(&pin, "b", ctx.snapshot(), &|| Ok(())).await?;

    // An intact FILE omitted from a delta's extras cannot lose its authored ID.
    let unrelated = capture_paths(&[Path::new("docs/moved.md")])?;
    assert!(!unrelated.covers_selected_path(Path::new("docs/a.md"))?);
    assert!(matches!(
        reconcile_native_knowledge_source_file(
            ctx.snapshot(),
            &read,
            &write,
            &first,
            &unrelated,
            document_batch(&ctx, &write, moved),
            limits(),
        )
        .await,
        Err(KnowledgeDocumentLifecycleError::ForeignSlugConflict { .. })
    ));
    let unchanged = BranchSnapshot::capture_observed(&first.corpus_uri, None, None).await?;
    assert_eq!(unchanged.version(), first.version);
    assert_eq!(
        stream::lookup(&unchanged, "a", ctx.snapshot(), &|| Ok(())).await?,
        original_a
    );
    assert_eq!(
        stream::lookup(&unchanged, "b", ctx.snapshot(), &|| Ok(())).await?,
        original_b
    );

    // Physical absence alone is also insufficient when this event did not cover it.
    std::fs::remove_file(f.project.owner_root().join("docs/a.md"))?;
    let unrelated = capture_paths(&[Path::new("docs/moved.md")])?;
    assert!(matches!(
        reconcile_native_knowledge_source_file(
            ctx.snapshot(),
            &read,
            &write,
            &first,
            &unrelated,
            document_batch(&ctx, &write, moved),
            limits(),
        )
        .await,
        Err(KnowledgeDocumentLifecycleError::ForeignSlugConflict { .. })
    ));

    // The genuine old-path event proves the absence and permits the rename.
    let renamed = capture_paths(&[Path::new("docs/moved.md"), Path::new("docs/a.md")])?;
    assert!(renamed.covers_selected_path(Path::new("docs/a.md"))?);
    let KnowledgeDocumentUpdateResult::Updated { generation, .. } =
        reconcile_native_knowledge_source_file(
            ctx.snapshot(),
            &read,
            &write,
            &first,
            &renamed,
            document_batch(&ctx, &write, moved),
            limits(),
        )
        .await?
    else {
        panic!("covered rename not published")
    };
    let current = BranchSnapshot::capture_observed(&generation.corpus_uri, None, None).await?;
    assert_eq!(
        stream::lookup(&current, "b", ctx.snapshot(), &|| Ok(())).await?,
        original_b
    );
    let a = stream::lookup(&current, "a", ctx.snapshot(), &|| Ok(())).await?;
    assert_eq!(
        index::array::<StringArray>(&a, 7)?.value(0),
        moved.source_path
    );
    assert_eq!(
        index::array::<StringArray>(&a, 2)?.value(0),
        "A replacement"
    );
    Ok(())
}

#[tokio::test]
async fn first_file_checks_its_captured_bytes_without_rehashing_changed_siblings()
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
    let read = auth.admit_read(&grant, &ctx, ProjectReadModule::Knowledge)?;
    let write = auth.admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)?;
    std::fs::create_dir_all(f.project.owner_root().join("docs"))?;
    std::fs::write(
        f.project.owner_root().join("docs/a.md"),
        "FILE A captured bytes",
    )?;
    std::fs::write(f.project.owner_root().join("docs/b.md"), "FILE B initial")?;
    let source = capture(&f, ctx.snapshot());
    std::fs::write(
        f.project.owner_root().join("docs/b.md"),
        "B independently changed after capture",
    )?;
    let mut a = managed_page();
    a.body = Some("FILE A captured bytes");
    let (publication, _) =
        crate::knowledge_document_lifecycle::publish_native_knowledge_source_file(
            ctx.snapshot(),
            &read,
            &write,
            &source,
            document_batch(&ctx, &write, a),
            write_limits(),
        )
        .await?;
    assert_eq!(publication.default.version, 1);
    // Mutating this FILE is terminal; preserving the sibling cannot excuse it.
    std::fs::write(f.project.owner_root().join("docs/a.md"), "A changed")?;
    assert!(
        reconcile_native_knowledge_source_file(
            ctx.snapshot(),
            &read,
            &write,
            &publication.default,
            &source,
            document_batch(&ctx, &write, a),
            limits()
        )
        .await
        .is_err()
    );
    let KnowledgeSourceOrigins::Published {
        generation, heads, ..
    } = read_native_knowledge_source_origins(ctx.snapshot(), &read, read_limits()).await?
    else {
        panic!("first file unavailable")
    };
    assert_eq!(generation, publication.default);
    assert_eq!(heads[0].slug, "a");
    Ok(())
}

#[tokio::test]
async fn scoped_file_reference_states_preserve_witnesses_foreign_rows_and_reactivation()
-> Result<(), Box<dyn Error>> {
    use crate::knowledge_references::{
        KnowledgeAuthoredReference, KnowledgeReferenceScope, KnowledgeReferenceTargetKind,
    };
    use arrow_array::Array;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let admin = issuer.bootstrap_admin(&caller)?;
    let parent = issuer.admin_parent(&caller, &admin)?;
    let (bearer, credential) = f.issue(&issuer, &parent);
    let grant = issuer.verify(bearer.expose_for_transport(), &credential, f.scope(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let read = auth.admit_read(&grant, &ctx, ProjectReadModule::Knowledge)?;
    let write = auth.admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)?;
    let reference = KnowledgeAuthoredReference {
        target_kind: KnowledgeReferenceTargetKind::Knowledge,
        scope: KnowledgeReferenceScope::User("owner β"),
        id: "literal ID",
        context: Some("ctx β"),
        relation: "rel β",
        field: Some(""),
    };
    let literal = [reference, reference];
    let mut b = page("b", &[]);
    b.source_path = "knowledge-source/t/646f6373/622e6d64";
    let rows = h::prepare(
        ctx.snapshot(),
        &[managed_page(), b, page("foreign", &[])],
        &[],
        hlimits(),
    )?;
    let refs = ["a", "b", "foreign"].map(|slug| KnowledgeDocumentReferenceUpdate {
        slug,
        authored: KnowledgeDocumentAuthoredUpdate::Replace(&literal),
    });
    let first = crate::knowledge_first_publication::publish_native_authored_with_reference_updates(
        ctx.snapshot(),
        &write,
        &rows,
        &refs,
        write_limits(),
    )
    .await?
    .default;
    let old_pin = BranchSnapshot::capture_observed(&first.corpus_uri, None, None).await?;
    let old_a = stream::lookup(&old_pin, "a", ctx.snapshot(), &|| Ok(())).await?;
    let old_b = stream::lookup(&old_pin, "b", ctx.snapshot(), &|| Ok(())).await?;
    let foreign = stream::lookup(&old_pin, "foreign", ctx.snapshot(), &|| Ok(())).await?;
    std::fs::create_dir_all(f.project.owner_root().join("docs"))?;
    std::fs::write(f.project.owner_root().join("docs/a.md"), "new A body")?;
    let source = capture(&f, ctx.snapshot());
    let mut a = managed_page();
    a.body = Some("new A body");
    a.title = Some("New Ω");
    a.compilation_digest = h::CompilationDigest32::from_bytes([7; 32]);
    let KnowledgeDocumentUpdateResult::Updated {
        generation: updated,
        ..
    } = reconcile_native_knowledge_source_file(
        ctx.snapshot(),
        &read,
        &write,
        &first,
        &source,
        document_batch(&ctx, &write, a),
        limits(),
    )
    .await?
    else {
        panic!("changed A")
    };
    let pin = BranchSnapshot::capture_observed(&first.corpus_uri, None, None).await?;
    let now_a = stream::lookup(&pin, "a", ctx.snapshot(), &|| Ok(())).await?;
    let old_projection = index::array::<StructArray>(&old_a, 17)?;
    let projection = index::array::<StructArray>(&now_a, 17)?;
    assert!(!projection.is_null(0));
    assert_eq!(
        projection.column(3).to_data(),
        old_projection.column(3).to_data()
    );
    assert_ne!(
        projection.column(4).to_data(),
        old_projection.column(4).to_data()
    );
    assert_eq!(
        stream::lookup(&pin, "b", ctx.snapshot(), &|| Ok(())).await?,
        old_b
    );
    assert_eq!(
        stream::lookup(&pin, "foreign", ctx.snapshot(), &|| Ok(())).await?,
        foreign
    );
    std::fs::remove_file(f.project.owner_root().join("docs/a.md"))?;
    let deleted_source = capture(&f, ctx.snapshot());
    let KnowledgeDocumentUpdateResult::Updated {
        generation: removed,
        ..
    } = remove_native_knowledge_source_origins(
        ctx.snapshot(),
        &read,
        &write,
        &updated,
        &deleted_source,
        &HashSet::from([a.source_path.to_owned()]),
        limits(),
    )
    .await?
    else {
        panic!("removed A")
    };
    let pin = BranchSnapshot::capture_observed(&first.corpus_uri, None, None).await?;
    let tombstone = stream::lookup(&pin, "a", ctx.snapshot(), &|| Ok(())).await?;
    assert_eq!(
        index::array::<UInt8Array>(&tombstone, 16)?.value(0),
        KnowledgeDocumentState::Removed as u8
    );
    assert!(!index::array::<StructArray>(&tombstone, 17)?.is_null(0));
    assert_eq!(
        index::array::<StructArray>(&tombstone, 17)?
            .column(3)
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap()
            .value_length(0),
        0
    );
    for column in 0..15 {
        assert_eq!(
            tombstone.column(column).to_data(),
            now_a.column(column).to_data()
        );
    }
    std::fs::write(f.project.owner_root().join("docs/a.md"), "new A body")?;
    let returned_source = capture(&f, ctx.snapshot());
    let KnowledgeDocumentUpdateResult::Updated {
        generation: returned,
        ..
    } = reconcile_native_knowledge_source_file(
        ctx.snapshot(),
        &read,
        &write,
        &removed,
        &returned_source,
        document_batch(&ctx, &write, a),
        limits(),
    )
    .await?
    else {
        panic!("returned A")
    };
    let pin = BranchSnapshot::capture_observed(&first.corpus_uri, None, None).await?;
    let returned_a = stream::lookup(&pin, "a", ctx.snapshot(), &|| Ok(())).await?;
    assert!(
        index::array::<StructArray>(&returned_a, 17)?.is_null(0),
        "historical authored references must not return"
    );
    let rows = h::prepare(ctx.snapshot(), &[a], &[], hlimits())?;
    let empty_refs = [KnowledgeDocumentReferenceUpdate {
        slug: "a",
        authored: KnowledgeDocumentAuthoredUpdate::Replace(&[]),
    }];
    let empty = crate::knowledge_first_publication::encode_native_knowledge_source_batch(
        ctx.snapshot(),
        &write,
        &rows,
        &empty_refs,
        write_limits(),
    )?;
    let KnowledgeDocumentUpdateResult::Updated {
        generation: completed,
        ..
    } = reconcile_native_knowledge_source_file(
        ctx.snapshot(),
        &read,
        &write,
        &returned,
        &returned_source,
        empty.clone(),
        limits(),
    )
    .await?
    else {
        panic!("Some(empty) must differ from None")
    };
    let pin = BranchSnapshot::capture_observed(&first.corpus_uri, None, None).await?;
    let completed_a = stream::lookup(&pin, "a", ctx.snapshot(), &|| Ok(())).await?;
    assert!(!index::array::<StructArray>(&completed_a, 17)?.is_null(0));
    assert!(
        matches!(reconcile_native_knowledge_source_file(ctx.snapshot(), &read, &write, &completed, &returned_source, empty, limits()).await?, KnowledgeDocumentUpdateResult::Unchanged { generation } if generation == completed)
    );
    let mut colliding = a;
    colliding.slug = Some("foreign");
    let rows = h::prepare(ctx.snapshot(), &[colliding], &[], hlimits())?;
    let refs = [KnowledgeDocumentReferenceUpdate {
        slug: "foreign",
        authored: KnowledgeDocumentAuthoredUpdate::Preserve,
    }];
    let batch = crate::knowledge_first_publication::encode_native_knowledge_source_batch(
        ctx.snapshot(),
        &write,
        &rows,
        &refs,
        write_limits(),
    )?;
    assert!(matches!(
        reconcile_native_knowledge_source_file(
            ctx.snapshot(),
            &read,
            &write,
            &completed,
            &returned_source,
            batch,
            limits()
        )
        .await,
        Err(KnowledgeDocumentLifecycleError::ForeignSlugConflict { .. })
    ));
    Ok(())
}
