use super::*;
use graphit_core::artifact_store::StoreOwner;
use std::num::NonZeroU32;

pub(super) fn limits() -> MemoryStoreReadLimits {
    read_limits()
}
fn project(f: &Fixture) -> graphit_core::project_identity::ProjectId {
    match f.project.owner() {
        StoreOwner::Project { project_id } => *project_id,
        _ => panic!("PROJECT"),
    }
}
fn reference(f: &Fixture, kind: ReferenceTargetKind, id: &str) -> MemoryReference {
    MemoryReference {
        target_kind: kind,
        scope: MemoryReferenceScope::Project(project(f)),
        id: id.into(),
        context: None,
        relation: " supports β \n".into(),
        field: Some(" body ".into()),
    }
}
pub(super) fn target(r: &MemoryReference) -> MemoryReferenceTarget {
    MemoryReferenceTarget {
        target_kind: r.target_kind,
        scope: r.scope.clone(),
        id: r.id.clone(),
        context: r.context.clone(),
    }
}
pub(super) fn query(
    id: Option<MemoryId>,
    target: Option<MemoryReferenceTarget>,
) -> MemoryReferencesQuery {
    MemoryReferencesQuery {
        outgoing: id,
        incoming: target,
        page_size: NonZeroUsize::new(100).unwrap(),
        outgoing_cursor: None,
        incoming_cursor: None,
    }
}
fn patch(key: &str, revision: u64) -> UpdateMemory {
    UpdateMemory {
        expected_revision: revision,
        idempotency_key: key.into(),
        reason: "reference lifecycle".into(),
        title: None,
        body: None,
        classification: None,
        mandatory: None,
        important: None,
        tags: None,
        references: None,
    }
}
pub(super) type ReferencePages = (
    MemoryGeneration,
    Option<Box<MemoryReferencePage>>,
    Option<Box<MemoryReferencePage>>,
    MemoryReferencesCoverage,
    Vec<MemoryReferencesWarning>,
);

pub(super) fn pages(read: MemoryReferencesRead) -> ReferencePages {
    match read {
        MemoryReferencesRead::Published {
            generation,
            outgoing,
            incoming,
            coverage,
            warnings,
        } => (generation, outgoing, incoming, coverage, warnings),
        _ => panic!("published"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reference_literals_duplicates_arbitrary_targets_and_own_rich_same_pin() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f,
        ctx,
        issuer,
        authority,
        grant,
        read,
        write,
        _bearer,
        _credential,
        _admin
    );
    let anchor = NativeMemoryStore::create(ctx.snapshot(), &write, input("target"), write_limits())
        .await
        .unwrap();
    let own = reference(&f, ReferenceTargetKind::Memory, &anchor.id.to_string());
    let task = reference(&f, ReferenceTargetKind::Task, " tsk-literal β ");
    let session = reference(&f, ReferenceTargetKind::Session, "ses-literal");
    let mut user = reference(&f, ReferenceTargetKind::Memory, "user-memory");
    user.scope = MemoryReferenceScope::User(" issuer/sub β ".into());
    let mut imported = reference(&f, ReferenceTargetKind::Knowledge, "doc#literal");
    imported.context = Some("catalog@v1".into());
    let mut foreign = own.clone();
    foreign.scope = MemoryReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap());
    let refs = vec![
        own.clone(),
        own.clone(),
        task.clone(),
        session.clone(),
        user.clone(),
        imported.clone(),
        foreign,
    ];
    let mut authored = input("source");
    authored.references = Some(refs.clone());
    let source = NativeMemoryStore::create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let (g, out, incoming, coverage, warnings) = pages(
        NativeMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(Some(source.id), Some(target(&own))),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert!(
        coverage.memory_project_source_complete
            && coverage.inbound_other_sources_not_qualified
            && warnings.is_empty()
    );
    let out = out.unwrap();
    assert!(out.complete);
    assert_eq!(out.total, refs.len());
    for (ordinal, edge) in out.results.iter().enumerate() {
        assert_eq!(edge.ordinal, ordinal);
        assert_eq!(edge.reference, refs[ordinal]);
        assert_eq!(edge.source_revision, 1);
        assert_eq!(edge.reference_generation, 1);
        assert_eq!(edge.source_snapshot, g);
        assert_eq!(
            edge.source_observation,
            MemoryReferenceResolution::Memory {
                id: source.id,
                title: source.title.clone()
            }
        );
        assert_eq!(
            edge.target_resolution,
            if ordinal < 2 {
                MemoryReferenceResolution::Memory {
                    id: anchor.id,
                    title: anchor.title.clone(),
                }
            } else {
                MemoryReferenceResolution::Unresolved
            }
        );
    }
    assert_eq!(incoming.unwrap().total, 2);
    for r in [&task, &session, &user, &imported] {
        let (_, outgoing, incoming, _, _) = pages(
            NativeMemoryStore::references(
                ctx.snapshot(),
                &read,
                query(None, Some(target(r))),
                limits(),
            )
            .await
            .unwrap(),
        );
        assert!(outgoing.is_none());
        let incoming = incoming.unwrap();
        assert_eq!(incoming.total, 1);
        assert_eq!(incoming.results[0].source, source.id);
        assert_eq!(incoming.results[0].reference, *r);
        assert_eq!(
            incoming.results[0].target_resolution,
            MemoryReferenceResolution::Unresolved
        );
    }
    let (_, outgoing, _, _, _) = pages(
        NativeMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(Some(anchor.id), None),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert!(outgoing.unwrap().results.is_empty());
    let raw = sdk::all(&sdk::pin(&f).await).await;
    assert_eq!(
        (0..raw.num_rows())
            .filter(|i| sdk::string(&raw, 1, *i) == "references")
            .count(),
        2
    );
    assert!(!sdk::string(&sdk::select(&raw, "references"), 17, 0).contains("Literal body"));
    assert_eq!(sdk::pin(&f).await.version(), g.sdk_version);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reference_publications_preserve_domain_generation_replay_and_delete_history() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f,
        ctx,
        issuer,
        authority,
        grant,
        read,
        write,
        _bearer,
        _credential,
        _admin
    );
    let r = reference(&f, ReferenceTargetKind::Task, "tsk-target");
    let mut authored = input("create");
    authored.references = Some(vec![r.clone()]);
    let initial =
        NativeMemoryStore::create(ctx.snapshot(), &write, authored.clone(), write_limits())
            .await
            .unwrap();
    let version = sdk::pin(&f).await.version();
    assert_eq!(
        NativeMemoryStore::create(ctx.snapshot(), &write, authored, write_limits())
            .await
            .unwrap(),
        initial
    );
    assert_eq!(sdk::pin(&f).await.version(), version);
    let mut p = patch("body", 1);
    p.body = Some("new body".into());
    let body = NativeMemoryStore::update(
        ctx.snapshot(),
        &write,
        initial.id,
        p.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(body.reference_generation, 1);
    let version = sdk::pin(&f).await.version();
    assert_eq!(
        NativeMemoryStore::update(ctx.snapshot(), &write, initial.id, p, write_limits())
            .await
            .unwrap(),
        body
    );
    assert_eq!(sdk::pin(&f).await.version(), version);
    let mut p = patch("clear", 2);
    p.references = Some(vec![]);
    let clear = NativeMemoryStore::update(ctx.snapshot(), &write, initial.id, p, write_limits())
        .await
        .unwrap();
    assert_eq!(clear.reference_generation, 2);
    let (_, out, incoming, _, _) = pages(
        NativeMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(Some(initial.id), Some(target(&r))),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(out.unwrap().total, 0);
    assert_eq!(incoming.unwrap().total, 0);
    let version = sdk::pin(&f).await.version();
    assert!(matches!(
        NativeMemoryStore::update(
            ctx.snapshot(),
            &write,
            initial.id,
            patch("no-change", 3),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::NoChange)
    ));
    assert_eq!(sdk::pin(&f).await.version(), version);
    let deleted = NativeMemoryStore::delete(
        ctx.snapshot(),
        &write,
        initial.id,
        DeleteMemory {
            expected_revision: 3,
            idempotency_key: "delete".into(),
            reason: "explicit".into(),
        },
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(deleted.reference_generation, 3);
    assert!(matches!(
        NativeMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(Some(initial.id), None),
            limits()
        )
        .await,
        Err(MemoryStoreError::NotFound)
    ));
    let (_, _, incoming, coverage, warnings) = pages(
        NativeMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(None, Some(target(&r))),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(incoming.unwrap().total, 0);
    assert!(coverage.memory_project_source_complete && warnings.is_empty());
    let h = NativeMemoryStore::history(ctx.snapshot(), &read, initial.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(h.revisions.len(), 3);
    assert_eq!(h.creation.memory.references, vec![r]);
    let raw = sdk::all(&sdk::pin(&f).await).await;
    let mut revisions = (0..raw.num_rows())
        .filter(|i| sdk::string(&raw, 1, *i) == "references")
        .map(|i| {
            serde_json::from_str::<serde_json::Value>(sdk::string(&raw, 17, i)).unwrap()["revision"]
                .as_u64()
                .unwrap()
        })
        .collect::<Vec<_>>();
    revisions.sort();
    assert_eq!(revisions, vec![1, 2, 3, 4]);
    assert_eq!(raw.num_rows(), 9);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reference_closed_cursor_bindings_pagination_and_cumulative_caps() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f,
        ctx,
        issuer,
        authority,
        grant,
        read,
        write,
        _bearer,
        _credential,
        _admin
    );
    let r = reference(&f, ReferenceTargetKind::Task, "task");
    let mut authored = input("cursor");
    authored.references = Some(vec![r.clone(), r.clone(), r.clone()]);
    let m = NativeMemoryStore::create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let mut q = query(Some(m.id), Some(target(&r)));
    q.page_size = NonZeroUsize::MIN;
    let (_, outgoing, incoming, _, _) = pages(
        NativeMemoryStore::references(ctx.snapshot(), &read, q.clone(), limits())
            .await
            .unwrap(),
    );
    let out = outgoing.unwrap();
    assert_eq!(out.results[0].ordinal, 0);
    let cursor = out.next_cursor.unwrap();
    let incoming_cursor = incoming.unwrap().next_cursor.unwrap();
    q.outgoing_cursor = Some(cursor.clone());
    q.incoming_cursor = Some(incoming_cursor);
    q.page_size = NonZeroUsize::new(2).unwrap();
    let (_, outgoing, incoming, _, _) = pages(
        NativeMemoryStore::references(ctx.snapshot(), &read, q.clone(), limits())
            .await
            .unwrap(),
    );
    assert_eq!(
        outgoing
            .unwrap()
            .results
            .iter()
            .map(|e| e.ordinal)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(incoming.unwrap().results.len(), 2);
    let mut encoded = serde_json::to_value(&cursor).unwrap();
    encoded["extra"] = true.into();
    assert!(serde_json::from_value::<MemoryReferencesCursor>(encoded).is_err());
    let mut changed_policy = limits();
    changed_policy.batch_rows = NonZeroU32::MIN;
    changed_policy.max_query_bytes = NonZeroUsize::MIN;
    let mut changed = q.clone();
    changed.outgoing_cursor = Some(cursor.clone());
    assert!(
        NativeMemoryStore::references(ctx.snapshot(), &read, changed, changed_policy)
            .await
            .is_ok()
    );
    let mut encoded = serde_json::to_value(&cursor).unwrap();
    encoded["offset"] = 3.into();
    let mut at_end = q.clone();
    at_end.outgoing_cursor = Some(serde_json::from_value(encoded.clone()).unwrap());
    assert!(
        pages(
            NativeMemoryStore::references(ctx.snapshot(), &read, at_end, limits())
                .await
                .unwrap()
        )
        .1
        .unwrap()
        .results
        .is_empty()
    );
    encoded["offset"] = 4.into();
    let mut beyond = q.clone();
    beyond.outgoing_cursor = Some(serde_json::from_value(encoded).unwrap());
    assert!(
        NativeMemoryStore::references(ctx.snapshot(), &read, beyond, limits())
            .await
            .is_err()
    );
    let mut p = patch("later", 1);
    p.title = Some("later".into());
    NativeMemoryStore::update(ctx.snapshot(), &write, m.id, p, write_limits())
        .await
        .unwrap();
    assert!(matches!(
        NativeMemoryStore::references(ctx.snapshot(), &read, q, limits()).await,
        Err(MemoryStoreError::Invalid("Memory reference cursor"))
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reference_no_corpus_preflight_and_fresh_original_authority_are_read_only() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, credential, admin
    );
    let r = reference(&f, ReferenceTargetKind::Task, "task");
    let q = query(None, Some(target(&r)));
    assert_eq!(
        NativeMemoryStore::references(ctx.snapshot(), &read, q.clone(), limits())
            .await
            .unwrap(),
        MemoryReferencesRead::NoCorpus
    );
    let mut malformed = q.clone();
    malformed.incoming.as_mut().unwrap().context = Some("not-knowledge".into());
    assert!(matches!(
        NativeMemoryStore::references(ctx.snapshot(), &read, malformed, limits()).await,
        Err(MemoryStoreError::References(_))
    ));
    assert!(
        NativeMemoryStore::references(ctx.snapshot(), &read, query(None, None), limits())
            .await
            .is_err()
    );
    let task = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    assert!(matches!(
        NativeMemoryStore::references(ctx.snapshot(), &task, q.clone(), limits()).await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    let other_cancel = Cancellation::default();
    let other = request(&other_cancel);
    assert!(matches!(
        NativeMemoryStore::references(other.snapshot(), &read, q.clone(), limits()).await,
        Err(MemoryStoreError::DifferentRequest)
    ));
    assert!(!f.project.runtime_root().join("memory").exists());
    NativeMemoryStore::create(ctx.snapshot(), &write, input("live"), write_limits())
        .await
        .unwrap();
    let version = sdk::pin(&f).await.version();
    let fresh = issuer
        .verify(
            bearer.expose_for_transport(),
            &credential,
            f.scope(),
            &other,
        )
        .unwrap();
    let admission = authority
        .admit_read(&fresh, &other, ProjectReadModule::Memory)
        .unwrap();
    other_cancel.cancel();
    assert!(matches!(
        NativeMemoryStore::references(other.snapshot(), &admission, q.clone(), limits()).await,
        Err(MemoryStoreError::Request(_))
    ));
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(matches!(
        NativeMemoryStore::references(ctx.snapshot(), &read, q, limits()).await,
        Err(MemoryStoreError::Auth(_))
    ));
    assert_eq!(sdk::pin(&f).await.version(), version);
}

async fn replace_fixture(f: &Fixture, batches: Vec<arrow_array::RecordBatch>) {
    let uri = url::Url::from_file_path(f.project.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string();
    let store = graphit_data::lance_module_store::LanceModuleStore::open(&uri, sdk::schema())
        .await
        .unwrap();
    store
        .snapshot()
        .prepare_replace(batches)
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reference_missing_current_marker_reports_incomplete_and_never_adopts_history() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f,
        ctx,
        issuer,
        authority,
        grant,
        read,
        write,
        _bearer,
        _credential,
        _admin
    );
    let r = reference(&f, ReferenceTargetKind::Session, "session");
    let mut authored = input("missing");
    authored.references = Some(vec![r.clone()]);
    let initial = NativeMemoryStore::create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let mut p = patch("body", 1);
    p.body = Some("next body".into());
    NativeMemoryStore::update(ctx.snapshot(), &write, initial.id, p, write_limits())
        .await
        .unwrap();
    let raw = sdk::all(&sdk::pin(&f).await).await;
    let keep = (0..raw.num_rows())
        .filter(|i| sdk::string(&raw, 0, *i) != format!("references/{}/2", initial.id))
        .map(|i| raw.slice(i, 1))
        .collect();
    replace_fixture(&f, keep).await;
    let version = sdk::pin(&f).await.version();
    let (_, outgoing, incoming, coverage, warnings) = pages(
        NativeMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(Some(initial.id), Some(target(&r))),
            limits(),
        )
        .await
        .unwrap(),
    );
    let outgoing = outgoing.unwrap();
    let incoming = incoming.unwrap();
    assert!(
        !outgoing.complete
            && !incoming.complete
            && outgoing.results.is_empty()
            && incoming.results.is_empty()
    );
    assert!(!coverage.memory_project_source_complete);
    assert_eq!(
        warnings,
        vec![MemoryReferencesWarning::MissingCurrentProjection {
            source: initial.id,
            revision: 2
        }]
    );
    assert_eq!(sdk::pin(&f).await.version(), version);
    replace_fixture(&f, vec![raw]).await;
    let (_, outgoing, incoming, coverage, warnings) = pages(
        NativeMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(Some(initial.id), Some(target(&r))),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert!(coverage.memory_project_source_complete && warnings.is_empty());
    assert_eq!(outgoing.unwrap().total, 1);
    assert_eq!(incoming.unwrap().total, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reference_marker_late_corruption_duplicate_foreign_and_old_orphan_reject_full_eof() {
    for mode in 0..6 {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        admissions!(
            f,
            ctx,
            issuer,
            authority,
            grant,
            read,
            write,
            _bearer,
            _credential,
            _admin
        );
        let r = reference(&f, ReferenceTargetKind::Task, "task");
        let mut authored = input("eof");
        authored.references = Some(vec![r.clone()]);
        let initial = NativeMemoryStore::create(ctx.snapshot(), &write, authored, write_limits())
            .await
            .unwrap();
        let mut p = patch("body", 1);
        p.body = Some("next body".into());
        NativeMemoryStore::update(ctx.snapshot(), &write, initial.id, p, write_limits())
            .await
            .unwrap();
        let raw = sdk::all(&sdk::pin(&f).await).await;
        let row = (0..raw.num_rows())
            .find(|i| sdk::string(&raw, 0, *i) == format!("references/{}/1", initial.id))
            .unwrap();
        let old = raw.slice(row, 1);
        let replacement = match mode {
            0 => vec![raw.clone(), old.clone()],
            1 => {
                let bad = sdk::text_column(&old, 17, vec!["{ invalid old payload".into()]);
                let mut batches = (0..raw.num_rows())
                    .filter(|i| *i != row)
                    .map(|i| raw.slice(i, 1))
                    .collect::<Vec<_>>();
                batches.push(bad);
                batches
            }
            2 => {
                let bad = sdk::text_column(&old, 3, vec!["01KSH1CRFFG8Z74B5ZS78WW809".into()]);
                let mut batches = (0..raw.num_rows())
                    .filter(|i| *i != row)
                    .map(|i| raw.slice(i, 1))
                    .collect::<Vec<_>>();
                batches.push(bad);
                batches
            }
            3 => {
                let orphan = "01KSH1CRFFG8Z74B5ZS78WW810";
                let payload = sdk::string(&old, 17, 0)
                    .replace(&initial.id.to_string(), orphan)
                    .replace("\"reference_generation\":1", "\"reference_generation\":2");
                let bad = sdk::text_column(&old, 0, vec![format!("references/{orphan}/1")]);
                let bad = sdk::text_column(&bad, 2, vec![orphan.into()]);
                let bad = sdk::text_column(&bad, 17, vec![payload]);
                let bad = sdk::numbers(&bad, 15, vec![2]);
                vec![raw.clone(), bad]
            }
            4 => {
                let head = sdk::select(&raw, "head");
                let mut memory: Memory = serde_json::from_str(sdk::string(&head, 17, 0)).unwrap();
                memory.references[0].id = "different".into();
                let bad = sdk::text_column(
                    &head,
                    16,
                    vec![serde_json::to_string(&memory.references).unwrap()],
                );
                let bad = sdk::text_column(&bad, 17, vec![serde_json::to_string(&memory).unwrap()]);
                let mut batches = (0..raw.num_rows())
                    .filter(|i| sdk::string(&raw, 1, *i) != "head")
                    .map(|i| raw.slice(i, 1))
                    .collect::<Vec<_>>();
                batches.push(bad);
                batches
            }
            _ => {
                let mut columns = old.columns().to_vec();
                columns[22] = Arc::new(arrow_array::UInt32Array::from(vec![Some(1)]));
                let bad = arrow_array::RecordBatch::try_new(old.schema(), columns).unwrap();
                let mut batches = (0..raw.num_rows())
                    .filter(|i| *i != row)
                    .map(|i| raw.slice(i, 1))
                    .collect::<Vec<_>>();
                batches.push(bad);
                batches
            }
        };
        replace_fixture(&f, replacement).await;
        let version = sdk::pin(&f).await.version();
        let mut bounded = limits();
        bounded.batch_rows = NonZeroU32::MIN;
        assert!(
            NativeMemoryStore::references(
                ctx.snapshot(),
                &read,
                query(Some(initial.id), Some(target(&r))),
                bounded
            )
            .await
            .is_err(),
            "mode {mode} accepted a prefix"
        );
        assert_eq!(sdk::pin(&f).await.version(), version);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reference_named_inheritance_preserves_head_origin_and_pin_isolation() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let mut f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let authority = ProjectRecordAuthority::new(&issuer);
    let (a, ar, _, _admin) = f.issue(&issuer, &ctx);
    let r = reference(&f, ReferenceTargetKind::Task, "task");
    let (initial, default_generation) = {
        let grant = issuer
            .verify(a.expose_for_transport(), &ar, f.scope(), &ctx)
            .unwrap();
        let write = authority
            .admit_write(&grant, &ctx, ProjectWriteOperation::Memory)
            .unwrap();
        let read = authority
            .admit_read(&grant, &ctx, ProjectReadModule::Memory)
            .unwrap();
        let mut authored = input("branch");
        authored.references = Some(vec![r.clone(), r.clone()]);
        let m = NativeMemoryStore::create(ctx.snapshot(), &write, authored, write_limits())
            .await
            .unwrap();
        let generation = pages(
            NativeMemoryStore::references(ctx.snapshot(), &read, query(Some(m.id), None), limits())
                .await
                .unwrap(),
        )
        .0;
        (m, generation)
    };
    let paths = graphit_core::project_paths::ProjectPaths::at(
        &f.root,
        &graphit_core::project_paths::InstallationPaths::resolve(
            graphit_core::brand::Brand::default(),
            &f.root,
            None,
            Some(f.installation.owner_root()),
        )
        .unwrap(),
    )
    .unwrap();
    f.version =
        VersionContext::select(&paths, Checkout::NonGit, "reference-feature", None, "").unwrap();
    let (b, br, _, _admin2) = f.issue_for(&issuer, &ctx, f.scope());
    {
        let grant = issuer
            .verify(b.expose_for_transport(), &br, f.scope(), &ctx)
            .unwrap();
        let read = authority
            .admit_read(&grant, &ctx, ProjectReadModule::Memory)
            .unwrap();
        let write = authority
            .admit_write(&grant, &ctx, ProjectWriteOperation::Memory)
            .unwrap();
        let mut q = query(Some(initial.id), Some(target(&r)));
        q.page_size = NonZeroUsize::MIN;
        let (g, out, _, coverage, _) = pages(
            NativeMemoryStore::references(ctx.snapshot(), &read, q.clone(), limits())
                .await
                .unwrap(),
        );
        assert!(g.inherited_target.is_some() && coverage.memory_project_source_complete);
        assert_eq!(g.sdk_version, default_generation.sdk_version);
        assert_eq!(g.project_version, "reference-feature");
        q.outgoing_cursor = out.unwrap().next_cursor;
        let mut p = patch("named", 1);
        p.body = Some("named body".into());
        NativeMemoryStore::update(ctx.snapshot(), &write, initial.id, p, write_limits())
            .await
            .unwrap();
        assert!(
            NativeMemoryStore::references(ctx.snapshot(), &read, q, limits())
                .await
                .is_err()
        );
        let (g, out, incoming, coverage, _) = pages(
            NativeMemoryStore::references(
                ctx.snapshot(),
                &read,
                query(Some(initial.id), Some(target(&r))),
                limits(),
            )
            .await
            .unwrap(),
        );
        assert!(g.inherited_target.is_none() && coverage.memory_project_source_complete);
        assert_eq!(out.unwrap().results[0].source_revision, 2);
        assert_eq!(incoming.unwrap().total, 2);
        let physical = sdk::all(&sdk::pin(&f).await).await;
        let marker = (0..physical.num_rows())
            .find(|i| sdk::string(&physical, 0, *i) == format!("references/{}/2", initial.id))
            .unwrap();
        assert_eq!(sdk::string(&physical, 4, marker), "main");
    }
    f.version = VersionContext::select(&paths, Checkout::NonGit, "main", None, "").unwrap();
    let grant = issuer
        .verify(a.expose_for_transport(), &ar, f.scope(), &ctx)
        .unwrap();
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let (g, out, _, _, _) = pages(
        NativeMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(Some(initial.id), None),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(g, default_generation);
    assert_eq!(out.unwrap().results[0].source_revision, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reference_payload_disagreement_fails_even_when_header_is_empty() {
    for kind in ["head", "references"] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        admissions!(
            f,
            ctx,
            issuer,
            authority,
            grant,
            read,
            write,
            _bearer,
            _credential,
            _admin
        );
        let m =
            NativeMemoryStore::create(ctx.snapshot(), &write, input("raw-budget"), write_limits())
                .await
                .unwrap();
        let pin = sdk::pin(&f).await;
        let raw = sdk::all(&pin).await;
        let row = sdk::select(&raw, kind);
        let refs =
            serde_json::to_string(&vec![reference(&f, ReferenceTargetKind::Task, "task"); 2])
                .unwrap();
        let payload = sdk::string(&row, 17, 0)
            .replace("\"references\":[]", &format!("\"references\":{refs}"));
        assert_ne!(payload, sdk::string(&row, 17, 0));
        let payloads = (0..raw.num_rows())
            .map(|index| {
                if sdk::string(&raw, 1, index) == kind {
                    payload.clone()
                } else {
                    sdk::string(&raw, 17, index).to_owned()
                }
            })
            .collect();
        sdk::replace(&pin, sdk::text_column(&raw, 17, payloads)).await;
        let version = sdk::pin(&f).await.version();
        let expected = if kind == "head" {
            "Memory header disagrees"
        } else {
            "Memory reference projection header"
        };
        let error =
            NativeMemoryStore::references(ctx.snapshot(), &read, query(Some(m.id), None), limits())
                .await
                .unwrap_err();
        assert!(
            matches!(&error, MemoryStoreError::Invalid(message) if *message == expected),
            "{kind}: {error}"
        );
        assert_eq!(sdk::pin(&f).await.version(), version);
    }
}
