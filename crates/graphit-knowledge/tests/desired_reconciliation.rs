#![cfg(target_os = "linux")]
use graphit_knowledge as knowledge;
#[path = "support/desired_reconciliation_fixture.rs"]
mod fixture;
use arrow_array::{
    Array, DurationNanosecondArray, RecordBatch, StringArray, TimestampNanosecondArray,
};
use fixture::*;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::{brand::Brand, file_lock::Cancellation};
use graphit_data::{
    knowledge_document_update::KnowledgeDocumentUpdateResult, knowledge_index::*,
    knowledge_reference_query::*, knowledge_references::*, lance_branch_store::BranchSnapshot,
    lance_module_store::LanceModuleStore,
};
use knowledge::desired_reconciliation::*;
use std::{error::Error, sync::Arc};

fn generation(result: KnowledgeDesiredReconciliationResult) -> KnowledgeGeneration {
    match result {
        KnowledgeDesiredReconciliationResult::First { publication, .. } => {
            publication.named.unwrap_or(publication.default)
        }
        KnowledgeDesiredReconciliationResult::Existing(
            KnowledgeDocumentUpdateResult::Updated { generation, .. }
            | KnowledgeDocumentUpdateResult::Unchanged { generation },
        ) => generation,
    }
}
fn updated(result: KnowledgeDesiredReconciliationResult) -> KnowledgeGeneration {
    let KnowledgeDesiredReconciliationResult::Existing(KnowledgeDocumentUpdateResult::Updated {
        generation,
        recovery_key,
    }) = result
    else {
        panic!("expected real update")
    };
    assert_eq!(recovery_key.corpus_uri(), generation.corpus_uri);
    generation
}
async fn snapshot(generation: &KnowledgeGeneration) -> Result<BranchSnapshot, Box<dyn Error>> {
    Ok(BranchSnapshot::capture_observed(
        &generation.corpus_uri,
        generation.physical_branch.as_deref(),
        None,
    )
    .await?)
}

#[tokio::test]
async fn progressive_selection_preserves_full_bodies_digests_and_literal_references()
-> Result<(), Box<dyn Error>> {
    for version in ["", "feature Ω"] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::with_version(&ctx, version);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
        let auth = ProjectRecordAuthority::new(&issuer);
        let admin = issuer.bootstrap_admin(&caller)?;
        let parent = issuer.admin_parent(&caller, &admin)?;
        let (bearer, credential) = f.issue(&issuer, &parent);
        let grant = issuer.verify(bearer.expose_for_transport(), &credential, f.scope(), &ctx)?;
        let read = auth.admit_read(&grant, &ctx, ProjectReadModule::Knowledge)?;
        let write = auth.admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)?;
        let guide = "\u{feff}---\r\nid: ' Manual Ω '\r\ntitle: Guia\r\ndescription: ''\r\nreferences:\r\n- target: {type: memory, id: ' literal ID ', scope: user, scope_id: 'owner β'}\r\n  relation: ' rel β '\r\n  field: ''\r\n---\r\nOlá 世界. [[b]] [[b]] [[missing]]\r\n";
        put(&f, "docs/guide.md", guide);
        put(&f, "docs/b.md", "---\nid: b\nreferences: []\n---\n# B\n");
        put(&f, "docs/plain.md", "# Default\nWhole physical source\n");
        put(&f, "ReadMe.md", "# README\nRoot source\n");
        put(&f, "docs/ignored.md", "---\nid: ignored\n---\nExcluded\n");
        put(&f, ".wikiignore", "docs/ignored.md\n");
        assert!(!f.project.runtime_root().exists());
        let config = config(&[]);
        let result = reconcile_knowledge_source_changes(
            ctx.snapshot(),
            &read,
            &write,
            &config,
            "spec",
            None,
            &|_| {},
            &|| Ok(()),
            limits(),
        )
        .await?;
        let selected = generation(result);
        let raw = image(&snapshot(&selected).await?).await?;
        assert_eq!(raw.len(), 4);
        assert!(!raw.contains_key("ignored"));
        for row in raw.values() {
            active(row);
        }
        assert_eq!(text(&raw[" Manual Ω "], 2), guide);
        assert_eq!(text(&raw[" Manual Ω "], 1), "Guia");
        assert_eq!(
            raw[" Manual Ω "]
                .column(15)
                .as_any()
                .downcast_ref::<arrow_array::ListArray>()
                .unwrap()
                .value_length(0),
            3
        );
        assert!(!raw_projection(&raw[" Manual Ω "]).is_null(0));
        assert!(!raw_projection(&raw["b"]).is_null(0));
        for row in raw
            .values()
            .filter(|row| matches!(text(row, 1), "Default" | "README"))
        {
            incomplete(row);
            let path = if text(row, 1) == "Default" {
                "docs/plain.md"
            } else {
                "ReadMe.md"
            };
            let identity = graphit_data::knowledge_document_lifecycle::KnowledgeSourceIdentity::from_relative_path(std::path::Path::new(path), None, n(4096))?;
            assert_eq!(text(row, 0), identity.slug());
            assert_eq!(text(row, 7), identity.source_path());
        }
        let KnowledgeIndexRead::Published {
            documents,
            generation,
            ..
        } = read_native_knowledge_index(ctx.snapshot(), &read, limits().index.read).await?
        else {
            panic!()
        };
        assert_eq!(generation, selected);
        assert_eq!(documents.len(), 4);
        let actual = documents.iter().find(|p| p.slug == " Manual Ω ").unwrap();
        assert_eq!(actual.body, guide);
        assert_eq!(actual.description.as_deref(), Some(""));
        assert_eq!(actual.kind, "spec");
        assert!(actual.physical_digest.is_some());
        assert_eq!(actual.updated_at, None);
        let KnowledgeReferencesRead::Published {
            read: references, ..
        } = read_native_knowledge_reference_query(
            ctx.snapshot(),
            &read,
            KnowledgeReferenceQuery {
                outgoing: Some(" Manual Ω "),
                incoming: None,
                page_size: n(10),
                outgoing_cursor: None,
                incoming_cursor: None,
            },
            limits().index.read,
        )
        .await?
        else {
            panic!()
        };
        assert!(!references.coverage.knowledge_source_complete);
        let out = references.outgoing.unwrap();
        assert_eq!(out.total, 4);
        assert!(
            matches!(&out.results[0].payload, KnowledgeReferencePayload::Authored { target, relation, field, .. } if target.id==" literal ID " && target.namespace.scope==KnowledgeReferenceOwnedScope::User("owner β".into()) && relation==" rel β " && field.as_deref()==Some(""))
        );
        assert_eq!(selected.physical_branch.is_some(), !version.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn existing_selection_noop_updates_omission_return_rename_and_foreign_integrity()
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
    put(&f, "docs/a.md", "---\nid: a\nreferences: []\n---\n# A\n");
    put(&f, "docs/b.md", "---\nid: b\n---\n# B\n");
    let config = config(&[("knowledge.include_readme", serde_json::json!(false))]);
    let mut generation = generation(
        reconcile_knowledge_source_changes(
            ctx.snapshot(),
            &read,
            &write,
            &config,
            "page",
            None,
            &|_| {},
            &|| Ok(()),
            limits(),
        )
        .await?,
    );
    let mut baseline = image(&snapshot(&generation).await?).await?;
    // Existing dates are real stored document data, independent of the removed
    // caller-only temporal override API. Rename and reparsing must preserve them.
    for slug in ["a", "b"] {
        let mut columns = baseline[slug].columns().to_vec();
        columns[12] = Arc::new(
            TimestampNanosecondArray::from(vec![Some(42_000_000_000)]).with_timezone("UTC"),
        );
        columns[14] = Arc::new(DurationNanosecondArray::from(vec![Some(12_000_000_000)]));
        baseline.insert(
            slug.into(),
            RecordBatch::try_new(physical_schema(), columns)?,
        );
    }
    let mut columns = baseline["a"].columns().to_vec();
    columns[0] = Arc::new(StringArray::from(vec!["foreign"]));
    columns[7] = Arc::new(StringArray::from(vec!["foreign/origin"]));
    columns[17] = arrow_array::new_null_array(physical_schema().field(17).data_type(), 1);
    let foreign = RecordBatch::try_new(physical_schema(), columns)?;
    let store = LanceModuleStore::open(&generation.corpus_uri, physical_schema()).await?;
    let mut all = baseline.values().cloned().collect::<Vec<_>>();
    all.push(foreign.clone());
    let store = store
        .snapshot()
        .prepare_replace(all)
        .await?
        .commit()
        .await?;
    generation.version = store.snapshot().version();
    let result = reconcile_knowledge_source_changes(
        ctx.snapshot(),
        &read,
        &write,
        &config,
        "page",
        None,
        &|_| {},
        &|| Ok(()),
        limits(),
    )
    .await?;
    assert!(
        matches!(result,KnowledgeDesiredReconciliationResult::Existing(KnowledgeDocumentUpdateResult::Unchanged { generation: ref observed }) if observed==&generation)
    );
    assert_eq!(
        image(&snapshot(&generation).await?).await?["foreign"],
        foreign
    );
    // A genuine A-only delta must not even capture/hash B's physical payload.
    // A full capture would reject this B at the existing per-FILE size limit.
    let b_path = f.project.owner_root().join("docs/b.md");
    let b_bytes = std::fs::read(&b_path)?;
    let too_large = usize::try_from(limits().capture.selection.max_selected_file_bytes)? + 1;
    std::fs::write(&b_path, vec![b'x'; too_large])?;
    put(
        &f,
        "docs/a.md",
        "---\nid: a\nreferences: []\n---\n# A delta\n",
    );
    let delta = std::collections::BTreeSet::from([std::path::PathBuf::from("docs/a.md")]);
    generation = updated(
        reconcile_knowledge_source_changes(
            ctx.snapshot(),
            &read,
            &write,
            &config,
            "page",
            Some(&delta),
            &|_| {},
            &|| Ok(()),
            limits(),
        )
        .await?,
    );
    let delta_image = image(&snapshot(&generation).await?).await?;
    assert_eq!(delta_image["b"], baseline["b"]);
    assert_eq!(delta_image["foreign"], foreign);
    assert_eq!(
        text(&delta_image["a"], 2),
        "---\nid: a\nreferences: []\n---\n# A delta\n"
    );
    baseline.insert("a".into(), delta_image["a"].clone());
    std::fs::write(&b_path, b_bytes)?;
    std::fs::remove_file(f.project.owner_root().join("docs/a.md"))?;
    put(&f, "docs/b.md", "---\nid: b\n---\n# B updated\n");
    generation = updated(
        reconcile_knowledge_source_changes(
            ctx.snapshot(),
            &read,
            &write,
            &config,
            "page",
            None,
            &|_| {},
            &|| Ok(()),
            limits(),
        )
        .await?,
    );
    let removed = image(&snapshot(&generation).await?).await?;
    assert_eq!(removed["foreign"], foreign);
    assert_eq!(state(&removed["a"]), KnowledgeDocumentState::Removed as u8);
    assert!(!raw_projection(&removed["a"]).is_null(0));
    for column in 0..=14 {
        assert_eq!(removed["a"].column(column), baseline["a"].column(column));
    }
    assert_eq!(text(&removed["b"], 2), "---\nid: b\n---\n# B updated\n");
    for column in 12..=14 {
        assert_eq!(removed["b"].column(column), baseline["b"].column(column));
    }
    put(
        &f,
        "docs/a.md",
        "---\nid: a\n---\n# Returned without references\n",
    );
    generation = updated(
        reconcile_knowledge_source_changes(
            ctx.snapshot(),
            &read,
            &write,
            &config,
            "page",
            None,
            &|_| {},
            &|| Ok(()),
            limits(),
        )
        .await?,
    );
    let returned = image(&snapshot(&generation).await?).await?;
    active(&returned["a"]);
    incomplete(&returned["a"]);
    assert_eq!(returned["foreign"], foreign);
    // Removed documents are not active temporal defaults. Returning A does not
    // restore its old dates; an active B must preserve dates through rename.
    assert!(returned["a"].column(12).is_null(0));
    assert!(returned["a"].column(14).is_null(0));
    std::fs::create_dir(f.project.owner_root().join("docs/old-dir"))?;
    std::fs::rename(
        f.project.owner_root().join("docs/b.md"),
        f.project.owner_root().join("docs/old-dir/b.md"),
    )?;
    generation = updated(
        reconcile_knowledge_source_changes(
            ctx.snapshot(),
            &read,
            &write,
            &config,
            "page",
            None,
            &|_| {},
            &|| Ok(()),
            limits(),
        )
        .await?,
    );
    let active_rename = image(&snapshot(&generation).await?).await?;
    for column in 12..=14 {
        assert_eq!(
            active_rename["b"].column(column),
            baseline["b"].column(column)
        );
    }
    assert_eq!(text(&active_rename["b"], 2), text(&returned["b"], 2));
    std::fs::rename(
        f.project.owner_root().join("docs/old-dir"),
        f.project.owner_root().join("docs/new-dir"),
    )?;
    let moved = std::collections::BTreeSet::from([
        std::path::PathBuf::from("docs/old-dir"),
        std::path::PathBuf::from("docs/new-dir"),
    ]);
    generation = updated(
        reconcile_knowledge_source_changes(
            ctx.snapshot(),
            &read,
            &write,
            &config,
            "page",
            Some(&moved),
            &|_| {},
            &|| Ok(()),
            limits(),
        )
        .await?,
    );
    let directory_rename = image(&snapshot(&generation).await?).await?;
    active(&directory_rename["b"]);
    assert_ne!(
        text(&directory_rename["b"], 7),
        text(&active_rename["b"], 7)
    );
    assert_eq!(directory_rename["a"], active_rename["a"]);
    for column in 12..=14 {
        assert_eq!(
            directory_rename["b"].column(column),
            baseline["b"].column(column)
        );
    }
    std::fs::remove_dir_all(f.project.owner_root().join("docs/new-dir"))?;
    let removed_directory =
        std::collections::BTreeSet::from([std::path::PathBuf::from("docs/new-dir")]);
    generation = updated(
        reconcile_knowledge_source_changes(
            ctx.snapshot(),
            &read,
            &write,
            &config,
            "page",
            Some(&removed_directory),
            &|_| {},
            &|| Ok(()),
            limits(),
        )
        .await?,
    );
    let directory_delete = image(&snapshot(&generation).await?).await?;
    assert_eq!(
        state(&directory_delete["b"]),
        KnowledgeDocumentState::Removed as u8
    );
    assert_eq!(directory_delete["a"], directory_rename["a"]);
    assert_eq!(directory_delete["foreign"], foreign);
    std::fs::rename(
        f.project.owner_root().join("docs/a.md"),
        f.project.owner_root().join("docs/renamed.md"),
    )?;
    generation = updated(
        reconcile_knowledge_source_changes(
            ctx.snapshot(),
            &read,
            &write,
            &config,
            "page",
            None,
            &|_| {},
            &|| Ok(()),
            limits(),
        )
        .await?,
    );
    let renamed = image(&snapshot(&generation).await?).await?;
    active(&renamed["a"]);
    assert_ne!(text(&renamed["a"], 7), text(&returned["a"], 7));
    assert_eq!(renamed["foreign"], foreign);
    for column in 12..=14 {
        assert_eq!(renamed["a"].column(column), returned["a"].column(column));
    }
    put(
        &f,
        "docs/renamed.md",
        "---\nid: a2\n---\n# New authored id\n",
    );
    generation = updated(
        reconcile_knowledge_source_changes(
            ctx.snapshot(),
            &read,
            &write,
            &config,
            "page",
            None,
            &|_| {},
            &|| Ok(()),
            limits(),
        )
        .await?,
    );
    let changed = image(&snapshot(&generation).await?).await?;
    active(&changed["a2"]);
    assert_eq!(state(&changed["a"]), KnowledgeDocumentState::Removed as u8);
    assert_eq!(changed["foreign"], foreign);
    Ok(())
}

#[tokio::test]
async fn ordinary_brand_operations_original_and_selection_errors_precede_bootstrap()
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
    let wrong = config_brand(&[], Brand::new("other").unwrap());
    assert!(matches!(
        reconcile_knowledge_source_changes(
            ctx.snapshot(),
            &read,
            &write,
            &wrong,
            "page",
            None,
            &|_| {},
            &|| Ok(()),
            limits()
        )
        .await,
        Err(KnowledgeDesiredReconciliationError::BrandMismatch)
    ));
    assert!(!f.project.runtime_root().exists());
    assert!(
        !f.project
            .owner_root()
            .join(f.project.brand().directory())
            .exists()
    );
    let config = config(&[]);
    let memory =
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Memory)?;
    assert!(matches!(
        reconcile_knowledge_source_changes(
            ctx.snapshot(),
            &memory,
            &write,
            &config,
            "page",
            None,
            &|_| {},
            &|| Ok(()),
            limits()
        )
        .await,
        Err(KnowledgeDesiredReconciliationError::WrongAdmission)
    ));
    let memory_write =
        auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Memory)?;
    assert!(matches!(
        reconcile_knowledge_source_changes(
            ctx.snapshot(),
            &read,
            &memory_write,
            &config,
            "page",
            None,
            &|_| {},
            &|| Ok(()),
            limits()
        )
        .await,
        Err(KnowledgeDesiredReconciliationError::WrongAdmission)
    ));
    let other_cancel = Cancellation::default();
    let other = request(&other_cancel);
    assert!(matches!(
        reconcile_knowledge_source_changes(
            other.snapshot(),
            &read,
            &write,
            &config,
            "page",
            None,
            &|_| {},
            &|| Ok(()),
            limits()
        )
        .await,
        Err(KnowledgeDesiredReconciliationError::DifferentRequest)
    ));
    let version = f.named_version("other version");
    let named = auth.admit_authenticated_write(
        &caller,
        &ctx,
        f.scope_for(&version),
        ProjectWriteOperation::Knowledge,
    )?;
    assert!(matches!(
        reconcile_knowledge_source_changes(
            ctx.snapshot(),
            &read,
            &named,
            &config,
            "page",
            None,
            &|_| {},
            &|| Ok(()),
            limits()
        )
        .await,
        Err(KnowledgeDesiredReconciliationError::ScopeMismatch)
    ));
    assert!(!f.project.runtime_root().exists());
    assert_no_birth(&f);
    Ok(())
}

#[tokio::test]
async fn malformed_late_files_preserve_confirmed_prefix_and_pending_source()
-> Result<(), Box<dyn Error>> {
    for late in [b"---\nid: a\n---\n# Duplicate slug\n".to_vec(), vec![0xff], b"---\nid: 42\n---\nlate\n".to_vec(),
        b"---\ntitle: [broken\n---\nlate\n".to_vec(),
        b"---\nreferences: [{target: {type: task, id: x, context: alias}, relation: supports}]\n---\nlate\n".to_vec(),
        b"# Late\n[bad](%FF.md)\n".to_vec()] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
        let auth = ProjectRecordAuthority::new(&issuer);
        let read = auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
        let write = auth.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)?;
        put(&f, "docs/a.md", "---\nid: a\n---\n# Valid prefix\n");
        put(&f, "docs/z.md", late);
        let error = reconcile_knowledge_source_changes(ctx.snapshot(), &read, &write,
            &config(&[("knowledge.include_readme",serde_json::json!(false))]),
            "page", None, &|_| {}, &|| Ok(()), limits()).await.unwrap_err();
        let KnowledgeDesiredReconciliationError::Published { publication, pending_paths, cause } = error else {
            panic!("confirmed prefix must survive invalid Z: {error:?}")
        };
        assert_eq!(pending_paths, std::collections::BTreeSet::from(["docs/z.md".into()]));
        assert!(matches!(*cause, KnowledgeDesiredReconciliationError::Source(_) |
            KnowledgeDesiredReconciliationError::Authored(_) | KnowledgeDesiredReconciliationError::Mapping(_)));
        let rows = image(&snapshot(&generation(*publication)).await?).await?;
        assert_eq!(rows.len(), 1);
        assert_eq!(text(&rows["a"], 2), "---\nid: a\n---\n# Valid prefix\n");
    }
    Ok(())
}

#[tokio::test]
async fn real_capture_mapping_projection_and_data_limits_accept_equality_and_reject_excess()
-> Result<(), Box<dyn Error>> {
    for case in 0..=4 {
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
        let a = format!(
            "---\nid: a\n---\n# A\n[[bbbb]] [[bbbb]]\n{}",
            "x".repeat(128)
        );
        let b = "---\nid: b\n---\n# B\n";
        put(&f, "docs/a.md", &a);
        put(&f, "docs/b.md", b);
        let config = config(&[("knowledge.include_readme", serde_json::json!(false))]);
        let mut capped = limits();
        capped.capture.selection.max_selected_file_bytes = u64::try_from(a.len())?;
        capped.preparation.mapping.max_uri_bytes = n(4);
        capped.projection.max_documents = n(1); // Local FILE preparation, never a corpus cap.
        capped.projection.max_field_bytes = n(a.len());
        match case {
            0 => capped.capture.selection.max_selected_file_bytes -= 1,
            1 => capped.preparation.mapping.max_uri_bytes = n(3),
            2 => capped.projection.max_field_bytes = n(a.len() - 1),
            3 => capped.index.write.content.max_identifier_bytes = n(1),
            4 => (),
            _ => unreachable!(),
        }
        let result = reconcile_knowledge_source_changes(
            ctx.snapshot(),
            &read,
            &write,
            &config,
            "page",
            None,
            &|_| {},
            &|| Ok(()),
            capped,
        )
        .await;
        if case == 4 {
            let published = generation(result?);
            assert_eq!(image(&snapshot(&published).await?).await?.len(), 2);
            continue;
        }
        let error = result.unwrap_err();
        if case == 0 {
            assert!(matches!(
                error,
                KnowledgeDesiredReconciliationError::Source(_)
            ));
            assert_no_birth(&f);
        } else {
            match case {
                1 => assert!(
                    matches!(error, KnowledgeDesiredReconciliationError::Mapping(_)),
                    "{error:?}"
                ),
                2 => assert!(
                    matches!(error, KnowledgeDesiredReconciliationError::Projection(_)),
                    "{error:?}"
                ),
                3 => assert!(
                    matches!(
                        error,
                        KnowledgeDesiredReconciliationError::DataImage(
                            KnowledgeIndexError::LimitExceeded("identifier_bytes")
                        )
                    ),
                    "{error:?}"
                ),
                _ => unreachable!(),
            }
            assert!(
                LanceModuleStore::open(&uri(&f), physical_schema())
                    .await
                    .is_err()
            );
        }
    }
    Ok(())
}
