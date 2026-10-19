use super::*;
use arrow_array::{RecordBatch, StringArray};
use futures::TryStreamExt;
use graphit_data::lance_branch_store::{
    BranchSnapshot, DeltaResolution, JsonFtsIndexState, LanceBranchStore,
};
use graphit_records::task_spec::FieldUpdate;
use lance_index::scalar::{
    FullTextSearchQuery,
    inverted::query::{FtsQuery, MatchQuery, Operator},
};

pub(super) async fn pin(f: &Fixture) -> BranchSnapshot {
    let uri = url::Url::from_file_path(f.project.runtime_root().join("task/index.lance"))
        .unwrap()
        .to_string();
    let dataset = lance::dataset::builder::DatasetBuilder::from_uri(&uri)
        .load()
        .await
        .unwrap();
    let schema = Arc::new(arrow_schema::Schema::from(dataset.schema()));
    let store = LanceBranchStore::open(&uri, schema, None).await.unwrap();
    let branch = f.version.lance_branch();
    store
        .capture_target((!branch.is_empty()).then_some(branch.as_str()))
        .await
        .unwrap()
}
// Only explicit publisher / missing-index adversaries remove derived state.
// Normal writer and consumer journeys never call this fixture preparation.
pub(super) async fn remove_derived_index(
    fixture: &Fixture,
    version: &graphit_core::version_context::VersionContext,
) {
    use lance::index::DatasetIndexExt;
    let uri = url::Url::from_file_path(fixture.project.runtime_root().join("task/index.lance"))
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
    let branch = version.lance_branch();
    let captured = store
        .capture_target((!branch.is_empty()).then_some(branch.as_str()))
        .await
        .unwrap();
    if captured
        .json_fts_index_state("public_search")
        .await
        .unwrap()
        == JsonFtsIndexState::Missing
    {
        return;
    }
    assert!(
        captured.inherited_target().is_none(),
        "prepare an absent inherited index on its parent fixture"
    );
    let mut dataset = if branch.is_empty() {
        dataset
    } else {
        dataset.checkout_branch(&branch).await.unwrap()
    };
    dataset
        .drop_index("task-public-json-content-v1")
        .await
        .unwrap();
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
fn text<'a>(batch: &'a RecordBatch, column: &str, row: usize) -> &'a str {
    batch
        .column_by_name(column)
        .unwrap()
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap()
        .value(row)
}
async fn matches(pin: &BranchSnapshot, term: &str) -> Vec<String> {
    assert_eq!(
        pin.json_fts_index_state("public_search").await.unwrap(),
        JsonFtsIndexState::Ready
    );
    let query = [
        "title",
        "description",
        "check_text",
        "evidence",
        "comment_body",
    ]
    .map(|p| format!("{p},str,{term}"))
    .join(";");
    let matched = MatchQuery::new(query)
        .with_column(Some("public_search".into()))
        .with_operator(Operator::Or)
        .with_fuzziness(Some(0));
    let mut scan = pin.scan();
    scan.full_text_search(FullTextSearchQuery::new_query(FtsQuery::Match(matched)).limit(None))
        .unwrap();
    scan.project(&["row_key"]).unwrap();
    scan.disable_scoring_autoprojection();
    scan.batch_size(1);
    let mut stream = scan.try_into_stream().await.unwrap();
    let mut rows = Vec::new();
    while let Some(batch) = stream.try_next().await.unwrap() {
        for row in 0..batch.num_rows() {
            rows.push(text(&batch, "row_key", row).to_owned());
        }
    }
    rows.sort();
    rows
}
async fn publish(
    original: &Arc<SnapshotContext>,
    write: &graphit_auth::ProjectWriteAdmission<'_>,
    fixture: &Fixture,
) -> TaskLexicalIndexPublication {
    remove_derived_index(fixture, write.project().unwrap().version).await;
    let TaskLexicalIndexCreation::Published(publication) =
        NativeTaskStore::create_lexical_index(original, write, write_limits())
            .await
            .unwrap()
    else {
        panic!("existing corpus")
    };
    publication
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lexical_index_first_writer_is_searchable_and_ready_writes_merge_once() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("initial"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let initial = pin(&f).await;
    assert_eq!(
        initial.json_fts_index_state("public_search").await.unwrap(),
        JsonFtsIndexState::Ready
    );
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "freshneedle"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let first = pin(&f).await;
    assert_eq!(first.version(), initial.version() + 1);
    let found = NativeTaskStore::search_lexical(
        ctx.snapshot(),
        &read,
        TaskLexicalQuery {
            mode: graphit_records::task_store::TaskSearchMode::Fts,
            text: "freshneedle".into(),
            filter: TaskSearchFilter::default(),
            page_size: NonZeroUsize::new(1).unwrap(),
            top_k: NonZeroUsize::new(10).unwrap(),
            cursor: None,
        },
        read_limits(),
        NonZeroUsize::new(1048576).unwrap(),
    )
    .await
    .unwrap();
    let TaskLexicalSearchRead::Published {
        results,
        total_hits,
        ..
    } = found
    else {
        panic!("native FTS")
    };
    assert_eq!(total_hits, 1);
    assert_eq!(results[0].task.id, task.specification.id);
    NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "another"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(pin(&f).await.version(), first.version() + 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lexical_index_mutations_public_allowlist_and_retained_checks() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let mut si = session_input("privatesessionkey");
    si.description = "privatesessionkeyword".into();
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        si,
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let mut ti = task_input(session.id, "privatecreationkey");
    ti.title = "originaltitlekeyword".into();
    ti.description = "originaldescriptionkeyword".into();
    ti.checks[0].text = "retainedcheckkeyword".into();
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        ti,
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let id = task.specification.id;
    let publication = publish(ctx.snapshot(), &write, &f).await;
    assert_eq!(
        NativeTaskStore::resolve_lexical_index(
            ctx.snapshot(),
            &write,
            &publication.recovery_key,
            write_limits()
        )
        .await
        .unwrap(),
        DeltaResolution::Applied {
            version: publication.generation.version
        }
    );
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    let token = claim.token;
    let mut task = claim.record;
    let claimed = pin(&f).await;
    let mut private_fence = false;
    for batch in all(&claimed).await {
        for row in 0..batch.num_rows() {
            if text(&batch, "kind", row) == "task" {
                let payload: serde_json::Value =
                    serde_json::from_str(text(&batch, "payload", row)).unwrap();
                private_fence |= !payload["record"]["fence"].is_null();
                let projected: serde_json::Value =
                    serde_json::from_str(text(&batch, "public_search", row)).unwrap();
                assert_eq!(projected.as_object().unwrap().len(), 4);
                assert!(projected.get("fence").is_none());
                assert!(projected.get("ownership").is_none());
            }
        }
    }
    assert!(
        private_fence,
        "a real claimed head retains its private fence"
    );
    task = NativeTaskStore::verify_task_check(
        ctx.snapshot(),
        &write,
        id,
        CheckUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(120),
            check_id: task.specification.checks[0].id,
            passed: true,
            evidence: "retainedevidencekeyword",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    task = NativeTaskStore::supersede_task_check(
        ctx.snapshot(),
        &write,
        id,
        SupersessionUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(120),
            check_id: task.specification.checks[0].id,
            reason: "privatesupersessionkeyword",
            replacement: Some(ReplacementInput {
                text: "replacementcheckkeyword",
                kind: None,
            }),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let p = pin(&f).await;
    for term in [
        "originaltitlekeyword",
        "originaldescriptionkeyword",
        "retainedcheckkeyword",
        "retainedevidencekeyword",
        "replacementcheckkeyword",
    ] {
        assert_eq!(matches(&p, term).await.len(), 1, "{term}");
    }
    let mut update = revision_input(&token, &task, "privatereasonkeyword");
    update.title = FieldUpdate::Set("newtitlekeyword");
    update.description = FieldUpdate::Set("newdescriptionkeyword");
    task = NativeTaskStore::revise_task(
        ctx.snapshot(),
        &write,
        id,
        update,
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let TaskCommentOutcome::Created {
        task: commented,
        comment,
    } = NativeTaskStore::add_task_comment(
        ctx.snapshot(),
        &write,
        id,
        TaskCommentUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(120),
            idempotency_key: "privatecommentkey".into(),
            kind: TaskCommentKind::Decision,
            body: "  commentbodykeyword, literal\ntext  ".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap()
    else {
        panic!("new comment")
    };
    task = commented;
    let parent = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "link-parent"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    task = NativeTaskStore::set_task_flag(
        ctx.snapshot(),
        &write,
        id,
        TaskFlagUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(120),
            flagged: true,
            reason: "privateflagkeyword",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    task = NativeTaskStore::update_task_links(
        ctx.snapshot(),
        &write,
        id,
        TaskLinksUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(120),
            parent_id: FieldUpdate::Set(Some(parent.specification.id)),
            depends_on: Some(&[]),
            reason: "privatelinkskeyword",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    task = NativeTaskStore::heartbeat_task(
        ctx.snapshot(),
        &write,
        id,
        owner_input(&token, &task),
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    task = NativeTaskStore::release_task(
        ctx.snapshot(),
        &write,
        id,
        ReleaseUpdate {
            claim: owner_input(&token, &task),
            summary: "privateprogresskeyword".into(),
            next_step: "privatenextkeyword".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    NativeTaskStore::cancel_task(
        ctx.snapshot(),
        &write,
        id,
        TaskCancellation {
            claim: None,
            reason: "privatecancelkeyword",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let mut new = task_input(session.id, "after-index");
    new.title = "newtaskkeyword".into();
    NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        new,
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let p = pin(&f).await;
    for term in [
        "newtitlekeyword",
        "newdescriptionkeyword",
        "retainedcheckkeyword",
        "retainedevidencekeyword",
        "replacementcheckkeyword",
        "newtaskkeyword",
        "commentbodykeyword",
    ] {
        assert_eq!(matches(&p, term).await.len(), 1, "{term}");
    }
    for term in [
        "originaltitlekeyword",
        "originaldescriptionkeyword",
        "privatesessionkeyword",
        "privatereasonkeyword",
        "privateflagkeyword",
        "privatelinkskeyword",
        "privateprogresskeyword",
        "privatenextkeyword",
        "privatecancelkeyword",
        "privatecreationkey",
        "privatecommentkey",
    ] {
        assert!(matches(&p, term).await.is_empty(), "{term}");
    }
    let detail = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput {
            id,
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
    assert_eq!(detail.comments.as_ref().unwrap().comments[0], comment);
    assert_eq!(
        detail.creation.record.specification.title,
        "originaltitlekeyword"
    );
    assert!(detail.history.as_ref().unwrap().events.iter().any(|e| {
        e.record.specification.checks.iter().any(|c| {
            c.verification
                .as_ref()
                .is_some_and(|v| v.evidence == "retainedevidencekeyword")
        })
    }));
    for batch in all(&p).await {
        for row in 0..batch.num_rows() {
            let projection: serde_json::Value =
                serde_json::from_str(text(&batch, "public_search", row)).unwrap();
            match text(&batch, "kind", row) {
                "task" => {
                    assert_eq!(projection.as_object().unwrap().len(), 4);
                    assert_eq!(projection["title"], text(&batch, "title", row));
                }
                "task_comment" => {
                    assert_eq!(projection, serde_json::json!({"comment_body":comment.body}));
                }
                _ => assert_eq!(projection, serde_json::json!({})),
            }
            assert!(!text(&batch, "public_search", row).contains("fence"));
        }
    }
    let _ = task;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lexical_index_absence_authority_and_existing_name() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, _read, write);
    assert_eq!(
        NativeTaskStore::create_lexical_index(ctx.snapshot(), &write, write_limits())
            .await
            .unwrap(),
        TaskLexicalIndexCreation::NoCorpus
    );
    assert!(!f.project.runtime_root().join("task").exists());
    for operation in [
        ProjectWriteOperation::OwnTask,
        ProjectWriteOperation::CoordinateTask,
    ] {
        let admission = authority.admit_write(&grant, &ctx, operation).unwrap();
        assert!(matches!(
            NativeTaskStore::create_lexical_index(ctx.snapshot(), &admission, write_limits()).await,
            Err(TaskStoreError::WrongAdmission)
        ));
    }
    let different = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    ));
    assert!(matches!(
        NativeTaskStore::create_lexical_index(&different, &write, write_limits()).await,
        Err(TaskStoreError::DifferentRequest)
    ));
    NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("empty"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let publication = publish(ctx.snapshot(), &write, &f).await;
    assert_eq!(
        pin(&f)
            .await
            .json_fts_index_state("public_search")
            .await
            .unwrap(),
        JsonFtsIndexState::Ready
    );
    assert!(
        NativeTaskStore::create_lexical_index(ctx.snapshot(), &write, write_limits())
            .await
            .is_err()
    );
    assert_eq!(pin(&f).await.version(), publication.generation.version);
    cancel.cancel();
    assert!(
        NativeTaskStore::resolve_lexical_index(
            ctx.snapshot(),
            &write,
            &publication.recovery_key,
            write_limits()
        )
        .await
        .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lexical_index_forged_projection_before_publication() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, _read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("bounded"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "bounded"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();

    let key = format!(
        "task/{}",
        ulid::Ulid::from(task.specification.id.get().get())
    );
    replace_current_text(
        &f,
        &key,
        &[("public_search", "{\"title\":\"forgedkeyword\"}")],
    )
    .await;
    let p = pin(&f).await;
    assert!(
        NativeTaskStore::create_lexical_index(ctx.snapshot(), &write, write_limits())
            .await
            .is_err()
    );
    assert_eq!(pin(&f).await.version(), p.version());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lexical_index_completion_and_old_pin_immutability() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, _read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("completed"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let mut input = task_input(session.id, "completed");
    input.title = "completedtitlekeyword".into();
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        input,
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    publish(ctx.snapshot(), &write, &f).await;
    let old = pin(&f).await;
    let old_rows = all(&old).await;
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        task.specification.id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    let token = claim.token;
    let mut task = claim.record;
    let id = task.specification.id;
    for check in task
        .specification
        .checks
        .iter()
        .map(|c| c.id)
        .collect::<Vec<_>>()
    {
        task = NativeTaskStore::verify_task_check(
            ctx.snapshot(),
            &write,
            id,
            CheckUpdate {
                claim: owner_input(&token, &task),
                lease: Duration::from_secs(120),
                check_id: check,
                passed: true,
                evidence: "completionevidencekeyword",
            },
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
    }
    let completed = NativeTaskStore::complete_task(
        ctx.snapshot(),
        &write,
        id,
        owner_input(&token, &task),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(
        completed.status,
        graphit_records::gates::TaskStatus::Completed
    );
    let current = pin(&f).await;
    assert_eq!(matches(&current, "completedtitlekeyword").await.len(), 1);
    assert_eq!(
        matches(&current, "completionevidencekeyword").await.len(),
        1
    );
    assert!(matches(&old, "completionevidencekeyword").await.is_empty());
    assert_eq!(all(&old).await, old_rows);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lexical_index_inherited_publication_and_foreign_recovery_binding() {
    use graphit_core::{
        project_paths::{InstallationPaths, ProjectPaths},
        version_context::{Checkout, VersionContext},
    };
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, _read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("main"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "inheritedkeyword"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    // Deliberately remove only this private fixture's derived index so the
    // explicit publisher still exercises inherited-target materialization.
    remove_derived_index(&f, &f.version).await;
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
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, scope, &ctx)
        .unwrap();
    let feature_write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    let uri = url::Url::from_file_path(f.project.runtime_root().join("task/index.lance"))
        .unwrap()
        .to_string();
    let store = LanceBranchStore::open(&uri, all(&main).await[0].schema(), None)
        .await
        .unwrap();
    let branch = version.lance_branch();
    assert_eq!(
        store
            .capture_target(Some(&branch))
            .await
            .unwrap()
            .inherited_target(),
        Some(branch.as_str())
    );
    let publication = publish(ctx.snapshot(), &feature_write, &f).await;
    assert_eq!(
        publication.generation.physical_branch.as_deref(),
        Some(branch.as_str())
    );
    assert!(publication.generation.inherited_target.is_none());
    assert_eq!(pin(&f).await.version(), main.version());
    assert_eq!(
        pin(&f)
            .await
            .json_fts_index_state("public_search")
            .await
            .unwrap(),
        JsonFtsIndexState::Missing
    );
    assert_eq!(
        NativeTaskStore::resolve_lexical_index(
            ctx.snapshot(),
            &feature_write,
            &publication.recovery_key,
            write_limits()
        )
        .await
        .unwrap(),
        DeltaResolution::Applied {
            version: publication.generation.version
        }
    );
    assert!(matches!(
        NativeTaskStore::resolve_lexical_index(
            ctx.snapshot(),
            &write,
            &publication.recovery_key,
            write_limits()
        )
        .await,
        Err(TaskStoreError::Branch(_))
    ));
    let other = Fixture::new(&ctx, "main");
    admissions!(
        other,
        ctx,
        other_issuer,
        other_authority,
        other_grant,
        _other_read,
        other_write
    );
    assert!(matches!(
        NativeTaskStore::resolve_lexical_index(
            ctx.snapshot(),
            &other_write,
            &publication.recovery_key,
            write_limits()
        )
        .await,
        Err(TaskStoreError::Branch(_))
    ));
    assert!(!other.project.runtime_root().join("task").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lexical_index_missing_history_late_payload_and_old_schema_rejected() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, _read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("corruption"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let mut last = None;
    for n in 0..10 {
        last = Some(
            NativeTaskStore::create_task(
                ctx.snapshot(),
                &write,
                task_input(session.id, &format!("late-{n}")),
                graphit_records::reference_projection::AuthoredReferencesField::Omitted,
                write_limits(),
            )
            .await
            .unwrap(),
        );
    }
    let task = last.unwrap();
    let task_id = ulid::Ulid::from(task.specification.id.get().get()).to_string();
    replace_current_text(
        &f,
        &format!("task_creation/{task_id}"),
        &[("payload", "{broken")],
    )
    .await;
    let current = pin(&f).await;
    let mut limits = write_limits();
    limits.read.batch_rows = std::num::NonZeroU32::new(1).unwrap();
    assert!(matches!(
        NativeTaskStore::create_lexical_index(ctx.snapshot(), &write, limits).await,
        Err(TaskStoreError::Json(_))
    ));
    assert_eq!(pin(&f).await.version(), current.version());
    let other = Fixture::new(&ctx, "main");
    admissions!(
        other,
        ctx,
        other_issuer,
        other_authority,
        other_grant,
        _other_read,
        other_write
    );
    let schema = all(&current).await[0].schema();
    let old_schema = Arc::new(arrow_schema::Schema::new(
        schema
            .fields()
            .iter()
            .filter(|f| f.name() != "public_search")
            .cloned()
            .collect::<Vec<_>>(),
    ));
    let uri = other.project.runtime_root().join("task/index.lance");
    // The SDK fixture must retain the real Artifact directory permissions.
    // A directly created permissive Task parent would fail before schema admission.
    {
        let limits = write_limits();
        let artifacts = graphit_core::artifact_store::ArtifactStore::new(
            other.project.clone(),
            limits.artifacts,
        );
        let mut writer = artifacts.writer_with_context(ctx.snapshot()).unwrap();
        writer
            .create_record_directory(
                graphit_core::artifact_store::ProjectRecordDirectory::Task,
                limits.read.native,
            )
            .unwrap();
    }
    graphit_data::lance_module_store::LanceModuleStore::create(
        uri.to_str().unwrap(),
        Arc::clone(&old_schema),
        vec![RecordBatch::new_empty(old_schema)],
    )
    .await
    .unwrap();
    let outcome =
        NativeTaskStore::create_lexical_index(ctx.snapshot(), &other_write, write_limits()).await;
    assert!(
        matches!(
            &outcome,
            Err(TaskStoreError::Branch(
                graphit_data::lance_branch_store::BranchError::SchemaMismatch
            ))
        ),
        "{outcome:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lexical_index_fresh_original_recovery_revoke_and_deadline() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (_, reference, _registration_run) = f.issue(&issuer, &ctx);
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
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("authority"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let publication = publish(ctx.snapshot(), &write, &f).await;
    let before = pin(&f).await.version();
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    let fresh_grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &fresh)
        .unwrap();
    let fresh_write = authority
        .admit_write(&fresh_grant, &fresh, ProjectWriteOperation::Task)
        .unwrap();
    assert_eq!(
        NativeTaskStore::resolve_lexical_index(
            fresh.snapshot(),
            &fresh_write,
            &publication.recovery_key,
            write_limits()
        )
        .await
        .unwrap(),
        DeltaResolution::Applied { version: before }
    );
    let short_cancel = Cancellation::default();
    let short = request_until(&short_cancel, Instant::now() + Duration::from_secs(1));
    let short_grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &short)
        .unwrap();
    let short_write = authority
        .admit_write(&short_grant, &short, ProjectWriteOperation::Task)
        .unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(matches!(
        NativeTaskStore::create_lexical_index(short.snapshot(), &short_write, write_limits()).await,
        Err(TaskStoreError::Request(_))
    ));
    assert!(matches!(
        NativeTaskStore::resolve_lexical_index(
            short.snapshot(),
            &short_write,
            &publication.recovery_key,
            write_limits()
        )
        .await,
        Err(TaskStoreError::Request(_))
    ));
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(matches!(
        NativeTaskStore::create_lexical_index(ctx.snapshot(), &write, write_limits()).await,
        Err(TaskStoreError::Auth(_))
    ));
    assert!(matches!(
        NativeTaskStore::resolve_lexical_index(
            fresh.snapshot(),
            &fresh_write,
            &publication.recovery_key,
            write_limits()
        )
        .await,
        Err(TaskStoreError::Auth(_))
    ));
    assert_eq!(pin(&f).await.version(), before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lexical_index_history_and_comment_attribution_validated_before_sdk() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, _read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("history"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "history"),
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
    let TaskCommentOutcome::Created { comment, .. } = NativeTaskStore::add_task_comment(
        ctx.snapshot(),
        &write,
        task.specification.id,
        TaskCommentUpdate {
            claim: owner_input(&claim.token, &claim.record),
            lease: Duration::from_secs(120),
            idempotency_key: "one".into(),
            kind: TaskCommentKind::Note,
            body: "historycommentkeyword".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap() else {
        panic!("new")
    };
    let current = pin(&f).await;
    let mut patches = Vec::new();
    for batch in all(&current).await {
        for row in 0..batch.num_rows() {
            if text(&batch, "kind", row) == "task_comment" {
                let mut changed = batch.slice(row, 1);
                let mut payload: serde_json::Value =
                    serde_json::from_str(text(&changed, "payload", 0)).unwrap();
                let other_actor = if comment.actor == "a".repeat(64) {
                    "b".repeat(64)
                } else {
                    "a".repeat(64)
                };
                payload["record"]["record"]["actor"] = serde_json::json!(other_actor);
                let mut columns = changed.columns().to_vec();
                columns[changed.schema().index_of("payload").unwrap()] =
                    Arc::new(StringArray::from(vec![
                        serde_json::to_string(&payload).unwrap(),
                    ]));
                changed = RecordBatch::try_new(changed.schema(), columns).unwrap();
                patches.push(changed);
            }
        }
    }
    current
        .prepare_merge(patches, vec!["row_key".into()])
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    let corrupt = pin(&f).await;
    assert!(
        NativeTaskStore::create_lexical_index(ctx.snapshot(), &write, write_limits())
            .await
            .is_err()
    );
    assert_eq!(pin(&f).await.version(), corrupt.version());
    // Independent corpus: canonical remaining rows cannot mask a missing creation.
    let other = Fixture::new(&ctx, "main");
    admissions!(
        other,
        ctx,
        other_issuer,
        other_authority,
        other_grant,
        _other_read,
        other_write
    );
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &other_write,
        session_input("missing"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    NativeTaskStore::create_task(
        ctx.snapshot(),
        &other_write,
        task_input(session.id, "missing"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let path = other.project.runtime_root().join("task/index.lance");
    let mut dataset = lance::Dataset::open(path.to_str().unwrap()).await.unwrap();
    dataset.delete("kind = 'task_creation'").await.unwrap();
    let deleted = pin(&other).await;
    assert!(matches!(
        NativeTaskStore::create_lexical_index(ctx.snapshot(), &other_write, write_limits()).await,
        Err(TaskStoreError::IncompleteCorpus)
    ));
    assert_eq!(pin(&other).await.version(), deleted.version());
}
