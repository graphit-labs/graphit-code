//! Real PROJECT grants, whole physical images, literal references and SDK branches.
use crate::knowledge_reference_fixture as support;
use arrow_array::{Array, FixedSizeBinaryArray, RecordBatch, StringArray, StructArray};
use futures::TryStreamExt;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::{
    file_lock::Cancellation,
    project_paths::{InstallationPaths, ProjectPaths},
    version_context::{Checkout, VersionContext},
};
use graphit_data::{
    knowledge_document_projection as h,
    knowledge_document_update::*,
    knowledge_index::*,
    knowledge_reference_query::*,
    knowledge_references::*,
    lance_branch_store::{BranchSnapshot, DeltaResolution, LanceBranchStore},
};
use std::{collections::BTreeMap, error::Error, sync::Arc};
use support::*;

fn limits() -> KnowledgeDocumentUpdateLimits {
    KnowledgeDocumentUpdateLimits {
        read: read_limits(),
        write: write_limits(),
    }
}
fn preserve(slug: &str) -> KnowledgeDocumentReferenceUpdate<'_> {
    KnowledgeDocumentReferenceUpdate {
        slug,
        authored: KnowledgeDocumentAuthoredUpdate::Preserve,
    }
}
fn ns() -> KnowledgeReferenceNamespace<'static> {
    KnowledgeReferenceNamespace {
        scope: KnowledgeReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap()),
        context: None,
    }
}
fn reference() -> KnowledgeAuthoredReference<'static> {
    KnowledgeAuthoredReference {
        target_kind: KnowledgeReferenceTargetKind::Knowledge,
        scope: KnowledgeReferenceScope::User("owner β"),
        id: "literal ID",
        context: Some("ctx β"),
        relation: "rel β",
        field: Some(""),
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
            rows.insert(slugs.value(row).to_owned(), batch.slice(row, 1));
        }
    }
    Ok(rows)
}
fn generation(uri: &str, pin: &BranchSnapshot) -> KnowledgeGeneration {
    KnowledgeGeneration {
        corpus_uri: uri.to_owned(),
        version: pin.version(),
        physical_branch: pin.physical_branch().map(str::to_owned),
        inherited_target: pin.inherited_target().map(str::to_owned),
    }
}
fn updated(result: KnowledgeDocumentUpdateResult) -> KnowledgeGeneration {
    match result {
        KnowledgeDocumentUpdateResult::Updated { generation, .. } => generation,
        _ => panic!("changed image expected"),
    }
}
fn witness(batch: &RecordBatch) -> Vec<u8> {
    batch
        .column(17)
        .as_any()
        .downcast_ref::<StructArray>()
        .unwrap()
        .column(4)
        .as_any()
        .downcast_ref::<FixedSizeBinaryArray>()
        .unwrap()
        .value(0)
        .to_vec()
}

#[tokio::test]
async fn subset_preserves_full18_removed_old_pin_and_recomputes_some_witness()
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
    let entries = [reference(), reference()];
    let docs = [
        page("a", &["a", "ghost", "", "a"]),
        page("b", &[]),
        page("removed", &[]),
    ];
    let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits())?;
    let fields = [
        KnowledgeReferenceFields {
            slug: "a",
            state: KnowledgeDocumentState::Active,
            namespace: ns(),
            authored: &entries,
        },
        KnowledgeReferenceFields {
            slug: "b",
            state: KnowledgeDocumentState::Active,
            namespace: ns(),
            authored: &[],
        },
        KnowledgeReferenceFields {
            slug: "removed",
            state: KnowledgeDocumentState::Removed,
            namespace: ns(),
            authored: &[],
        },
    ];
    let refs = prepare_knowledge_references(ctx.snapshot(), &rows, &fields, read_limits().content)?;
    let first = publish_native_authored_with_references(
        ctx.snapshot(),
        &write,
        &rows,
        &refs,
        write_limits(),
    )
    .await?
    .default;
    let store = LanceBranchStore::open(&first.corpus_uri, physical_schema(), None).await?;
    let old_pin = store.capture_target(None).await?;
    let old = image(&old_pin).await?;
    let mut b = page("b", &["a", "", "b"]);
    b.title = Some("New title β");
    b.body = Some("New complete source");
    b.summary = Some("New summary");
    b.description = Some("");
    b.summary_truncated = true;
    b.source_path = "new/logical.md";
    b.physical_digest = Some(h::PhysicalSourceDigest32::from_bytes([3; 32]));
    b.compilation_digest = h::CompilationDigest32::from_bytes([4; 32]);
    b.recipe.parameters.as_mut().unwrap().fallback_title = "New fallback";
    b.recipe.parameters.as_mut().unwrap().max_summary_bytes = n(32);
    b.stale_since = Some(std::time::UNIX_EPOCH);
    b.stale_after = Some(std::time::Duration::from_secs(9));
    let update = h::prepare(ctx.snapshot(), &[b], &[], hlimits())?;
    let next = updated(
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &read,
            &write,
            &first,
            &update,
            &[preserve("b")],
            limits(),
            |_, old, new| Ok(old == new),
        )
        .await?,
    );
    assert!(next.version > first.version);
    let fresh = BranchSnapshot::capture_observed(&next.corpus_uri, None, None).await?;
    let after = image(&fresh).await?;
    assert_eq!(old["a"], after["a"]);
    assert_eq!(old["removed"], after["removed"]);
    assert_ne!(old["b"], after["b"]);
    assert_eq!(image(&old_pin).await?, old);
    let KnowledgeIndexRead::Published { documents, .. } =
        read_native_knowledge_index(ctx.snapshot(), &read, read_limits()).await?
    else {
        panic!()
    };
    assert_eq!(documents.len(), 2);
    let b = documents.iter().find(|d| d.slug == "b").unwrap();
    assert_eq!(
        (
            &*b.title,
            &*b.body,
            &*b.summary,
            b.description.as_deref(),
            b.summary_truncated,
            &*b.source_path
        ),
        (
            "New title β",
            "New complete source",
            "New summary",
            Some(""),
            true,
            "new/logical.md"
        )
    );
    assert_eq!(b.compilation_digest.as_bytes(), &[4; 32]);
    assert_eq!(b.recipe_parameters.max_summary_bytes, n(32));
    // Preserve authored tuples with a new header/digest and a newly verified witness.
    let mut a = page("a", &["b", "a"]);
    a.title = Some("A changed");
    a.compilation_digest = h::CompilationDigest32::from_bytes([7; 32]);
    let row_a = h::prepare(ctx.snapshot(), &[a], &[], hlimits())?;
    let next2 = updated(
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &read,
            &write,
            &next,
            &row_a,
            &[preserve("a")],
            limits(),
            |_, old, new| Ok(old == new),
        )
        .await?,
    );
    let new =
        image(&BranchSnapshot::capture_observed(&next2.corpus_uri, None, None).await?).await?;
    assert_ne!(witness(&old["a"]), witness(&new["a"]));
    let q = KnowledgeReferenceQuery {
        outgoing: Some("a"),
        incoming: None,
        page_size: n(20),
        outgoing_cursor: None,
        incoming_cursor: None,
    };
    let KnowledgeReferencesRead::Published {
        read: references, ..
    } = read_native_knowledge_reference_query(ctx.snapshot(), &read, q, read_limits()).await?
    else {
        panic!()
    };
    let out = references.outgoing.unwrap();
    assert_eq!(out.total, 4);
    assert_eq!(out.results[0].payload, out.results[1].payload);
    assert!(
        matches!(&out.results[0].payload,KnowledgeReferencePayload::Authored{relation,field,..} if relation=="rel β" && field.as_deref()==Some(""))
    );
    assert!(
        matches!(&out.results[0].payload, KnowledgeReferencePayload::Authored{target,..}
        if target.namespace.scope==KnowledgeReferenceOwnedScope::User("owner β".into()))
    );
    Ok(())
}

#[tokio::test]
async fn none_preserve_explicit_clear_literal_replace_and_true_noop() -> Result<(), Box<dyn Error>>
{
    let cancel = Cancellation::default();
    let ctx = request_deadline(
        &cancel,
        std::time::Instant::now() + std::time::Duration::from_secs(600),
    );
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let read =
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
    let write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
    let initial = h::prepare(
        ctx.snapshot(),
        &[page("a", &[]), page("b", &[])],
        &[],
        hlimits(),
    )?;
    let mut current = publish_native_authored(ctx.snapshot(), &write, &initial, write_limits())
        .await?
        .default;
    let store = LanceBranchStore::open(&current.corpus_uri, physical_schema(), None).await?;
    let original = store.capture_target(None).await?;
    let mut a = page("a", &["a", ""]);
    a.body = Some("changed");
    let rows = h::prepare(ctx.snapshot(), &[a], &[], hlimits())?;
    current = updated(
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &read,
            &write,
            &current,
            &rows,
            &[preserve("a")],
            limits(),
            |_, old, new| Ok(old == new),
        )
        .await?,
    );
    let snapshot = BranchSnapshot::capture_observed(&current.corpus_uri, None, None).await?;
    assert!(image(&snapshot).await?["a"].column(17).is_null(0));
    let clear = [KnowledgeDocumentReferenceUpdate {
        slug: "a",
        authored: KnowledgeDocumentAuthoredUpdate::Replace(&[]),
    }];
    let prior = current.clone();
    current = updated(
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &read,
            &write,
            &current,
            &rows,
            &clear,
            limits(),
            |_, old, new| Ok(old == new),
        )
        .await?,
    );
    assert!(current.version > prior.version);
    let snapshot = BranchSnapshot::capture_observed(&current.corpus_uri, None, None).await?;
    assert!(!image(&snapshot).await?["a"].column(17).is_null(0));
    // Exact same values including empty Some are genuinely unchanged.
    assert!(
        matches!(update_existing_native_knowledge_documents(ctx.snapshot(),&read,&write,&current,&rows,&clear,limits(), |_, old, new| Ok(old == new)).await?,
        KnowledgeDocumentUpdateResult::Unchanged{generation:g} if g==current)
    );
    assert_eq!(
        BranchSnapshot::capture_observed(&current.corpus_uri, None, None)
            .await?
            .version(),
        current.version
    );
    // Both updated headers use sparse Some/None in the same complete image.
    let mut b = page("b", &["a"]);
    b.body = Some("changed unknown projection");
    let mixed = h::prepare(ctx.snapshot(), &[a, b], &[], hlimits())?;
    current = updated(
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &read,
            &write,
            &current,
            &mixed,
            &[preserve("a"), preserve("b")],
            limits(),
            |_, old, new| Ok(old == new),
        )
        .await?,
    );
    let mixed_image =
        image(&BranchSnapshot::capture_observed(&current.corpus_uri, None, None).await?).await?;
    assert!(!mixed_image["a"].column(17).is_null(0));
    assert!(mixed_image["b"].column(17).is_null(0));
    let entries = [reference(), reference()];
    let replace = [KnowledgeDocumentReferenceUpdate {
        slug: "a",
        authored: KnowledgeDocumentAuthoredUpdate::Replace(&entries),
    }];
    current = updated(
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &read,
            &write,
            &current,
            &rows,
            &replace,
            limits(),
            |_, old, new| Ok(old == new),
        )
        .await?,
    );
    let old = image(&original).await?;
    assert!(old["a"].column(17).is_null(0));
    assert!(matches!(
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &read,
            &write,
            &current,
            &rows,
            &[preserve("a")],
            limits(),
            |_, old, new| Ok(old == new)
        )
        .await?,
        KnowledgeDocumentUpdateResult::Unchanged { .. }
    ));
    let empty = h::prepare(ctx.snapshot(), &[], &[], hlimits())?;
    assert!(matches!(
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &read,
            &write,
            &current,
            &empty,
            &[],
            limits(),
            |_, old, new| Ok(old == new)
        )
        .await?,
        KnowledgeDocumentUpdateResult::Unchanged { .. }
    ));
    for stale in [
        prior,
        KnowledgeGeneration {
            corpus_uri: "different".into(),
            ..current.clone()
        },
        KnowledgeGeneration {
            physical_branch: Some("different".into()),
            ..current.clone()
        },
        KnowledgeGeneration {
            inherited_target: Some("different".into()),
            ..current.clone()
        },
    ] {
        assert!(
            matches!(update_existing_native_knowledge_documents(ctx.snapshot(),&read,&write,&stale,&empty,&[],limits(), |_, old, new| Ok(old == new)).await,
            Err(KnowledgeDocumentUpdateError::GenerationChanged{observed}) if observed==current)
        );
    }
    Ok(())
}

#[tokio::test]
async fn no_birth_unknown_removed_and_reference_map_errors() -> Result<(), Box<dyn Error>> {
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
    let rows = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
    let expected = KnowledgeGeneration {
        corpus_uri: "expected".into(),
        version: 1,
        physical_branch: None,
        inherited_target: None,
    };
    assert!(matches!(
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &read,
            &write,
            &expected,
            &rows,
            &[preserve("a")],
            limits(),
            |_, old, new| Ok(old == new)
        )
        .await,
        Err(KnowledgeDocumentUpdateError::NoCorpus)
    ));
    assert!(!f.project.runtime_root().join("knowledge").exists());
    let docs = [page("a", &[]), page("removed", &[])];
    let initial = h::prepare(ctx.snapshot(), &docs, &[], hlimits())?;
    let fields = [
        KnowledgeReferenceFields {
            slug: "a",
            state: KnowledgeDocumentState::Active,
            namespace: ns(),
            authored: &[],
        },
        KnowledgeReferenceFields {
            slug: "removed",
            state: KnowledgeDocumentState::Removed,
            namespace: ns(),
            authored: &[],
        },
    ];
    let refs =
        prepare_knowledge_references(ctx.snapshot(), &initial, &fields, read_limits().content)?;
    let current = publish_native_authored_with_references(
        ctx.snapshot(),
        &write,
        &initial,
        &refs,
        write_limits(),
    )
    .await?
    .default;
    for slug in ["absent", "removed"] {
        let rows = h::prepare(ctx.snapshot(), &[page(slug, &[])], &[], hlimits())?;
        assert!(matches!(
            update_existing_native_knowledge_documents(
                ctx.snapshot(),
                &read,
                &write,
                &current,
                &rows,
                &[preserve(slug)],
                limits(),
                |_, old, new| Ok(old == new)
            )
            .await,
            Err(KnowledgeDocumentUpdateError::UnknownSource)
        ));
    }
    for refs in [
        vec![],
        vec![preserve("other")],
        vec![preserve("a"), preserve("a")],
    ] {
        assert!(matches!(
            update_existing_native_knowledge_documents(
                ctx.snapshot(),
                &read,
                &write,
                &current,
                &rows,
                &refs,
                limits(),
                |_, old, new| Ok(old == new)
            )
            .await,
            Err(KnowledgeDocumentUpdateError::InvalidInput(
                "reference_document_map"
            ))
        ));
    }
    let duplicate = h::prepare(
        ctx.snapshot(),
        &[page("a", &[]), page("a", &[])],
        &[],
        hlimits(),
    );
    // H owns duplicate validation before storage; both boundaries reject it.
    if let Ok(duplicate) = duplicate {
        assert!(
            update_existing_native_knowledge_documents(
                ctx.snapshot(),
                &read,
                &write,
                &current,
                &duplicate,
                &[preserve("a"), preserve("a")],
                limits(),
                |_, old, new| Ok(old == new)
            )
            .await
            .is_err()
        );
    }
    Ok(())
}

#[tokio::test]
async fn named_physical_updates_and_inherited_noop_without_materialization()
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
    let rows = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
    let first = publish_native_authored(ctx.snapshot(), &write, &rows, write_limits())
        .await?
        .default;
    let install = InstallationPaths::resolve(
        f.project.brand().clone(),
        f.project.owner_root(),
        None,
        Some(f.realm().live.owner_root()),
    )?;
    let paths = ProjectPaths::at(f.project.owner_root(), &install)?;
    let branch = VersionContext::select(&paths, Checkout::NonGit, "feature β", None, "")?;
    let missing = branch.lance_branch();
    let scope = f.scope_for(&branch);
    let named_read =
        auth.admit_authenticated_read(&caller, &ctx, scope, ProjectReadModule::Knowledge)?;
    let named_write =
        auth.admit_authenticated_write(&caller, &ctx, scope, ProjectWriteOperation::Knowledge)?;
    let pin = BranchSnapshot::capture_observed(&first.corpus_uri, Some(&missing), None).await?;
    assert!(pin.inherited_target().is_some());
    let inherited = generation(&first.corpus_uri, &pin);
    assert!(matches!(
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &named_read,
            &named_write,
            &inherited,
            &rows,
            &[preserve("a")],
            limits(),
            |_, old, new| Ok(old == new)
        )
        .await?,
        KnowledgeDocumentUpdateResult::Unchanged { .. }
    ));
    let mut a = page("a", &[]);
    a.body = Some("named change");
    let change = h::prepare(ctx.snapshot(), &[a], &[], hlimits())?;
    assert!(matches!(
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &named_read,
            &named_write,
            &inherited,
            &change,
            &[preserve("a")],
            limits(),
            |_, old, new| Ok(old == new)
        )
        .await,
        Err(KnowledgeDocumentUpdateError::InheritedTarget)
    ));
    assert!(
        BranchSnapshot::capture_observed(&first.corpus_uri, Some(&missing), None)
            .await?
            .inherited_target()
            .is_some()
    );
    assert_eq!(
        BranchSnapshot::capture_observed(&first.corpus_uri, None, None)
            .await?
            .version(),
        first.version
    );
    // Separate SDK materialization prepares fixture only, never bundled into update.
    let materialized = pin.materialize().await?;
    let expected = generation(&first.corpus_uri, &materialized);
    let new = updated(
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &named_read,
            &named_write,
            &expected,
            &change,
            &[preserve("a")],
            limits(),
            |_, old, new| Ok(old == new),
        )
        .await?,
    );
    assert_eq!(new.physical_branch.as_deref(), Some(missing.as_str()));
    assert!(new.inherited_target.is_none());
    assert_eq!(
        BranchSnapshot::capture_observed(&first.corpus_uri, None, None)
            .await?
            .version(),
        first.version
    );
    assert!(matches!(
        materialized
            .resolve(&match update_existing_native_knowledge_documents(
                ctx.snapshot(),
                &named_read,
                &named_write,
                &new,
                &rows,
                &[preserve("a")],
                limits(),
                |_, old, new| Ok(old == new)
            )
            .await?
            {
                KnowledgeDocumentUpdateResult::Updated { recovery_key, .. } => recovery_key,
                _ => panic!(),
            })
            .await?,
        DeltaResolution::Applied { .. }
    ));
    assert!(matches!(
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &read,
            &named_write,
            &first,
            &rows,
            &[preserve("a")],
            limits(),
            |_, old, new| Ok(old == new)
        )
        .await,
        Err(KnowledgeDocumentUpdateError::ScopeMismatch)
    ));
    Ok(())
}

#[tokio::test]
async fn full_eof_rejects_late_corruption_even_for_empty_updates() -> Result<(), Box<dyn Error>> {
    use arrow_array::{UInt8Array, builder::FixedSizeBinaryBuilder};
    for defect in ["state", "witness", "duplicate", "removed_refs", "schema"] {
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
        let docs = [page("a", &[]), page("z", &["a"])];
        let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits())?;
        let fields = [
            KnowledgeReferenceFields {
                slug: "a",
                state: KnowledgeDocumentState::Active,
                namespace: ns(),
                authored: &[],
            },
            KnowledgeReferenceFields {
                slug: "z",
                state: KnowledgeDocumentState::Active,
                namespace: ns(),
                authored: &[],
            },
        ];
        let refs =
            prepare_knowledge_references(ctx.snapshot(), &rows, &fields, read_limits().content)?;
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
        let prior = image(&pin).await?;
        let mut columns = prior["z"].columns().to_vec();
        match defect {
            "state" => columns[16] = Arc::new(UInt8Array::from(vec![255])),
            "removed_refs" => columns[16] = Arc::new(UInt8Array::from(vec![2])),
            "witness" => {
                let p = columns[17].as_any().downcast_ref::<StructArray>().unwrap();
                let mut parts = p.columns().to_vec();
                let mut witness = FixedSizeBinaryBuilder::new(32);
                witness.append_value([0; 32])?;
                parts[4] = Arc::new(witness.finish());
                columns[17] = Arc::new(StructArray::try_new(p.fields().clone(), parts, None)?);
            }
            "duplicate" => columns[0] = Arc::new(StringArray::from(vec!["a"])),
            "schema" => {}
            _ => unreachable!(),
        }
        let bad = if defect == "schema" {
            prior["z"].project(&(0..16).collect::<Vec<_>>())?
        } else {
            RecordBatch::try_new(physical_schema(), columns)?
        };
        let image = if defect == "schema" {
            vec![bad]
        } else {
            vec![prior["a"].clone(), bad]
        };
        let damaged = pin.prepare_overwrite(image).await?.commit().await?;
        let expected = generation(&first.corpus_uri, &damaged);
        let empty = h::prepare(ctx.snapshot(), &[], &[], hlimits())?;
        let error = update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &read,
            &write,
            &expected,
            &empty,
            &[],
            limits(),
            |_, old, new| Ok(old == new),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, KnowledgeDocumentUpdateError::Data(_)),
            "{defect}: {error}"
        );
        assert_eq!(
            BranchSnapshot::capture_observed(&first.corpus_uri, None, None)
                .await?
                .version(),
            damaged.version()
        );
    }
    Ok(())
}

#[tokio::test]
async fn real_bearer_revocation_wrong_module_original_and_scope_are_denied()
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
    let other_cancel = Cancellation::default();
    let other = request(&other_cancel);
    assert!(matches!(
        update_existing_native_knowledge_documents(
            other.snapshot(),
            &read,
            &write,
            &first,
            &rows,
            &[preserve("a")],
            limits(),
            |_, old, new| Ok(old == new)
        )
        .await,
        Err(KnowledgeDocumentUpdateError::Data(
            KnowledgeIndexError::DifferentRequest
        ))
    ));
    let wrong_read =
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)?;
    assert!(matches!(
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &wrong_read,
            &write,
            &first,
            &rows,
            &[preserve("a")],
            limits(),
            |_, old, new| Ok(old == new)
        )
        .await,
        Err(KnowledgeDocumentUpdateError::Data(
            KnowledgeIndexError::WrongAdmission
        ))
    ));
    let run = issuer.start_run(&parent, &f.scope(), std::time::Duration::from_secs(900))?;
    let wrong_write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Memory)?;
    assert!(matches!(
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &read,
            &wrong_write,
            &first,
            &rows,
            &[preserve("a")],
            limits(),
            |_, old, new| Ok(old == new)
        )
        .await,
        Err(KnowledgeDocumentUpdateError::Data(
            KnowledgeIndexError::WrongAdmission
        ))
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
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &different_id_read,
            &write,
            &first,
            &rows,
            &[preserve("a")],
            limits(),
            |_, old, new| Ok(old == new)
        )
        .await,
        Err(KnowledgeDocumentUpdateError::ScopeMismatch)
    ));
    for permissions in [
        vec![Permission::Read(Module::Knowledge)],
        vec![Permission::MutateModule(Module::Knowledge)],
    ] {
        let restricted = issuer.issue(&parent, &run, &reference, &permissions, &f.scope())?;
        let grant = issuer.verify(
            restricted.expose_for_transport(),
            &reference,
            f.scope(),
            &ctx,
        )?;
        if permissions[0] == Permission::Read(Module::Knowledge) {
            assert!(matches!(
                auth.admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge),
                Err(AuthError::Denied)
            ));
        } else {
            assert!(matches!(
                auth.admit_read(&grant, &ctx, ProjectReadModule::Knowledge),
                Err(AuthError::Denied)
            ));
        }
    }
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
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &foreign_read,
            &write,
            &first,
            &rows,
            &[preserve("a")],
            limits(),
            |_, old, new| Ok(old == new)
        )
        .await,
        Err(KnowledgeDocumentUpdateError::ScopeMismatch)
    ));
    issuer.revoke(&parent, &bearer)?;
    let error = update_existing_native_knowledge_documents(
        ctx.snapshot(),
        &read,
        &write,
        &first,
        &rows,
        &[preserve("a")],
        limits(),
        |_, old, new| Ok(old == new),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        KnowledgeDocumentUpdateError::Data(KnowledgeIndexError::Auth(AuthError::Denied))
    ));
    assert!(error.source().unwrap().source().is_some());
    Ok(())
}

#[tokio::test]
async fn whole_image_read_write_and_utf8_identifier_limits_are_exact() -> Result<(), Box<dyn Error>>
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
    let raw = "é".repeat(32);
    let targets = [raw.as_str(), "", "a"];
    let docs = [page("a", &targets), page("b", &[])];
    let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits())?;
    let first = publish_native_authored(ctx.snapshot(), &write, &rows, write_limits())
        .await?
        .default;
    let a = h::prepare(ctx.snapshot(), &[docs[0]], &[], hlimits())?;
    for side in [0, 1] {
        let mut policy = limits();
        let content = if side == 0 {
            &mut policy.read.content
        } else {
            &mut policy.write.content
        };
        content.max_identifier_bytes = n(64);
        assert!(matches!(update_existing_native_knowledge_documents(
            ctx.snapshot(), &read, &write, &first, &a, &[preserve("a")], policy
        , |_, old, new| Ok(old == new)).await?, KnowledgeDocumentUpdateResult::Unchanged { ref generation } if generation == &first));
        let content = if side == 0 {
            &mut policy.read.content
        } else {
            &mut policy.write.content
        };
        content.max_identifier_bytes = n(63);
        assert!(matches!(
            update_existing_native_knowledge_documents(
                ctx.snapshot(),
                &read,
                &write,
                &first,
                &a,
                &[preserve("a")],
                policy,
                |_, old, new| Ok(old == new)
            )
            .await,
            Err(KnowledgeDocumentUpdateError::Data(
                KnowledgeIndexError::LimitExceeded("identifier_bytes")
            ))
        ));
    }
    Ok(())
}

#[tokio::test]
async fn cooperative_writer_serializes_loser_and_cancel_while_waiting() -> Result<(), Box<dyn Error>>
{
    use graphit_core::artifact_store::ArtifactStore;
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
    let initial = h::prepare(ctx.snapshot(), &[page("a", &[])], &[], hlimits())?;
    let first = publish_native_authored(ctx.snapshot(), &write, &initial, write_limits())
        .await?
        .default;
    let mut x = page("a", &[]);
    x.body = Some("x");
    let xr = h::prepare(ctx.snapshot(), &[x], &[], hlimits())?;
    let mut y = page("a", &[]);
    y.body = Some("y");
    let yr = h::prepare(ctx.snapshot(), &[y], &[], hlimits())?;
    let changes = [preserve("a")];
    let (a, b) = tokio::join!(
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &read,
            &write,
            &first,
            &xr,
            &changes,
            limits(),
            |_, old, new| Ok(old == new)
        ),
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &read,
            &write,
            &first,
            &yr,
            &changes,
            limits(),
            |_, old, new| Ok(old == new)
        )
    );
    assert!(matches!(
        (&a, &b),
        (
            Ok(KnowledgeDocumentUpdateResult::Updated { .. }),
            Err(KnowledgeDocumentUpdateError::GenerationChanged { .. })
        ) | (
            Err(KnowledgeDocumentUpdateError::GenerationChanged { .. }),
            Ok(KnowledgeDocumentUpdateResult::Updated { .. })
        )
    ));
    let pin = BranchSnapshot::capture_observed(&first.corpus_uri, None, None).await?;
    let expected = generation(&first.corpus_uri, &pin);
    let artifacts = ArtifactStore::new(f.project.clone(), write_limits().artifacts);
    let held = artifacts.writer_with_context(ctx.snapshot())?;
    let (result, _) = tokio::join!(
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &read,
            &write,
            &expected,
            &xr,
            &changes,
            limits(),
            |_, old, new| Ok(old == new)
        ),
        async {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            cancel.cancel();
        }
    );
    assert!(matches!(result, Err(KnowledgeDocumentUpdateError::Data(_))));
    assert!(result.unwrap_err().source().is_some());
    drop(held);
    assert_eq!(
        BranchSnapshot::capture_observed(&first.corpus_uri, None, None)
            .await?
            .version(),
        expected.version
    );
    Ok(())
}
