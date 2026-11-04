use super::*;
use arrow_array::{Float32Array, RecordBatch, StringArray};
use futures::TryStreamExt;
use graphit_data::lance_branch_store::{BranchSnapshot, JsonFtsIndexState, LanceBranchStore};
use graphit_records::{gates::TaskStatus, task_spec::FieldUpdate};
use lance_index::scalar::{
    FullTextSearchQuery,
    inverted::query::{FtsQuery, MatchQuery, Operator},
};
use std::collections::HashMap;

fn query_max_bytes() -> NonZeroUsize {
    NonZeroUsize::new(16 * 1024 * 1024).unwrap()
}
fn query(text: &str) -> TaskLexicalQuery {
    TaskLexicalQuery {
        mode: graphit_records::task_store::TaskSearchMode::Fts,
        text: text.into(),
        filter: TaskSearchFilter::default(),
        page_size: NonZeroUsize::new(100).unwrap(),
        top_k: NonZeroUsize::new(100).unwrap(),
        cursor: None,
    }
}
async fn search(
    original: &Arc<SnapshotContext>,
    read: &graphit_auth::ProjectReadAdmission<'_>,
    query: TaskLexicalQuery,
) -> TaskLexicalSearchRead {
    NativeTaskStore::search_lexical(original, read, query, read_limits(), query_max_bytes())
        .await
        .unwrap()
}
fn published(
    value: TaskLexicalSearchRead,
) -> (
    TaskGeneration,
    usize,
    Vec<TaskLexicalHit>,
    Option<TaskSearchCursor>,
) {
    let TaskLexicalSearchRead::Published {
        generation,
        total_hits,
        results,
        next_cursor,
    } = value
    else {
        panic!("published search: {value:?}")
    };
    (generation, total_hits, results, next_cursor)
}
async fn pin(f: &Fixture) -> BranchSnapshot {
    let uri = url::Url::from_file_path(f.project.runtime_root().join("task/index.lance"))
        .unwrap()
        .to_string();
    let dataset = lance::dataset::builder::DatasetBuilder::from_uri(&uri)
        .load()
        .await
        .unwrap();
    let store = LanceBranchStore::open(
        &uri,
        Arc::new(arrow_schema::Schema::from(dataset.schema())),
        None,
    )
    .await
    .unwrap();
    let branch = f.version.lance_branch();
    store
        .capture_target((!branch.is_empty()).then_some(branch.as_str()))
        .await
        .unwrap()
}
async fn all(pin: &BranchSnapshot) -> Vec<RecordBatch> {
    pin.scan()
        .try_into_stream()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap()
}
fn text<'a>(batch: &'a RecordBatch, name: &str, row: usize) -> &'a str {
    batch
        .column_by_name(name)
        .unwrap()
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap()
        .value(row)
}
async fn scores(pin: &BranchSnapshot, term: &str) -> (Vec<(String, String, f32)>, Vec<usize>) {
    assert_eq!(
        pin.json_fts_index_state("public_search").await.unwrap(),
        JsonFtsIndexState::Ready
    );
    let expanded = [
        "title",
        "description",
        "check_text",
        "evidence",
        "comment_body",
    ]
    .map(|p| format!("{p},str,{term}"))
    .join(";");
    let matched = MatchQuery::new(expanded)
        .with_column(Some("public_search".into()))
        .with_operator(Operator::Or)
        .with_fuzziness(Some(0));
    let mut scan = pin.scan();
    scan.full_text_search(FullTextSearchQuery::new_query(FtsQuery::Match(matched)).limit(None))
        .unwrap();
    scan.project(&["row_key", "record_id", "_score"]).unwrap();
    scan.disable_scoring_autoprojection();
    scan.batch_size(1);
    let mut stream = scan.try_into_stream().await.unwrap();
    let mut rows = vec![];
    let mut sizes = vec![];
    while let Some(batch) = stream.try_next().await.unwrap() {
        sizes.push(batch.num_rows());
        let score = batch
            .column_by_name("_score")
            .unwrap()
            .as_any()
            .downcast_ref::<Float32Array>()
            .unwrap();
        for r in 0..batch.num_rows() {
            rows.push((
                text(&batch, "row_key", r).into(),
                text(&batch, "record_id", r).into(),
                score.value(r),
            ));
        }
    }
    (rows, sizes)
}
async fn release(
    original: &Arc<SnapshotContext>,
    write: &graphit_auth::ProjectWriteAdmission<'_>,
    token: &ClaimToken,
    task: &Task,
) {
    NativeTaskStore::release_task(
        original,
        write,
        task.specification.id,
        ReleaseUpdate {
            claim: owner_input(token, task),
            summary: "privateprogressword".into(),
            next_step: "privatenextword".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
}
async fn add_comment(
    original: &Arc<SnapshotContext>,
    write: &graphit_auth::ProjectWriteAdmission<'_>,
    token: &ClaimToken,
    task: &Task,
    key: &str,
    body: &str,
) -> (Task, TaskComment) {
    let TaskCommentOutcome::Created { task, comment } = NativeTaskStore::add_task_comment(
        original,
        write,
        task.specification.id,
        TaskCommentUpdate {
            claim: owner_input(token, task),
            lease: Duration::from_secs(120),
            idempotency_key: key.into(),
            kind: TaskCommentKind::Decision,
            body: body.into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap() else {
        panic!("new comment")
    };
    (task, comment)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lexical_search_public_paths_max_sdk_score_collapse_and_filters() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let mut si = session_input("privatecreationword");
    si.description = "privatesessionword".into();
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        si,
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let mut ti = task_input(session.id, "spec");
    ti.title = "needle specword".into();
    let spec = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        ti,
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let evidence = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "evidence"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        evidence.specification.id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    let evidence = NativeTaskStore::verify_task_check(
        ctx.snapshot(),
        &write,
        evidence.specification.id,
        CheckUpdate {
            claim: owner_input(&claim.token, &claim.record),
            lease: Duration::from_secs(120),
            check_id: claim.record.specification.checks[0].id,
            passed: true,
            evidence: "needle evidenceword",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    release(ctx.snapshot(), &write, &claim.token, &evidence).await;
    let mut ci = task_input(session.id, "comment");
    ci.depends_on.push(spec.specification.id);
    ci.title = "needle blockedword".into();
    let blocked = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        ci,
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    // A blocked head can be coordinated by the broad Task write operation for the fixture.
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        blocked.specification.id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await;
    assert!(claim.is_err(), "blocked task not ready to claim");
    // Comment-only task remains ready; blocked Task is an independent current-head filter fixture.
    let comment = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "comment-only"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        comment.specification.id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    let (comment, c1) = add_comment(
        ctx.snapshot(),
        &write,
        &claim.token,
        &claim.record,
        "comment1",
        "needle commentword, literal\ntext",
    )
    .await;
    let (comment, _) = add_comment(
        ctx.snapshot(),
        &write,
        &claim.token,
        &comment,
        "comment2",
        "needle needle",
    )
    .await;
    let comment = NativeTaskStore::set_task_flag(
        ctx.snapshot(),
        &write,
        comment.specification.id,
        TaskFlagUpdate {
            claim: owner_input(&claim.token, &comment),
            lease: Duration::from_secs(120),
            flagged: true,
            reason: "privateflagword",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    release(ctx.snapshot(), &write, &claim.token, &comment).await;
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        spec.specification.id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    let (spec, _) = add_comment(
        ctx.snapshot(),
        &write,
        &claim.token,
        &claim.record,
        "spec-comment1",
        "needle",
    )
    .await;
    let (spec, _) = add_comment(
        ctx.snapshot(),
        &write,
        &claim.token,
        &spec,
        "spec-comment2",
        "needle needle needle",
    )
    .await;
    release(ctx.snapshot(), &write, &claim.token, &spec).await;
    let current = pin(&f).await;
    let (physical, sizes) = scores(&current, "needle").await;
    assert_eq!(physical.len(), 7);
    assert!(sizes.iter().sum::<usize>() > 1);
    eprintln!("Task FTS candidate batches at hint1: {sizes:?}");
    let mut max: HashMap<String, f32> = HashMap::new();
    for (_, id, score) in &physical {
        max.entry(id.clone())
            .and_modify(|old| *old = old.max(*score))
            .or_insert(*score);
    }
    let mut rl = read_limits();
    rl.batch_rows = std::num::NonZeroU32::MIN;
    let (_, total, hits, cursor) = published(
        NativeTaskStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("needle"),
            rl,
            query_max_bytes(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 4);
    assert_eq!(hits.len(), 4);
    assert!(cursor.is_none());
    for h in &hits {
        assert_eq!(
            h.score,
            max[&ulid::Ulid::from(h.task.id.get().get()).to_string()]
        );
    }
    assert!(hits.windows(2).all(|p| p[0].score >= p[1].score));
    let (_, _, comment_hits, _) =
        published(search(ctx.snapshot(), &read, query("commentword")).await);
    assert_eq!(comment_hits[0].task.id, comment.specification.id);
    assert_eq!(
        comment_hits[0].location,
        TaskLexicalMatchLocation::Comment {
            comment_id: c1.id,
            sequence: c1.sequence.get()
        }
    );
    for (term, expected) in [
        ("specword", spec.specification.id),
        ("evidenceword", evidence.specification.id),
    ] {
        let (_, total, hits, _) = published(search(ctx.snapshot(), &read, query(term)).await);
        assert_eq!(total, 1);
        assert_eq!(hits[0].task.id, expected);
        assert!(matches!(
            hits[0].location,
            TaskLexicalMatchLocation::Specification { .. }
        ));
    }
    let mut q = query("needle");
    q.top_k = NonZeroUsize::MIN;
    let (_, total, one, _) = published(search(ctx.snapshot(), &read, q.clone()).await);
    assert_eq!(total, 4);
    assert_eq!(one.len(), 1);
    assert_eq!(one[0], hits[0]);
    q.filter.flagged = Some(true);
    let (_, total, one, _) = published(search(ctx.snapshot(), &read, q).await);
    assert_eq!(total, 1);
    assert_eq!(one[0].task.id, comment.specification.id);
    let mut q = query("needle");
    q.filter.blocked = Some(true);
    let (_, total, bhits, _) = published(search(ctx.snapshot(), &read, q.clone()).await);
    assert_eq!(total, 1);
    assert_eq!(bhits[0].task.id, blocked.specification.id);
    q.filter.blocked = Some(false);
    assert_eq!(published(search(ctx.snapshot(), &read, q).await).1, 3);
    let other_session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("unmatched-session"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let mut q = query("needle");
    q.filter.session_id = Some(other_session.id);
    assert_eq!(published(search(ctx.snapshot(), &read, q).await).1, 0);
    let mut q = query("needle");
    q.filter = TaskSearchFilter {
        status: Some(TaskStatus::Open),
        session_id: Some(session.id),
        flagged: Some(true),
        blocked: Some(false),
    };
    assert_eq!(
        published(search(ctx.snapshot(), &read, q.clone()).await).1,
        1
    );
    q.filter.blocked = Some(true);
    assert_eq!(published(search(ctx.snapshot(), &read, q).await).1, 0);
    let mut q = query("needle");
    q.filter.flagged = Some(false);
    assert_eq!(published(search(ctx.snapshot(), &read, q).await).1, 3);
    for private in [
        "privatesessionword",
        "privatecreationword",
        "privateprogressword",
        "privatenextword",
        "privateflagword",
    ] {
        let (_, total, hits, _) = published(search(ctx.snapshot(), &read, query(private)).await);
        assert_eq!(total, 0);
        assert!(hits.is_empty());
    }
    let mut q = query("needle");
    q.filter.status = Some(TaskStatus::InProgress);
    assert_eq!(published(search(ctx.snapshot(), &read, q).await).1, 0);
    assert_eq!(
        pin(&f).await.version(),
        current.version() + 1,
        "only explicit extra Session write changed manifest"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lexical_search_cursor_pages_fresh_original_and_closed_binding() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("pages"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let mut expected = vec![];
    for n in 0..8 {
        let mut ti = task_input(session.id, &format!("page{n}"));
        ti.title = "equalword".into();
        let task = NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            ti,
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
        expected.push(task.specification.id);
    }
    expected.sort_unstable_by_key(|id| id.get());
    let mut q = query("equalword");
    q.page_size = NonZeroUsize::new(2).unwrap();
    q.top_k = NonZeroUsize::new(5).unwrap();
    let (first_generation, total, first, next) =
        published(search(ctx.snapshot(), &read, q.clone()).await);
    assert_eq!(total, 8);
    assert_eq!(
        first.iter().map(|h| h.task.id).collect::<Vec<_>>(),
        expected[..2]
    );
    assert_eq!(first[0].score, first[1].score);
    let cursor = next.unwrap();
    let mut continued = q.clone();
    continued.cursor = Some(cursor.clone());
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    admissions!(
        f,
        fresh,
        new_issuer,
        new_authority,
        new_grant,
        new_read,
        _new_write
    );
    assert!(matches!(
        NativeTaskStore::search_lexical(
            fresh.snapshot(),
            &read,
            continued.clone(),
            read_limits(),
            query_max_bytes()
        )
        .await,
        Err(TaskStoreError::DifferentRequest)
    ));
    let (_, total, second, next) =
        published(search(fresh.snapshot(), &new_read, continued.clone()).await);
    assert_eq!(total, 8);
    assert_eq!(
        second.iter().map(|h| h.task.id).collect::<Vec<_>>(),
        expected[2..4]
    );
    continued.cursor = next;
    let (_, _, third, next) = published(search(fresh.snapshot(), &new_read, continued).await);
    assert_eq!(third.len(), 1);
    assert_eq!(third[0].task.id, expected[4]);
    assert!(next.is_none());
    for changed in 0..8 {
        let mut changed_q = q.clone();
        changed_q.cursor = Some(cursor.clone());
        match changed {
            0 => changed_q.text = "otherword".into(),
            1 => changed_q.filter.status = Some(TaskStatus::Open),
            2 => changed_q.filter.flagged = Some(false),
            3 => changed_q.filter.blocked = Some(false),
            4 => changed_q.filter.session_id = Some(session.id),
            5 => changed_q.page_size = NonZeroUsize::new(3).unwrap(),
            6 => changed_q.top_k = NonZeroUsize::new(6).unwrap(),
            _ => changed_q.text = "equalword;".into(),
        }
        assert!(
            matches!(
                NativeTaskStore::search_lexical(
                    ctx.snapshot(),
                    &read,
                    changed_q,
                    read_limits(),
                    query_max_bytes()
                )
                .await,
                Err(TaskStoreError::InvalidCursor)
            ),
            "changed {changed}"
        );
    }
    let encoded = serde_json::to_value(&cursor).unwrap();
    assert_eq!(
        serde_json::from_value::<TaskSearchCursor>(encoded.clone()).unwrap(),
        cursor
    );
    for nested in ["generation", "filter"] {
        let mut extra = encoded.clone();
        extra["binding"][nested]["unknown"] = serde_json::json!(true);
        assert!(serde_json::from_value::<TaskSearchCursor>(extra).is_err());
    }
    let mut extra = encoded.clone();
    extra["binding"]["unknown"] = serde_json::json!(true);
    assert!(serde_json::from_value::<TaskSearchCursor>(extra).is_err());

    for offset in [0usize, 1, 5, 6, usize::MAX] {
        let mut value = encoded.clone();
        value["offset"] = serde_json::json!(offset);
        let mut forged = q.clone();
        forged.cursor = Some(serde_json::from_value(value).unwrap());
        assert!(
            matches!(
                NativeTaskStore::search_lexical(
                    ctx.snapshot(),
                    &read,
                    forged,
                    read_limits(),
                    query_max_bytes()
                )
                .await,
                Err(TaskStoreError::InvalidCursor)
            ),
            "offset {offset}"
        );
    }
    for field in ["project_id", "project_version", "generation", "root"] {
        let mut value = encoded.clone();
        match field {
            "project_id" => {
                value["binding"][field] = serde_json::json!("01KSH1CRFFG8Z74B5ZS78WW809")
            }
            "project_version" => value["binding"][field] = serde_json::json!("other-version"),
            "generation" => value["binding"][field]["version"] = serde_json::json!(1),
            _ => {
                value["binding"][field] = serde_json::to_value(
                    graphit_core::native_path::NativePath::new(f.root.join("other-root")).unwrap(),
                )
                .unwrap()
            }
        }
        let mut forged = q.clone();
        forged.cursor = Some(serde_json::from_value(value).unwrap());
        assert!(
            matches!(
                NativeTaskStore::search_lexical(
                    ctx.snapshot(),
                    &read,
                    forged,
                    read_limits(),
                    query_max_bytes()
                )
                .await,
                Err(TaskStoreError::InvalidCursor)
            ),
            "binding {field}"
        );
    }
    let mut unknown = encoded;
    unknown["authority"] = serde_json::json!("forged");
    assert!(serde_json::from_value::<TaskSearchCursor>(unknown).is_err());
    NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "stale"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    q.cursor = Some(cursor);
    let (generation, total, page, _) =
        published(search(fresh.snapshot(), &new_read, q.clone()).await);
    assert_eq!(
        generation, first_generation,
        "a new commit does not change an admitted continuation"
    );
    assert_eq!(total, 8);
    assert_eq!(
        page.iter().map(|h| h.task.id).collect::<Vec<_>>(),
        expected[2..4]
    );
    q.cursor = None;
    let (current, _, _, _) = published(search(fresh.snapshot(), &new_read, q).await);
    assert!(current.version > first_generation.version);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lexical_search_absence_empty_missing_incompatible_and_limits() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    assert_eq!(
        search(ctx.snapshot(), &read, query("needle")).await,
        TaskLexicalSearchRead::NoCorpus
    );
    assert!(!f.project.runtime_root().join("task").exists());
    for blank in ["", " ", ";needle", "needle;", "needle;;other", "needle; \t"] {
        assert!(matches!(
            NativeTaskStore::search_lexical(
                ctx.snapshot(),
                &read,
                query(blank),
                read_limits(),
                query_max_bytes()
            )
            .await,
            Err(TaskStoreError::Invalid("Task FTS blank literal"))
        ));
    }
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("empty"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let empty = search(ctx.snapshot(), &read, query("needle")).await;
    assert!(matches!(
        empty,
        TaskLexicalSearchRead::PublishedEmpty { .. }
    ));
    NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "needle"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    // The writer has already produced a real searchable index without a seed.
    assert_eq!(
        published(search(ctx.snapshot(), &read, query("needle")).await).1,
        1
    );
    super::lexical_index::remove_derived_index(&f, &f.version).await;
    assert!(matches!(
        NativeTaskStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("needle"),
            read_limits(),
            query_max_bytes()
        )
        .await,
        Err(TaskStoreError::Invalid("Task FTS index missing"))
    ));
    NativeTaskStore::create_lexical_index(ctx.snapshot(), &write, write_limits())
        .await
        .unwrap();
    let (_, total, hits, next) =
        published(search(ctx.snapshot(), &read, query("nonexistentword")).await);
    assert_eq!(total, 0);
    assert!(hits.is_empty());
    assert!(next.is_none());
    let current = pin(&f).await;
    assert_eq!(
        published(search(ctx.snapshot(), &read, query("needle")).await).1,
        1
    );
    // The literal fits, but its five-path expansion exceeds the individual query bound.
    let small = NonZeroUsize::new(6).unwrap();
    assert!(matches!(
        NativeTaskStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("needle"),
            read_limits(),
            small
        )
        .await,
        Err(TaskStoreError::Invalid("Task FTS query byte limit"))
    ));
    assert_eq!(pin(&f).await.version(), current.version());
    use lance::index::DatasetIndexExt;
    use lance_index::{IndexType, scalar::InvertedIndexParams};
    let uri = url::Url::from_file_path(f.project.runtime_root().join("task/index.lance"))
        .unwrap()
        .to_string();
    let mut dataset = lance::dataset::builder::DatasetBuilder::from_uri(&uri)
        .load()
        .await
        .unwrap();
    let params = InvertedIndexParams::default()
        .base_tokenizer("simple".into())
        .max_token_length(None)
        .lower_case(true)
        .stem(false)
        .remove_stop_words(false)
        .ascii_folding(false);
    dataset
        .create_index(
            &["public_search"],
            IndexType::Inverted,
            Some("task-public-json-content-v1".into()),
            &params,
            true,
        )
        .await
        .unwrap();
    assert_eq!(
        pin(&f)
            .await
            .json_fts_index_state("public_search")
            .await
            .unwrap(),
        JsonFtsIndexState::Incompatible
    );
    assert!(matches!(
        NativeTaskStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("needle"),
            read_limits(),
            query_max_bytes()
        )
        .await,
        Err(TaskStoreError::Invalid("Task FTS index incompatible"))
    ));
    assert_eq!(
        current.json_fts_index_state("public_search").await.unwrap(),
        JsonFtsIndexState::Ready
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lexical_search_current_revision_reset_superseded_evidence_and_authoritative_history()
{
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("revision"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let mut ti = task_input(session.id, "privateattemptword");
    ti.title = "oldtitleword".into();
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        ti,
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        task.specification.id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    let mut task = claim.record;
    for n in 0..2 {
        task = NativeTaskStore::verify_task_check(
            ctx.snapshot(),
            &write,
            task.specification.id,
            CheckUpdate {
                claim: owner_input(&claim.token, &task),
                lease: Duration::from_secs(120),
                check_id: task.specification.checks[n].id,
                passed: true,
                evidence: if n == 0 {
                    "retainedevidenceword"
                } else {
                    "resetevidenceword"
                },
            },
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
    }
    task = NativeTaskStore::supersede_task_check(
        ctx.snapshot(),
        &write,
        task.specification.id,
        SupersessionUpdate {
            claim: owner_input(&claim.token, &task),
            lease: Duration::from_secs(120),
            check_id: task.specification.checks[0].id,
            reason: "privatereasonword",
            replacement: Some(ReplacementInput {
                text: "newcheckword",
                kind: None,
            }),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let mut update = revision_input(&claim.token, &task, "privaterevisionword");
    update.title = FieldUpdate::Set("newtitleword");
    task = NativeTaskStore::revise_task(
        ctx.snapshot(),
        &write,
        task.specification.id,
        update,
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let (task, comment) = add_comment(
        ctx.snapshot(),
        &write,
        &claim.token,
        &task,
        "privatecommentattemptword",
        "literalcommentword, keep\nnewlines",
    )
    .await;
    let (_, total, hits, _) =
        published(search(ctx.snapshot(), &read, query("retainedevidenceword")).await);
    assert_eq!(total, 1);
    assert_eq!(hits[0].task.revision, task.specification.revision);
    assert_eq!(
        hits[0].location,
        TaskLexicalMatchLocation::Specification {
            revision: task.specification.revision
        }
    );
    for term in [
        "oldtitleword",
        "resetevidenceword",
        "privatereasonword",
        "privaterevisionword",
        "privatecommentattemptword",
        "privateattemptword",
    ] {
        assert_eq!(
            published(search(ctx.snapshot(), &read, query(term)).await).1,
            0,
            "{term}"
        );
    }
    for term in [
        "newtitleword",
        "newcheckword",
        "literalcommentword",
        "literalcommentword, keep\nnewlines",
    ] {
        assert_eq!(
            published(search(ctx.snapshot(), &read, query(term)).await).1,
            1,
            "{term}"
        );
    }
    let detail = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput {
            id: task.specification.id,
            history: Some(graphit_records::task_store::TaskDetailPageInput {
                page_size: std::num::NonZeroUsize::new(64).unwrap(),
                cursor: None,
            }),
            comments: Some(graphit_records::task_store::TaskDetailPageInput {
                page_size: std::num::NonZeroUsize::new(64).unwrap(),
                cursor: None,
            }),
        },
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(detail.history.as_ref().unwrap().next_cursor.is_none());
    assert!(detail.comments.as_ref().unwrap().next_cursor.is_none());
    assert_eq!(detail.creation.record.specification.title, "oldtitleword");
    assert_eq!(detail.task, task);
    assert_eq!(detail.comments.as_ref().unwrap().comments, vec![comment]);
    assert!(!detail.history.as_ref().unwrap().events.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lexical_search_inherited_read_isolation_and_branch_cursor() {
    use graphit_core::{
        project_paths::{InstallationPaths, ProjectPaths},
        version_context::{Checkout, VersionContext},
    };
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("branches"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    for n in 0..3 {
        let mut ti = task_input(session.id, &format!("branch{n}"));
        ti.title = "branchword".into();
        NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            ti,
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
    }
    let main = pin(&f).await;
    let install = InstallationPaths::resolve(
        f.installation.brand().clone(),
        &f.root,
        None,
        Some(f.installation.owner_root()),
    )
    .unwrap();
    let paths = ProjectPaths::at(&f.root, &install).unwrap();
    let version =
        VersionContext::select(&paths, Checkout::NonGit, "lexical-feature", None, "").unwrap();
    let mut scope = f.scope();
    scope.project.as_mut().unwrap().version = &version;
    let (bearer, reference, _run) = f.issue_for(&issuer, &ctx, scope);
    let feature_grant = issuer
        .verify(bearer.expose_for_transport(), &reference, scope, &ctx)
        .unwrap();
    let feature_read = authority
        .admit_read(&feature_grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let mut q = query("branchword");
    q.page_size = NonZeroUsize::MIN;
    let (generation, total, feature_results, next) =
        published(search(ctx.snapshot(), &feature_read, q.clone()).await);
    assert_eq!(total, 3);
    assert_eq!(generation.version, main.version());
    assert_eq!(
        generation.physical_branch,
        main.physical_branch().map(str::to_owned)
    );
    assert_eq!(generation.inherited_target, Some(version.lance_branch()));
    let (_, _, main_results, main_next) = published(search(ctx.snapshot(), &read, q.clone()).await);
    assert_eq!(main_results, feature_results);
    q.cursor = next;
    assert!(matches!(
        NativeTaskStore::search_lexical(
            ctx.snapshot(),
            &read,
            q.clone(),
            read_limits(),
            query_max_bytes()
        )
        .await,
        Err(TaskStoreError::InvalidCursor)
    ));
    q.cursor = main_next;
    assert!(matches!(
        NativeTaskStore::search_lexical(
            ctx.snapshot(),
            &feature_read,
            q.clone(),
            read_limits(),
            query_max_bytes()
        )
        .await,
        Err(TaskStoreError::InvalidCursor)
    ));
    let uri = url::Url::from_file_path(f.project.runtime_root().join("task/index.lance"))
        .unwrap()
        .to_string();
    let store = LanceBranchStore::open(&uri, all(&main).await[0].schema(), None)
        .await
        .unwrap();
    assert_eq!(
        store
            .capture_target(Some(&version.lance_branch()))
            .await
            .unwrap()
            .inherited_target(),
        Some(version.lance_branch().as_str())
    );
    assert_eq!(pin(&f).await.version(), main.version());
    let other = Fixture::new(&ctx, "main");
    admissions!(other, ctx, oi, oa, og, other_read, _ow);
    assert!(matches!(
        NativeTaskStore::search_lexical(
            ctx.snapshot(),
            &other_read,
            q,
            read_limits(),
            query_max_bytes()
        )
        .await,
        Err(TaskStoreError::InvalidCursor)
    ));
    assert!(!other.project.runtime_root().join("task").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lexical_search_late_typed_and_projection_corruption_invalidates_whole_read() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("corpus"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let mut last = None;
    for n in 0..12 {
        let mut ti = task_input(session.id, &format!("late{n}"));
        ti.title = "completeword".into();
        last = Some(
            NativeTaskStore::create_task(
                ctx.snapshot(),
                &write,
                ti,
                graphit_records::reference_projection::AuthoredReferencesField::Omitted,
                write_limits(),
            )
            .await
            .unwrap(),
        );
    }
    let mut rl = read_limits();
    rl.batch_rows = std::num::NonZeroU32::MIN;
    assert_eq!(
        published(
            NativeTaskStore::search_lexical(
                ctx.snapshot(),
                &read,
                query("completeword"),
                rl,
                query_max_bytes()
            )
            .await
            .unwrap()
        )
        .1,
        12
    );
    let task_id = last.unwrap().specification.id;
    let id = ulid::Ulid::from(task_id.get().get()).to_string();
    let old = pin(&f).await;
    let old_batches = all(&old).await;
    // An absent optional commit stays absent in the canonical payload. Existing
    // records need no rewrite when the optional delivery association is added.
    for batch in &old_batches {
        for row in 0..batch.num_rows() {
            if matches!(text(batch, "kind", row), "task" | "task_creation") {
                assert!(!text(batch, "payload", row).contains("\"commit\":"));
            }
        }
    }
    let current = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        TaskGetInput {
            id: task_id,
            history: None,
            comments: None,
        },
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(current.task.commit.is_none());
    assert!(
        serde_json::to_value(&current.task)
            .unwrap()
            .get("commit")
            .is_none()
    );
    assert!(
        old_batches.iter().map(RecordBatch::num_rows).sum::<usize>() > rl.batch_rows.get() as usize
    );
    replace_current_text(
        &f,
        &format!("task_creation/{id}"),
        &[("payload", "{broken")],
    )
    .await;
    // A current search is not an audit of unrelated creation/history rows.
    // Task Get still consults and rejects the corrupted creation.
    assert_eq!(
        published(search(ctx.snapshot(), &read, query("completeword")).await).1,
        12
    );
    assert!(matches!(
        NativeTaskStore::read_task(
            ctx.snapshot(),
            &read,
            TaskGetInput {
                id: task_id,
                history: None,
                comments: None
            },
            read_limits(),
        )
        .await,
        Err(TaskStoreError::Json(_))
    ));
    // Current matching payloads still fail even when they arrive after healthy hits.
    replace_current_text(&f, &format!("task/{id}"), &[("payload", "{broken")]).await;
    let corrupted = pin(&f).await.version();
    assert!(matches!(
        NativeTaskStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("completeword"),
            rl,
            query_max_bytes()
        )
        .await,
        Err(TaskStoreError::Json(_))
    ));
    assert_eq!(pin(&f).await.version(), corrupted);
    assert_eq!(all(&old).await, old_batches);
    let other = Fixture::new(&ctx, "main");
    admissions!(other, ctx, oi, oa, og, other_read, other_write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &other_write,
        session_input("forged"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &other_write,
        task_input(session.id, "completeword"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    replace_current_text(
        &other,
        &format!(
            "task/{}",
            ulid::Ulid::from(task.specification.id.get().get())
        ),
        &[("public_search", "{\"title\":\"forgedword\"}")],
    )
    .await;
    assert!(
        NativeTaskStore::search_lexical(
            ctx.snapshot(),
            &other_read,
            query("forgedword"),
            rl,
            query_max_bytes()
        )
        .await
        .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lexical_search_current_admission_revoke_deadline_cancel_and_partial_corpus() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (_, reference, _registration) = f.issue(&issuer, &ctx);
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let run = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(120))
        .unwrap();
    let permissions = [
        graphit_auth::Permission::Read(graphit_core::module_settings::Module::Task),
        graphit_auth::Permission::WriteTask,
    ];
    let bearer = issuer
        .issue(&parent, &run, &reference, &permissions, &f.scope())
        .unwrap();
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("auth"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "authorityword"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let current = pin(&f).await;
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    let fresh_grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &fresh)
        .unwrap();
    let fresh_read = authority
        .admit_read(&fresh_grant, &fresh, ProjectReadModule::Task)
        .unwrap();
    assert!(matches!(
        NativeTaskStore::search_lexical(
            fresh.snapshot(),
            &read,
            query("authorityword"),
            read_limits(),
            query_max_bytes()
        )
        .await,
        Err(TaskStoreError::DifferentRequest)
    ));
    assert_eq!(
        published(search(fresh.snapshot(), &fresh_read, query("authorityword")).await).1,
        1
    );
    let short_cancel = Cancellation::default();
    let short = request_until(&short_cancel, Instant::now() + Duration::from_secs(1));
    let short_grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &short)
        .unwrap();
    let short_read = authority
        .admit_read(&short_grant, &short, ProjectReadModule::Task)
        .unwrap();
    tokio::time::sleep_until(short.snapshot().deadline().unwrap().into()).await;
    assert!(matches!(
        NativeTaskStore::search_lexical(
            short.snapshot(),
            &short_read,
            query("authorityword"),
            read_limits(),
            query_max_bytes()
        )
        .await,
        Err(TaskStoreError::Request(_))
    ));
    fresh_cancel.cancel();
    assert!(matches!(
        NativeTaskStore::search_lexical(
            fresh.snapshot(),
            &fresh_read,
            query("authorityword"),
            read_limits(),
            query_max_bytes()
        )
        .await,
        Err(TaskStoreError::Request(_))
    ));
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(matches!(
        NativeTaskStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("authorityword"),
            read_limits(),
            query_max_bytes()
        )
        .await,
        Err(TaskStoreError::Auth(_))
    ));
    assert_eq!(pin(&f).await.version(), current.version());
    let other_cancel = Cancellation::default();
    let other_ctx = request(&other_cancel);
    let other = Fixture::new(&other_ctx, "main");
    admissions!(other, other_ctx, oi, oa, og, other_read, _ow);
    {
        let limits = write_limits();
        let artifacts = graphit_core::artifact_store::ArtifactStore::new(
            other.project.clone(),
            limits.artifacts,
        );
        let mut writer = artifacts.writer_with_context(other_ctx.snapshot()).unwrap();
        writer
            .create_record_directory(
                graphit_core::artifact_store::ProjectRecordDirectory::Task,
                limits.read.native,
            )
            .unwrap();
    }
    assert!(matches!(
        NativeTaskStore::search_lexical(
            other_ctx.snapshot(),
            &other_read,
            query("needle"),
            read_limits(),
            query_max_bytes()
        )
        .await,
        Err(TaskStoreError::IncompleteCorpus)
    ));
    assert!(
        !other
            .project
            .runtime_root()
            .join("task/index.lance")
            .exists()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lexical_search_consulted_comment_event_attribution_is_checked() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("attribution"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "attribution"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        task.specification.id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    let (_, comment) = add_comment(
        ctx.snapshot(),
        &write,
        &claim.token,
        &claim.record,
        "comment",
        "attributionword",
    )
    .await;
    assert_eq!(
        published(search(ctx.snapshot(), &read, query("attributionword")).await).1,
        1
    );
    let key = format!(
        "task_event/{}/{}",
        ulid::Ulid::from(task.specification.id.get().get()),
        comment.revision
    );
    let captured = pin(&f).await;
    let mut changed = false;
    for batch in all(&captured).await {
        for row in 0..batch.num_rows() {
            if text(&batch, "row_key", row) == key {
                let mut payload: serde_json::Value =
                    serde_json::from_str(text(&batch, "payload", row)).unwrap();
                let actor = if comment.actor == "a".repeat(64) {
                    "b".repeat(64)
                } else {
                    "a".repeat(64)
                };
                payload["record"]["actor"] = serde_json::json!(actor);
                replace_current_text(
                    &f,
                    &key,
                    &[("payload", &serde_json::to_string(&payload).unwrap())],
                )
                .await;
                changed = true;
            }
        }
    }
    assert!(changed);
    assert!(matches!(
        NativeTaskStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("attributionword"),
            read_limits(),
            query_max_bytes(),
        )
        .await,
        Err(TaskStoreError::Invalid("comment event attribution"))
    ));
}
