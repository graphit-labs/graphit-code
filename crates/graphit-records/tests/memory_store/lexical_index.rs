use super::*;
use futures::TryStreamExt;
use graphit_data::lance_branch_store::{
    BranchError, DeltaResolution, JsonFtsIndexState, LanceBranchStore,
};
use lance_index::scalar::{
    FullTextSearchQuery,
    inverted::query::{FtsQuery, MatchQuery, Operator},
};

fn publication(value: MemoryLexicalIndexOutcome) -> MemoryLexicalIndexPublication {
    match value {
        MemoryLexicalIndexOutcome::Published(value) => value,
        MemoryLexicalIndexOutcome::NoCorpus | MemoryLexicalIndexOutcome::Ready(_) => {
            panic!("published index")
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lexical_index_no_corpus_incomplete_empty_and_authority_before_io() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, _read, write, _bearer, _reference, _admin
    );
    assert_eq!(
        NativeMemoryStore::maintain_lexical_index(
            ctx.snapshot(),
            &write,
            MemoryLexicalIndexOperation::Ensure,
            write_limits()
        )
        .await
        .unwrap(),
        MemoryLexicalIndexOutcome::NoCorpus
    );
    assert!(!f.project.runtime_root().join("memory").exists());
    let task = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    let fenced = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::FencedMemory)
        .unwrap();
    for wrong in [&task, &fenced] {
        assert!(matches!(
            NativeMemoryStore::maintain_lexical_index(
                ctx.snapshot(),
                wrong,
                MemoryLexicalIndexOperation::Ensure,
                write_limits()
            )
            .await,
            Err(MemoryStoreError::WrongAdmission)
        ));
    }
    let other_cancel = Cancellation::default();
    let other = request(&other_cancel);
    assert!(matches!(
        NativeMemoryStore::maintain_lexical_index(
            other.snapshot(),
            &write,
            MemoryLexicalIndexOperation::Ensure,
            write_limits()
        )
        .await,
        Err(MemoryStoreError::DifferentRequest)
    ));
    assert!(!f.project.runtime_root().join("memory").exists());
    drop(sdk::directory_only(&f, &ctx).await);
    assert!(matches!(
        NativeMemoryStore::maintain_lexical_index(
            ctx.snapshot(),
            &write,
            MemoryLexicalIndexOperation::Ensure,
            write_limits()
        )
        .await,
        Err(MemoryStoreError::IncompleteCorpus)
    ));
    let empty = Fixture::new_for(&ctx, "main", "01KSH1CRFFG8Z74B5ZS78WW809");
    admissions!(
        empty,
        ctx,
        issuer2,
        _authority2,
        _grant2,
        _read2,
        write2,
        _bearer2,
        _reference2,
        _admin2
    );
    sdk::empty_with_schema(&empty, &ctx, sdk::schema()).await;
    let before = sdk::pin(&empty).await;
    let result = publication(
        NativeMemoryStore::maintain_lexical_index(
            ctx.snapshot(),
            &write2,
            MemoryLexicalIndexOperation::Ensure,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(result.generation.sdk_version, before.version() + 1);
    let after = sdk::pin(&empty).await;
    assert_eq!(
        after.json_fts_index_state("payload").await.unwrap(),
        JsonFtsIndexState::Ready
    );
    assert_eq!(sdk::all(&after).await.num_rows(), 0);
    assert_eq!(
        NativeMemoryStore::resolve_lexical_index(
            ctx.snapshot(),
            &write2,
            &result.recovery_key,
            write_limits()
        )
        .await
        .unwrap(),
        DeltaResolution::Applied {
            version: result.generation.sdk_version
        }
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lexical_index_publication_preserves_history_rows_and_real_key_reobservation() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let a = NativeMemoryStore::create(ctx.snapshot(), &write, input("a"), write_limits())
        .await
        .unwrap();
    let b = NativeMemoryStore::create(ctx.snapshot(), &write, input("b"), write_limits())
        .await
        .unwrap();
    NativeMemoryStore::update(
        ctx.snapshot(),
        &write,
        a.id,
        UpdateMemory {
            expected_revision: a.revision,
            idempotency_key: "update-before-index".into(),
            reason: "fixture history".into(),
            title: Some("Updated title".into()),
            body: None,
            classification: None,
            mandatory: None,
            important: None,
            tags: None,
            references: None,
        },
        write_limits(),
    )
    .await
    .unwrap();
    let original = sdk::pin(&f).await;
    let rows = sdk::all(&original).await;
    let history = NativeMemoryStore::history(ctx.snapshot(), &read, a.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    let result = publication(
        NativeMemoryStore::maintain_lexical_index(
            ctx.snapshot(),
            &write,
            MemoryLexicalIndexOperation::Refresh,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(result.generation.sdk_version, original.version() + 1);
    assert_eq!(result.recovery_key.base_version(), original.version());
    assert!(!result.recovery_key.transaction_id().is_empty());
    assert_eq!(result.recovery_key.physical_branch(), None);
    let after = sdk::pin(&f).await;
    assert_eq!(sdk::all(&after).await, rows);
    assert_eq!(
        original.json_fts_index_state("payload").await.unwrap(),
        JsonFtsIndexState::Ready
    );
    assert_eq!(
        after.json_fts_index_state("payload").await.unwrap(),
        JsonFtsIndexState::Ready
    );
    let current = NativeMemoryStore::history(ctx.snapshot(), &read, a.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.current, history.current);
    assert_eq!(current.creation, history.creation);
    assert_eq!(current.revisions, history.revisions);
    let query = MatchQuery::new("body,str,Literal".into())
        .with_column(Some("payload".into()))
        .with_operator(Operator::Or);
    let mut scan = after.scan();
    scan.project(&["row_key"]).unwrap();
    scan.full_text_search(FullTextSearchQuery::new_query(FtsQuery::Match(query)).limit(None))
        .unwrap();
    let batches: Vec<_> = scan
        .try_into_stream()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();
    let hits: std::collections::BTreeSet<_> = batches
        .iter()
        .flat_map(|batch| {
            batch
                .column(0)
                .as_any()
                .downcast_ref::<arrow_array::StringArray>()
                .unwrap()
                .iter()
                .flatten()
        })
        .collect();
    assert_eq!(
        hits,
        std::collections::BTreeSet::from([format!("head/{}", a.id), format!("head/{}", b.id)])
            .iter()
            .map(String::as_str)
            .collect()
    );
    let repeat = NativeMemoryStore::maintain_lexical_index(
        ctx.snapshot(),
        &write,
        MemoryLexicalIndexOperation::Ensure,
        write_limits(),
    )
    .await
    .unwrap();
    assert!(
        matches!(repeat, MemoryLexicalIndexOutcome::Ready(generation)
        if generation.sdk_version == after.version())
    );
    assert_eq!(sdk::pin(&f).await.version(), after.version());
    let fresh = request(&cancel);
    let freshgrant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &fresh)
        .unwrap();
    let freshwrite = authority
        .admit_write(&freshgrant, &fresh, ProjectWriteOperation::Memory)
        .unwrap();
    assert_eq!(
        NativeMemoryStore::resolve_lexical_index(
            fresh.snapshot(),
            &freshwrite,
            &result.recovery_key,
            write_limits()
        )
        .await
        .unwrap(),
        DeltaResolution::Applied {
            version: result.generation.sdk_version
        }
    );
    assert_eq!(sdk::pin(&f).await.version(), after.version());
    let foreign = Fixture::new_for(&ctx, "main", "01KSH1CRFFG8Z74B5ZS78WW809");
    admissions!(
        foreign,
        ctx,
        issuer2,
        authority2,
        grant2,
        _read2,
        write2,
        _bearer2,
        _reference2,
        _admin2
    );
    assert!(matches!(
        NativeMemoryStore::resolve_lexical_index(
            ctx.snapshot(),
            &write2,
            &result.recovery_key,
            write_limits()
        )
        .await,
        Err(MemoryStoreError::Branch(BranchError::RecoveryBinding))
    ));
    assert!(!foreign.project.runtime_root().join("memory").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lexical_index_inherited_validation_then_named_publication_isolated_from_main() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    let a = NativeMemoryStore::create(ctx.snapshot(), &write, input("main"), write_limits())
        .await
        .unwrap();
    let main = sdk::pin(&f).await;
    let rows = sdk::all(&main).await;
    let version = VersionContext::select(
        &graphit_core::project_paths::ProjectPaths::at(
            &f.root,
            &graphit_core::project_paths::InstallationPaths::resolve(
                f.installation.brand().clone(),
                &f.root,
                None,
                Some(f.installation.owner_root()),
            )
            .unwrap(),
        )
        .unwrap(),
        Checkout::NonGit,
        "feature",
        None,
        "",
    )
    .unwrap();
    let mut scope = f.scope();
    scope.project.as_mut().unwrap().version = &version;
    let (bearer, reference, _run, _admin) = f.issue_for(&issuer, &ctx, scope);
    let featuregrant = issuer
        .verify(bearer.expose_for_transport(), &reference, scope, &ctx)
        .unwrap();
    let featurewrite = authority
        .admit_write(&featuregrant, &ctx, ProjectWriteOperation::Memory)
        .unwrap();
    let featureread = authority
        .admit_read(&featuregrant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let inherited = NativeMemoryStore::get(ctx.snapshot(), &featureread, a.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert!(inherited.generation.inherited_target.is_some());
    let uri = url::Url::from_file_path(f.project.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string();
    assert!(matches!(
        lance::Dataset::open(&uri)
            .await
            .unwrap()
            .branches()
            .get(&version.lance_branch())
            .await,
        Err(lance::Error::RefNotFound { .. })
    ));
    let result = publication(
        NativeMemoryStore::maintain_lexical_index(
            ctx.snapshot(),
            &featurewrite,
            MemoryLexicalIndexOperation::Refresh,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(result.generation.inherited_target, None);
    assert_eq!(
        result.generation.physical_branch.as_deref(),
        Some(version.lance_branch().as_str())
    );
    assert_eq!(
        NativeMemoryStore::resolve_lexical_index(
            ctx.snapshot(),
            &featurewrite,
            &result.recovery_key,
            write_limits()
        )
        .await
        .unwrap(),
        DeltaResolution::Applied {
            version: result.generation.sdk_version
        }
    );
    assert!(matches!(
        NativeMemoryStore::resolve_lexical_index(
            ctx.snapshot(),
            &write,
            &result.recovery_key,
            write_limits()
        )
        .await,
        Err(MemoryStoreError::Branch(BranchError::RecoveryBinding))
    ));
    assert_eq!(sdk::pin(&f).await.version(), main.version());
    assert_eq!(sdk::all(&main).await, rows);
    let current = NativeMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.memory, a);
    let physical = result.recovery_key.physical_branch().unwrap();
    let dataset = lance::Dataset::open(&uri).await.unwrap();
    dataset.branches().delete(physical, false).await.unwrap();
    let fresh = request(&cancel);
    let freshgrant = issuer
        .verify(bearer.expose_for_transport(), &reference, scope, &fresh)
        .unwrap();
    let freshwrite = authority
        .admit_write(&freshgrant, &fresh, ProjectWriteOperation::Memory)
        .unwrap();
    assert_eq!(
        NativeMemoryStore::resolve_lexical_index(
            fresh.snapshot(),
            &freshwrite,
            &result.recovery_key,
            write_limits()
        )
        .await
        .unwrap(),
        DeltaResolution::Unresolved
    );
    assert!(matches!(
        dataset.branches().get(physical).await,
        Err(lance::Error::RefNotFound { .. })
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lexical_index_global_caps_and_late_chain_corruption_precede_index_build() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, _authority, _grant, _read, write, _bearer, _reference, _admin
    );
    NativeMemoryStore::create(ctx.snapshot(), &write, input("a"), write_limits())
        .await
        .unwrap();
    NativeMemoryStore::create(ctx.snapshot(), &write, input("b"), write_limits())
        .await
        .unwrap();
    let original = sdk::pin(&f).await;
    let batch = sdk::all(&original).await;
    let mut values = (0..batch.num_rows())
        .map(|r| sdk::string(&batch, 17, r).to_owned())
        .collect::<Vec<_>>();
    let last = values.len() - 1;
    values[last] = "{broken".into();
    sdk::replace(&original, sdk::text_column(&batch, 17, values)).await;
    let corrupt = sdk::pin(&f).await;
    let version = corrupt.version();
    assert!(matches!(
        NativeMemoryStore::maintain_lexical_index(
            ctx.snapshot(),
            &write,
            MemoryLexicalIndexOperation::Refresh,
            write_limits()
        )
        .await,
        Err(MemoryStoreError::Json(_))
    ));
    assert_eq!(sdk::pin(&f).await.version(), version);
    assert_eq!(
        corrupt.json_fts_index_state("payload").await.unwrap(),
        JsonFtsIndexState::Ready
    );
    sdk::replace(&corrupt, batch).await;
    let uri = url::Url::from_file_path(f.project.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string();
    let mut dataset = lance::Dataset::open(&uri).await.unwrap();
    dataset.delete("kind = 'head'").await.unwrap();
    let incomplete = sdk::pin(&f).await;
    assert!(matches!(
        NativeMemoryStore::maintain_lexical_index(
            ctx.snapshot(),
            &write,
            MemoryLexicalIndexOperation::Refresh,
            write_limits()
        )
        .await,
        Err(MemoryStoreError::IncompleteCorpus)
    ));
    assert_eq!(sdk::pin(&f).await.version(), incomplete.version());
    assert_eq!(
        incomplete.json_fts_index_state("payload").await.unwrap(),
        JsonFtsIndexState::Ready
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lexical_index_incomplete_chain_is_rejected_before_materializing_inherited_branch() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "feature");
    admissions!(
        f, ctx, issuer, _authority, _grant, _read, write, _bearer, _reference, _admin
    );
    sdk::empty_with_schema(&f, &ctx, sdk::schema()).await;
    let mainuri = url::Url::from_file_path(f.project.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string();
    let store = LanceBranchStore::open(&mainuri, sdk::schema(), None)
        .await
        .unwrap();
    assert!(
        store
            .capture_target(Some(f.version.lance_branch().as_str()))
            .await
            .unwrap()
            .inherited_target()
            .is_some()
    );
    // A schema-invalid existing corpus must fail before any named branch birth.
    let bad = sdk::schema()
        .as_ref()
        .clone()
        .with_metadata(std::collections::HashMap::new());
    let badfixture = Fixture::new(&ctx, "branch-bad");
    admissions!(
        badfixture,
        ctx,
        issuer2,
        _authority2,
        _grant2,
        _read2,
        write2,
        _bearer2,
        _reference2,
        _admin2
    );
    sdk::empty_with_schema(&badfixture, &ctx, Arc::new(bad)).await;
    assert!(
        NativeMemoryStore::maintain_lexical_index(
            ctx.snapshot(),
            &write2,
            MemoryLexicalIndexOperation::Ensure,
            write_limits()
        )
        .await
        .is_err()
    );
    let uri =
        url::Url::from_file_path(badfixture.project.runtime_root().join("memory/index.lance"))
            .unwrap()
            .to_string();
    let dataset = lance::Dataset::open(&uri).await.unwrap();
    assert!(matches!(
        dataset
            .branches()
            .get(&badfixture.version.lance_branch())
            .await,
        Err(lance::Error::RefNotFound { .. })
    ));
    // Empty typed corpus is valid and explicit Writer may materialize it.
    let published = publication(
        NativeMemoryStore::maintain_lexical_index(
            ctx.snapshot(),
            &write,
            MemoryLexicalIndexOperation::Ensure,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert!(published.generation.physical_branch.is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lexical_index_complete_eof_preserves_all_rows() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, _issuer, _authority, _grant, _read, write, _bearer, _reference, _admin
    );
    NativeMemoryStore::create(ctx.snapshot(), &write, input("a"), write_limits())
        .await
        .unwrap();
    NativeMemoryStore::create(ctx.snapshot(), &write, input("b"), write_limits())
        .await
        .unwrap();
    let pin = sdk::pin(&f).await;
    let rows = sdk::all(&pin).await;
    let result = publication(
        NativeMemoryStore::maintain_lexical_index(
            ctx.snapshot(),
            &write,
            MemoryLexicalIndexOperation::Refresh,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(result.generation.sdk_version, pin.version() + 1);
    assert_eq!(sdk::all(&sdk::pin(&f).await).await, rows);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lexical_index_read_only_revoke_cancel_and_deadline_gate_without_publication() {
    use graphit_auth::Permission;
    use graphit_core::module_settings::Module;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, _read, write, bearer, reference, admin
    );
    NativeMemoryStore::create(ctx.snapshot(), &write, input("a"), write_limits())
        .await
        .unwrap();
    let version = sdk::pin(&f).await.version();
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    let freshgrant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &fresh)
        .unwrap();
    let freshwrite = authority
        .admit_write(&freshgrant, &fresh, ProjectWriteOperation::Memory)
        .unwrap();
    fresh_cancel.cancel();
    assert!(
        NativeMemoryStore::maintain_lexical_index(
            fresh.snapshot(),
            &freshwrite,
            MemoryLexicalIndexOperation::Refresh,
            write_limits()
        )
        .await
        .is_err()
    );
    let expired_cancel = Cancellation::default();
    let expired = request_until(&expired_cancel, Instant::now() + Duration::from_secs(5));
    let expiredgrant = issuer
        .verify(
            bearer.expose_for_transport(),
            &reference,
            f.scope(),
            &expired,
        )
        .unwrap();
    let expiredwrite = authority
        .admit_write(&expiredgrant, &expired, ProjectWriteOperation::Memory)
        .unwrap();
    std::thread::sleep(Duration::from_secs(5));
    assert!(
        NativeMemoryStore::maintain_lexical_index(
            expired.snapshot(),
            &expiredwrite,
            MemoryLexicalIndexOperation::Refresh,
            write_limits()
        )
        .await
        .is_err()
    );
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(
        NativeMemoryStore::maintain_lexical_index(
            ctx.snapshot(),
            &write,
            MemoryLexicalIndexOperation::Refresh,
            write_limits()
        )
        .await
        .is_err()
    );
    assert_eq!(sdk::pin(&f).await.version(), version);
    let readfixture = Fixture::new(&ctx, "main");
    let readissuer = readfixture.issuer(&ctx);
    let (readbearer, readreference, _run, _admin) = readfixture.issue_role_for(
        &readissuer,
        &ctx,
        readfixture.scope(),
        graphit_hub::native::Role::Coordinator,
        &[Permission::Read(Module::Memory)],
    );
    let readgrant = readissuer
        .verify(
            readbearer.expose_for_transport(),
            &readreference,
            readfixture.scope(),
            &ctx,
        )
        .unwrap();
    let readauthority = ProjectRecordAuthority::new(&readissuer);
    assert!(
        readauthority
            .admit_read(&readgrant, &ctx, ProjectReadModule::Memory)
            .is_ok()
    );
    assert!(
        readauthority
            .admit_write(&readgrant, &ctx, ProjectWriteOperation::Memory)
            .is_err()
    );
    assert!(!readfixture.project.runtime_root().join("memory").exists());
}
