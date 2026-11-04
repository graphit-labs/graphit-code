use super::*;
use graphit_auth::Permission;
use graphit_core::module_settings::Module;
use graphit_data::lance_branch_store::JsonFtsIndexState;
use std::num::{NonZeroU32, NonZeroU64};

fn query(text: &str) -> MemoryLexicalQuery {
    MemoryLexicalQuery {
        text: text.into(),
        filter: MemoryListFilter::default(),
        include_deleted_history: false,
        top_k: NonZeroUsize::new(100).unwrap(),
    }
}
fn result(value: MemoryLexicalSearchRead) -> (MemoryGeneration, usize, Vec<MemoryLexicalHit>) {
    match value {
        MemoryLexicalSearchRead::Published {
            generation,
            total_hits,
            hits,
        } => (generation, total_hits, hits),
        other => panic!("expected Published: {other:?}"),
    }
}
fn update(memory: &Memory, key: &str, body: &str) -> UpdateMemory {
    UpdateMemory {
        expected_revision: memory.revision,
        idempotency_key: key.into(),
        reason: "excludedreasonkeyword".into(),
        title: None,
        body: Some(body.into()),
        classification: None,
        mandatory: None,
        important: None,
        tags: None,
        references: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lexical_search_real_index_then_merge_archives_delete_no_refresh() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    let mut authored = input("excludedkeykeyword");
    authored.title = "firsttitlekeyword".into();
    authored.body = "originalbodykeyword sharedkeyword".into();
    authored.tags = vec!["originaltagkeyword".into()];
    authored.references = Some(vec![MemoryReference {
        target_kind: ReferenceTargetKind::Knowledge,
        scope: MemoryReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap()),
        id: "excludedreferencekeyword".into(),
        context: None,
        relation: "exclusiveauditkeyword".into(),
        field: None,
    }]);
    let mut current = NativeMemoryStore::create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let old = sdk::pin(&f).await;
    let old_rows = sdk::all(&old).await;
    let (_, total, hits) = result(
        NativeMemoryStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("originalbodykeyword"),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 1);
    assert_eq!(hits[0].location, MemoryMatchLocation::Current);
    assert_eq!(hits[0].matched, current);
    let creation = current.clone();
    for revision in 2..=10 {
        let body = if revision == 2 || revision == 10 {
            "archivekeyword sharedkeyword".to_owned()
        } else {
            format!("differentkeyword sharedkeyword phase{revision}")
        };
        current = NativeMemoryStore::update(
            ctx.snapshot(),
            &write,
            current.id,
            update(&current, &format!("update-{revision}"), &body),
            write_limits(),
        )
        .await
        .unwrap();
    }
    let rev10 = current.clone();
    current = NativeMemoryStore::update(
        ctx.snapshot(),
        &write,
        current.id,
        update(&current, "update-11", "finalbodykeyword sharedkeyword"),
        write_limits(),
    )
    .await
    .unwrap();
    let pin = sdk::pin(&f).await;
    assert!(pin.version() > old.version());
    let (generation, total, hits) = result(
        NativeMemoryStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("finalbodykeyword"),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(generation.sdk_version, pin.version());
    assert_eq!(total, 1);
    assert_eq!(hits[0].location, MemoryMatchLocation::Current);
    assert_eq!(hits[0].current, current);
    let (_, total, hits) = result(
        NativeMemoryStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("archivekeyword"),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 1);
    assert_eq!(
        hits[0].location,
        MemoryMatchLocation::Archived {
            revision: NonZeroU64::new(10).unwrap()
        }
    );
    assert_eq!(hits[0].matched, rev10);
    assert_eq!(hits[0].current, current);
    assert!(!hits[0].deleted);
    let (_, _, hits) = result(
        NativeMemoryStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("originalbodykeyword"),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(
        hits[0].location,
        MemoryMatchLocation::Archived {
            revision: NonZeroU64::MIN
        }
    );
    assert_eq!(hits[0].matched, creation);
    let (_, _, hits) = result(
        NativeMemoryStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("sharedkeyword"),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].location, MemoryMatchLocation::Current);
    assert!(hits[0].score.is_finite());
    for text in ["firsttitlekeyword", "originaltagkeyword", "decision"] {
        assert_eq!(
            result(
                NativeMemoryStore::search_lexical(
                    ctx.snapshot(),
                    &read,
                    query(text),
                    read_limits()
                )
                .await
                .unwrap()
            )
            .1,
            1
        );
    }
    for text in [
        "excludedkeykeyword",
        "excludedreasonkeyword",
        "excludedreferencekeyword",
        "exclusiveauditkeyword",
        "nothingmatcheskeyword",
    ] {
        let (_, total, hits) = result(
            NativeMemoryStore::search_lexical(ctx.snapshot(), &read, query(text), read_limits())
                .await
                .unwrap(),
        );
        assert_eq!(total, 0);
        assert!(hits.is_empty());
    }
    let deleted = NativeMemoryStore::delete(
        ctx.snapshot(),
        &write,
        current.id,
        DeleteMemory {
            expected_revision: current.revision,
            idempotency_key: "delete-keyword".into(),
            reason: "deletedreasonkeyword".into(),
        },
        write_limits(),
    )
    .await
    .unwrap();
    let (_, total, hits) = result(
        NativeMemoryStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("finalbodykeyword"),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 0);
    assert!(hits.is_empty());
    let mut historical = query("finalbodykeyword");
    historical.include_deleted_history = true;
    let (_, total, hits) = result(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read, historical, read_limits())
            .await
            .unwrap(),
    );
    assert_eq!(total, 1);
    assert!(hits[0].deleted);
    assert_eq!(hits[0].current, deleted);
    assert_eq!(
        hits[0].location,
        MemoryMatchLocation::Archived {
            revision: NonZeroU64::new(deleted.revision).unwrap()
        }
    );
    assert_eq!(hits[0].matched, deleted);
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &read, deleted.id, read_limits())
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        matches!(NativeMemoryStore::list(ctx.snapshot(), &read, MemoryListFilter::default(), read_limits()).await.unwrap(), MemoryListRead::Published {memories,..} if memories.is_empty())
    );
    assert_eq!(sdk::pin(&f).await.version(), pin.version() + 1);
    assert_eq!(sdk::all(&old).await, old_rows);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lexical_search_late_fork_duplicate_and_bad_schema_fail_before_hits() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    let initial = NativeMemoryStore::create(
        ctx.snapshot(),
        &write,
        input("fork-original"),
        write_limits(),
    )
    .await
    .unwrap();
    NativeMemoryStore::update(
        ctx.snapshot(),
        &write,
        initial.id,
        update(&initial, "fork-event", "newbody"),
        write_limits(),
    )
    .await
    .unwrap();
    let pin = sdk::pin(&f).await;
    let good = sdk::all(&pin).await;
    let revision = sdk::select(&good, "revision");
    let mut payload: serde_json::Value =
        serde_json::from_str(sdk::string(&revision, 17, 0)).unwrap();
    payload["before"]["body"] = serde_json::Value::String("differentpastbody".into());
    sdk::replace(
        &pin,
        sdk::text_column(
            &revision,
            17,
            vec![serde_json::to_string(&payload).unwrap()],
        ),
    )
    .await;
    let mut limits = read_limits();
    limits.batch_rows = NonZeroU32::MIN;
    assert!(matches!(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read, query("Literal"), limits).await,
        Err(MemoryStoreError::Invalid(_))
    ));
    let bad = sdk::pin(&f).await;
    sdk::replace(&bad, revision).await;
    let uri = url::Url::from_file_path(f.project.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string();
    lance::Dataset::write(
        arrow_array::RecordBatchIterator::new(vec![Ok(sdk::select(&good, "head"))], sdk::schema()),
        &uri,
        Some(lance::dataset::WriteParams {
            mode: lance::dataset::WriteMode::Append,
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read, query("Literal"), limits).await,
        Err(MemoryStoreError::Invalid("duplicate Memory row key"))
    ));
    let other = Fixture::new_for(&ctx, "main", "01KSH1CRFFG8Z74B5ZS78WW809");
    admissions!(
        other,
        ctx,
        issuer2,
        authority2,
        grant2,
        read2,
        _write2,
        _bearer2,
        _reference2,
        _admin2
    );
    let wrong = Arc::new(arrow_schema::Schema::new(vec![arrow_schema::Field::new(
        "row_key",
        arrow_schema::DataType::Utf8,
        false,
    )]));
    sdk::empty_with_schema(&other, &ctx, wrong).await;
    assert!(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read2, query("Literal"), limits)
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lexical_search_absence_empty_shape_caps_and_full_validation() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    for text in ["", "  ", "; ;\n;"] {
        assert!(matches!(
            NativeMemoryStore::search_lexical(ctx.snapshot(), &read, query(text), read_limits())
                .await,
            Err(MemoryStoreError::Invalid(_))
        ));
    }
    let mut blanktag = query("valid");
    blanktag.filter.tag = Some("  ".into());
    assert!(matches!(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read, blanktag, read_limits()).await,
        Err(MemoryStoreError::Invalid(_))
    ));
    let mut tight = read_limits();
    tight.max_query_bytes = NonZeroUsize::new(100).unwrap();
    assert!(matches!(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read, query("valid"), tight).await,
        Err(MemoryStoreError::Limit("lexical expansion bytes"))
    ));
    assert_eq!(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read, query("valid"), read_limits())
            .await
            .unwrap(),
        MemoryLexicalSearchRead::NoCorpus
    );
    assert!(!f.project.runtime_root().join("memory").exists());
    sdk::empty_with_schema(&f, &ctx, sdk::schema()).await;
    let version = sdk::pin(&f).await.version();
    assert!(
        matches!(NativeMemoryStore::search_lexical(ctx.snapshot(), &read, query("valid"), read_limits()).await.unwrap(), MemoryLexicalSearchRead::PublishedEmpty {generation} if generation.sdk_version==version)
    );
    let a = NativeMemoryStore::create(ctx.snapshot(), &write, input("a"), write_limits())
        .await
        .unwrap();
    NativeMemoryStore::create(ctx.snapshot(), &write, input("b"), write_limits())
        .await
        .unwrap();
    let pin = sdk::pin(&f).await;
    let batch = sdk::all(&pin).await;
    let mut limits = read_limits();
    limits.batch_rows = NonZeroU32::MIN;
    assert_eq!(
        result(
            NativeMemoryStore::search_lexical(ctx.snapshot(), &read, query("Literal"), limits)
                .await
                .unwrap()
        )
        .1,
        2
    );
    let mut values: Vec<_> = (0..batch.num_rows())
        .map(|r| sdk::string(&batch, 17, r).to_owned())
        .collect();
    let last = values.len() - 1;
    let mut nofilter = query("Literal");
    nofilter.filter.tag = Some("excluded".into());
    values[last] = "{ malformed late JSON".into();
    sdk::replace(&pin, sdk::text_column(&batch, 17, values)).await;
    let corrupted = sdk::pin(&f).await;
    let corrupt_version = corrupted.version();
    // The fixture bypasses the product writer. FTS now reaches the SDK tokenizer before
    // typed candidate decoding, so invalid JSON can surface as a recoverable SDK error.
    let error = NativeMemoryStore::search_lexical(ctx.snapshot(), &read, nofilter, read_limits())
        .await
        .expect_err("malformed JSON must not produce a successful or partial response");
    eprintln!("corrupt unindexed JSON causal error: {error:?}");
    match error {
        MemoryStoreError::Json(_) => {}
        MemoryStoreError::Store(graphit_data::lance_module_store::StoreError::Sdk(cause)) => {
            let message = format!("{cause:?}");
            assert!(
                message.contains("JSON parse error") || message.contains("key must be a string"),
                "expected malformed JSON cause, got {message}"
            );
        }
        other => panic!("expected JSON codec/tokenizer error, got {other:?}"),
    }
    assert_eq!(
        sdk::pin(&f).await.version(),
        corrupt_version,
        "failed read must be readonly"
    );
    // Recovery is confined to this fixture: restore the original row, then query again
    // with the same runtime, request and admitted store. A caught worker panic must not
    // leave the executor or subsequent queries unusable.
    sdk::replace(&corrupted, batch).await;
    let restored_version = sdk::pin(&f).await.version();
    let (_, total, hits) = result(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read, query("Literal"), read_limits())
            .await
            .unwrap(),
    );
    assert_eq!(total, 2);
    assert_eq!(hits.len(), 2);
    assert_eq!(sdk::pin(&f).await.version(), restored_version);
    assert_eq!(a.revision, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lexical_search_current_metadata_filter_priority_unique_topk_and_literals() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    let long = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let long_unicode = "CAFÉβήτα".repeat(12);
    let mut normal = input("normal");
    normal.title = "ordinarytitle".into();
    normal.body = format!(
        "sharedneedle sharedneedle sharedneedle sharedneedle {long} {long_unicode} CAFÉ βήτα literaloperator AND NOT OR comma,newline\nword"
    );
    normal.tags = vec!["oldtag".into()];
    normal.attributes = MemoryAttributes {
        classification: MemoryClass::Decision,
        mandatory: false,
        important: true,
    };
    let normal = NativeMemoryStore::create(ctx.snapshot(), &write, normal, write_limits())
        .await
        .unwrap();
    let mut important = input("important");
    important.title = "importanttitle".into();
    important.body = "sharedneedle".into();
    important.attributes.mandatory = false;
    important.attributes.important = true;
    important.tags = vec!["currenttag".into()];
    let important = NativeMemoryStore::create(ctx.snapshot(), &write, important, write_limits())
        .await
        .unwrap();
    let mut mandatory = input("mandatory");
    mandatory.title = "mandatorytitle".into();
    mandatory.body = "sharedneedle".into();
    mandatory.attributes.important = true;
    mandatory.tags = vec!["currenttag".into()];
    let mandatory = NativeMemoryStore::create(ctx.snapshot(), &write, mandatory, write_limits())
        .await
        .unwrap();
    let mut change = update(&normal, "metadata-change", "freshcontent");
    change.tags = Some(vec!["currenttag".into()]);
    change.classification = Some(MemoryClass::Fact);
    change.important = Some(false);
    let current =
        NativeMemoryStore::update(ctx.snapshot(), &write, normal.id, change, write_limits())
            .await
            .unwrap();
    let mut q = query("sharedneedle");
    q.top_k = NonZeroUsize::MIN;
    let (_, total, hits) = result(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read, q.clone(), read_limits())
            .await
            .unwrap(),
    );
    assert_eq!(total, 3);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].current.id, mandatory.id);
    q.top_k = NonZeroUsize::new(10).unwrap();
    let (_, total, hits) = result(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read, q.clone(), read_limits())
            .await
            .unwrap(),
    );
    assert_eq!(total, 3);
    assert_eq!(
        hits.iter().map(|h| h.current.id).collect::<Vec<_>>(),
        [mandatory.id, important.id, normal.id]
    );
    assert_eq!(hits[2].matched, normal);
    assert_eq!(hits[2].current, current);
    assert_eq!(
        hits[2].location,
        MemoryMatchLocation::Archived {
            revision: NonZeroU64::MIN
        }
    );
    q.filter = MemoryListFilter {
        classification: Some(MemoryClass::Fact),
        tag: Some("currenttag".into()),
        mandatory: Some(false),
        important: Some(false),
        exclude_mandatory: true,
    };
    let (_, total, hits) = result(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read, q, read_limits())
            .await
            .unwrap(),
    );
    assert_eq!(total, 1);
    assert_eq!(hits[0].current, current);
    let mut oldtag = query("sharedneedle");
    oldtag.filter.tag = Some("oldtag".into());
    assert_eq!(
        result(
            NativeMemoryStore::search_lexical(ctx.snapshot(), &read, oldtag, read_limits())
                .await
                .unwrap()
        )
        .1,
        0
    );
    for literal in [
        long,
        &long_unicode,
        "café",
        "βήτα",
        "comma,newline\nword",
        "AND",
        "NOT",
        "OR",
        "  ; sharedneedle ; ; CAFÉ ; ",
    ] {
        assert!(
            result(
                NativeMemoryStore::search_lexical(
                    ctx.snapshot(),
                    &read,
                    query(literal),
                    read_limits()
                )
                .await
                .unwrap()
            )
            .1 > 0,
            "literal {literal}"
        );
    }
    assert_eq!(
        result(
            NativeMemoryStore::search_lexical(ctx.snapshot(), &read, query("cafe"), read_limits())
                .await
                .unwrap()
        )
        .1,
        0
    );
    let q = query("input.idempotency_key,str,normal");
    assert_eq!(
        result(
            NativeMemoryStore::search_lexical(ctx.snapshot(), &read, q, read_limits())
                .await
                .unwrap()
        )
        .1,
        0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lexical_search_read_roles_original_cancellation_revoke_and_path() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, admin
    );
    NativeMemoryStore::create(ctx.snapshot(), &write, input("role-record"), write_limits())
        .await
        .unwrap();
    let pin = sdk::pin(&f).await;
    let version = pin.version();
    let rows = sdk::all(&pin).await;
    let (rb, rr, _run, _rs) = f.issue_role_for(
        &issuer,
        &ctx,
        f.scope(),
        graphit_hub::native::Role::Coordinator,
        &[Permission::Read(Module::Memory)],
    );
    let rg = issuer
        .verify(rb.expose_for_transport(), &rr, f.scope(), &ctx)
        .unwrap();
    let readonly = authority
        .admit_read(&rg, &ctx, ProjectReadModule::Memory)
        .unwrap();
    assert_eq!(
        result(
            NativeMemoryStore::search_lexical(
                ctx.snapshot(),
                &readonly,
                query("Literal"),
                read_limits()
            )
            .await
            .unwrap()
        )
        .1,
        1
    );
    assert!(
        authority
            .admit_write(&rg, &ctx, ProjectWriteOperation::Memory)
            .is_err()
    );
    let wrong = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    assert!(matches!(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &wrong, query(""), read_limits()).await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    let othercancel = Cancellation::default();
    let other = request(&othercancel);
    assert!(matches!(
        NativeMemoryStore::search_lexical(other.snapshot(), &read, query("Literal"), read_limits())
            .await,
        Err(MemoryStoreError::DifferentRequest)
    ));
    let cancelled = Cancellation::default();
    let fresh = request(&cancelled);
    let fg = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &fresh)
        .unwrap();
    let fr = authority
        .admit_read(&fg, &fresh, ProjectReadModule::Memory)
        .unwrap();
    cancelled.cancel();
    assert!(
        NativeMemoryStore::search_lexical(fresh.snapshot(), &fr, query("Literal"), read_limits())
            .await
            .is_err()
    );
    let expiredcancel = Cancellation::default();
    let exp = request_until(&expiredcancel, Instant::now() + Duration::from_secs(5));
    let eg = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &exp)
        .unwrap();
    let er = authority
        .admit_read(&eg, &exp, ProjectReadModule::Memory)
        .unwrap();
    tokio::time::sleep_until(exp.snapshot().deadline().unwrap().into()).await;
    assert!(matches!(
        NativeMemoryStore::search_lexical(exp.snapshot(), &er, query("Literal"), read_limits()).await,
        Err(MemoryStoreError::Request(error)) if error.kind() == std::io::ErrorKind::TimedOut
    ));
    let path = f.project.runtime_root().join("memory");
    let saved = f.project.runtime_root().join("memory-saved");
    std::fs::rename(&path, &saved).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read, query("Literal"), read_limits())
            .await
            .is_err()
    );
    std::fs::remove_dir(&path).unwrap();
    std::fs::rename(&saved, &path).unwrap();
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read, query("Literal"), read_limits())
            .await
            .is_err()
    );
    assert_eq!(sdk::pin(&f).await.version(), version);
    assert_eq!(sdk::all(&sdk::pin(&f).await).await, rows);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lexical_search_real_120_physical_candidates_multibatch_collapse() {
    use futures::TryStreamExt;
    use lance_index::scalar::{
        FullTextSearchQuery,
        inverted::query::{FtsQuery, MatchQuery, Operator},
    };
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    let mut ids = Vec::new();
    for n in 0..60 {
        let mut v = input(&format!("eof-{n}"));
        v.body = "allcandidatekeyword".into();
        v.attributes.mandatory = n == 59;
        ids.push(
            NativeMemoryStore::create(ctx.snapshot(), &write, v, write_limits())
                .await
                .unwrap()
                .id,
        );
    }
    let pin = sdk::pin(&f).await;
    let matched =
        MatchQuery::new("body,str,allcandidatekeyword;memory.body,str,allcandidatekeyword".into())
            .with_column(Some("payload".into()))
            .with_operator(Operator::Or)
            .with_fuzziness(Some(0));
    let mut scan = pin.scan();
    scan.full_text_search(FullTextSearchQuery::new_query(FtsQuery::Match(matched)).limit(None))
        .unwrap();
    scan.project(&["row_key"]).unwrap();
    scan.disable_scoring_autoprojection();
    scan.batch_size(1);
    let mut stream = scan.try_into_stream().await.unwrap();
    let mut batches = 0;
    let mut candidates = 0;
    while let Some(batch) = stream.try_next().await.unwrap() {
        batches += 1;
        candidates += batch.num_rows();
    }
    assert_eq!(candidates, 120);
    assert!(batches >= 1);
    // FTS Take may emit all scores in one batch; the knob is a scan hint.
    let mut typed_scan = pin.scan();
    typed_scan.batch_size(1);
    let mut typed_stream = typed_scan.try_into_stream().await.unwrap();
    let mut typed_batches = 0;
    let mut typed_rows = 0;
    while let Some(batch) = typed_stream.try_next().await.unwrap() {
        typed_batches += 1;
        typed_rows += batch.num_rows();
    }
    assert_eq!(typed_rows, 180);
    assert!(typed_batches > 1);
    let mut limits = read_limits();
    limits.batch_rows = NonZeroU32::MIN;
    let mut q = query("allcandidatekeyword");
    q.top_k = NonZeroUsize::MIN;
    let (_, total, hits) = result(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read, q.clone(), limits)
            .await
            .unwrap(),
    );
    assert_eq!(total, 60);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].current.id, ids[59]);
    q.top_k = NonZeroUsize::new(60).unwrap();
    let (_, total, hits) = result(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read, q, limits)
            .await
            .unwrap(),
    );
    assert_eq!(total, 60);
    assert_eq!(hits.len(), 60);
    assert!(
        hits.iter()
            .all(|h| h.location == MemoryMatchLocation::Current)
    );
    assert_eq!(
        hits.iter()
            .map(|h| h.current.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        60
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lexical_search_wrong_physical_index_and_incomplete_do_not_return_empty() {
    use lance::{Dataset, index::DatasetIndexExt};
    use lance_index::{IndexType, scalar::InvertedIndexParams};
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    NativeMemoryStore::create(
        ctx.snapshot(),
        &write,
        input("wrong-policy"),
        write_limits(),
    )
    .await
    .unwrap();
    let uri = url::Url::from_file_path(f.project.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string();
    let params = InvertedIndexParams::default()
        .base_tokenizer("simple".into())
        .max_token_length(None)
        .lower_case(true)
        .stem(false)
        .remove_stop_words(false)
        .ascii_folding(false);
    let mut dataset = Dataset::open(&uri).await.unwrap();
    dataset
        .create_index(
            &["payload"],
            IndexType::Inverted,
            Some("wrong-text-content".into()),
            &params,
            false,
        )
        .await
        .unwrap();
    let pin = sdk::pin(&f).await;
    let version = pin.version();
    assert!(matches!(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read, query("Literal"), read_limits())
            .await,
        Err(MemoryStoreError::LexicalIndexUnavailable(
            JsonFtsIndexState::Incompatible
        ))
    ));
    assert_eq!(sdk::pin(&f).await.version(), version);
    let rows_before = sdk::all(&pin).await;
    assert!(matches!(
        NativeMemoryStore::create(
            ctx.snapshot(),
            &write,
            input("reject-incompatible-policy"),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::LexicalIndexUnavailable(
            JsonFtsIndexState::Incompatible
        ))
    ));
    let after_rejection = sdk::pin(&f).await;
    assert_eq!(after_rejection.version(), version);
    assert_eq!(sdk::all(&after_rejection).await, rows_before);
    let mut physical = Dataset::open(&uri).await.unwrap();
    physical.delete("kind = 'head'").await.unwrap();
    assert!(matches!(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read, query("Literal"), read_limits())
            .await,
        Err(MemoryStoreError::LexicalIndexUnavailable(
            JsonFtsIndexState::Incompatible
        ))
    ));
    // Integrity of a matching history remains mandatory when the index is usable.
    let incomplete = Fixture::new_for(&ctx, "main", "01KSH1CRFFG8Z74B5ZS78WW80A");
    admissions!(
        incomplete,
        ctx,
        issuer3,
        authority3,
        grant3,
        read3,
        write3,
        _bearer3,
        _reference3,
        _admin3
    );
    NativeMemoryStore::create(
        ctx.snapshot(),
        &write3,
        input("missing-head"),
        write_limits(),
    )
    .await
    .unwrap();
    let incomplete_uri =
        url::Url::from_file_path(incomplete.project.runtime_root().join("memory/index.lance"))
            .unwrap()
            .to_string();
    Dataset::open(&incomplete_uri)
        .await
        .unwrap()
        .delete("kind = 'head'")
        .await
        .unwrap();
    assert!(matches!(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read3, query("Literal"), read_limits())
            .await,
        Err(MemoryStoreError::IncompleteCorpus)
    ));
    let empty = Fixture::new_for(&ctx, "main", "01KSH1CRFFG8Z74B5ZS78WW809");
    admissions!(
        empty,
        ctx,
        issuer2,
        authority2,
        grant2,
        read2,
        _write2,
        _bearer2,
        _reference2,
        _admin2
    );
    drop(sdk::directory_only(&empty, &ctx).await);
    assert!(matches!(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read2, query("Literal"), read_limits())
            .await,
        Err(MemoryStoreError::IncompleteCorpus)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lexical_search_project_named_inherited_same_snapshot_isolation() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let main = Fixture::new(&ctx, "main");
    admissions!(
        main, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    let mut value = input("branch-record");
    value.body = "maincontentkeyword".into();
    let original = NativeMemoryStore::create(ctx.snapshot(), &write, value, write_limits())
        .await
        .unwrap();
    let mainpin = sdk::pin(&main).await;
    let version = mainpin.version();
    let feature = VersionContext::select(
        &graphit_core::project_paths::ProjectPaths::at(
            &main.root,
            &graphit_core::project_paths::InstallationPaths::resolve(
                main.installation.brand().clone(),
                &main.root,
                None,
                Some(main.installation.owner_root()),
            )
            .unwrap(),
        )
        .unwrap(),
        Checkout::NonGit,
        "Feature",
        None,
        "",
    )
    .unwrap();
    let mut scope = main.scope();
    scope.project.as_mut().unwrap().version = &feature;
    let (fb, fr, _run, _admin) = main.issue_for(&issuer, &ctx, scope);
    let fg = issuer
        .verify(fb.expose_for_transport(), &fr, scope, &ctx)
        .unwrap();
    let fre = authority
        .admit_read(&fg, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let fw = authority
        .admit_write(&fg, &ctx, ProjectWriteOperation::Memory)
        .unwrap();
    let (generation, total, hits) = result(
        NativeMemoryStore::search_lexical(
            ctx.snapshot(),
            &fre,
            query("maincontentkeyword"),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 1);
    assert_eq!(hits[0].current, original);
    assert_eq!(generation.project_version, "Feature");
    assert!(generation.inherited_target.is_some());
    let current = NativeMemoryStore::update(
        ctx.snapshot(),
        &fw,
        original.id,
        update(&original, "feature-update", "featurecontentkeyword"),
        write_limits(),
    )
    .await
    .unwrap();
    let (generation, total, hits) = result(
        NativeMemoryStore::search_lexical(
            ctx.snapshot(),
            &fre,
            query("featurecontentkeyword"),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 1);
    assert_eq!(hits[0].current, current);
    assert!(generation.inherited_target.is_none());
    assert_eq!(generation.physical_branch, Some(feature.lance_branch()));
    assert_eq!(
        result(
            NativeMemoryStore::search_lexical(
                ctx.snapshot(),
                &read,
                query("featurecontentkeyword"),
                read_limits()
            )
            .await
            .unwrap()
        )
        .1,
        0
    );
    assert_eq!(sdk::pin(&main).await.version(), version);
    assert_eq!(
        NativeMemoryStore::get(ctx.snapshot(), &read, original.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory,
        original
    );
    let other = Fixture::new_for(&ctx, "main", "01KSH1CRFFG8Z74B5ZS78WW809");
    admissions!(
        other,
        ctx,
        issuer2,
        authority2,
        grant2,
        read2,
        write2,
        _bearer2,
        _reference2,
        _admin2
    );
    assert_eq!(
        NativeMemoryStore::search_lexical(
            ctx.snapshot(),
            &read2,
            query("maincontentkeyword"),
            read_limits()
        )
        .await
        .unwrap(),
        MemoryLexicalSearchRead::NoCorpus
    );
    let mut value = input("foreign-record");
    value.body = "otherprojectkeyword".into();
    NativeMemoryStore::create(ctx.snapshot(), &write2, value, write_limits())
        .await
        .unwrap();
    let (generation, total, _) = result(
        NativeMemoryStore::search_lexical(
            ctx.snapshot(),
            &read2,
            query("maincontentkeyword"),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 0);
    assert_eq!(
        generation.project_id,
        "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lexical_search_validates_only_candidate_histories_on_the_captured_pin() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    let mut authored = input("candidate");
    authored.body = "uniquecandidatebody".into();
    let wanted = NativeMemoryStore::create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let unrelated =
        NativeMemoryStore::create(ctx.snapshot(), &write, input("unrelated"), write_limits())
            .await
            .unwrap();
    NativeMemoryStore::update(
        ctx.snapshot(),
        &write,
        unrelated.id,
        update(&unrelated, "unrelated-event", "unrelatedrevisedbody"),
        write_limits(),
    )
    .await
    .unwrap();
    let pin = sdk::pin(&f).await;
    let revision = sdk::select(&sdk::all(&pin).await, "revision");
    let mut payload: serde_json::Value =
        serde_json::from_str(sdk::string(&revision, 17, 0)).unwrap();
    payload["before"]["body"] = "differentpastbody".into();
    sdk::replace(
        &pin,
        sdk::text_column(
            &revision,
            17,
            vec![serde_json::to_string(&payload).unwrap()],
        ),
    )
    .await;
    let mut limits = read_limits();
    limits.batch_rows = NonZeroU32::MIN;
    let (_, total, hits) = result(
        NativeMemoryStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("uniquecandidatebody"),
            limits,
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 1);
    assert_eq!(hits[0].current, wanted);
    let (_, total, hits) = result(
        NativeMemoryStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("absentcandidatebody"),
            limits,
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 0);
    assert!(hits.is_empty());
    // Matching chains are still fully validated, even when current metadata excludes the hit.
    let mut filtered = query("unrelatedrevisedbody");
    filtered.filter.tag = Some("excluded".into());
    assert!(matches!(
        NativeMemoryStore::search_lexical(ctx.snapshot(), &read, filtered, limits).await,
        Err(MemoryStoreError::Invalid("Memory history fork"))
    ));
}
