use super::*;
use graphit_records::reference_projection::AuthoredReferencesField;
use std::num::NonZeroU32;

fn identity(value: u8) -> TaskEmbeddingIdentity {
    TaskEmbeddingIdentity::new(
        [value; 32],
        NonZeroU32::new(2).unwrap(),
        TASK_DOCUMENT_RECIPE.into(),
    )
    .unwrap()
}
fn search_input(mode: TaskSearchMode) -> TaskLexicalQuery {
    TaskLexicalQuery {
        mode,
        text: "Actual work".into(),
        filter: TaskSearchFilter::default(),
        page_size: NonZeroUsize::MIN,
        top_k: NonZeroUsize::new(10).unwrap(),
        cursor: None,
    }
}
async fn pending(
    ctx: &graphit_hub::policy::RequestContext<'_>,
    read: &graphit_auth::ProjectReadAdmission<'_>,
    identity: &TaskEmbeddingIdentity,
) -> Vec<TaskEmbeddingDocument> {
    let TaskEmbeddingCorpusRead::Published(mut corpus) =
        NativeTaskStore::capture_embeddings(ctx.snapshot(), read, identity.clone(), read_limits())
            .await
            .unwrap()
    else {
        panic!("published corpus")
    };
    let mut documents = Vec::new();
    while let Some(batch) = corpus
        .next_documents(ctx.snapshot(), read, NonZeroUsize::new(10).unwrap())
        .await
        .unwrap()
    {
        documents.extend(batch);
    }
    documents
}
async fn attach(
    ctx: &graphit_hub::policy::RequestContext<'_>,
    write: &graphit_auth::ProjectWriteAdmission<'_>,
    document: TaskEmbeddingDocument,
    vector: Vec<f32>,
) -> TaskEmbeddingPublication {
    NativeTaskStore::attach_embeddings(
        ctx.snapshot(),
        write,
        vec![TaskEmbeddingAttachment { document, vector }],
        write_limits(),
    )
    .await
    .unwrap()
}
fn hits(read: TaskLexicalSearchRead) -> (Vec<TaskLexicalHit>, Option<TaskSearchCursor>) {
    let TaskLexicalSearchRead::Published {
        results,
        next_cursor,
        ..
    } = read
    else {
        panic!("published")
    };
    (results, next_cursor)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_embeddings_progressive_reuse_freshness_hybrid_and_paging() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("embedding-session"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let mut tasks = vec![];
    for key in ["embedding-a", "embedding-b"] {
        tasks.push(
            NativeTaskStore::create_task(
                ctx.snapshot(),
                &write,
                task_input(session.id, key),
                AuthoredReferencesField::Omitted,
                write_limits(),
            )
            .await
            .unwrap(),
        );
    }
    let model = identity(7);
    let mut documents = pending(&ctx, &read, &model).await;
    assert_eq!(documents.len(), 2);
    let a = documents.remove(0);
    let b = documents.remove(0);
    let first_id = a.task_id();
    let second_id = b.task_id();
    let query = search_input(TaskSearchMode::Semantic);
    let result = NativeTaskStore::search_semantic(
        ctx.snapshot(),
        &read,
        model.clone(),
        &[1., 0.],
        query.clone(),
        read_limits(),
        NonZeroUsize::new(4096).unwrap(),
    )
    .await
    .unwrap();
    assert!(hits(result).0.is_empty());
    let publication = attach(&ctx, &write, a.clone(), vec![0.6, 0.8]).await;
    assert_eq!(publication.attached, 1);
    // One ready vector is immediately searchable; the other task is still pending.
    let result = NativeTaskStore::search_semantic(
        ctx.snapshot(),
        &read,
        model.clone(),
        &[1., 0.],
        query.clone(),
        read_limits(),
        NonZeroUsize::new(4096).unwrap(),
    )
    .await
    .unwrap();
    let results = hits(result).0;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].task.id, first_id);
    assert_eq!(pending(&ctx, &read, &model).await.len(), 1);
    let retry = attach(&ctx, &write, a.clone(), vec![0.6, 0.8]).await;
    assert_eq!(retry.reused, 1);
    assert_eq!(
        retry.generation, publication.generation,
        "no redundant commit for already covered text"
    );
    attach(&ctx, &write, b, vec![1., 0.]).await;
    assert!(pending(&ctx, &read, &model).await.is_empty());
    let result = NativeTaskStore::search_semantic(
        ctx.snapshot(),
        &read,
        model.clone(),
        &[1., 0.],
        query.clone(),
        read_limits(),
        NonZeroUsize::new(4096).unwrap(),
    )
    .await
    .unwrap();
    let (results, cursor) = hits(result);
    assert_eq!(results[0].task.id, second_id);
    let mut second_page = query.clone();
    second_page.cursor = cursor;
    assert_eq!(
        second_page
            .cursor
            .as_ref()
            .unwrap()
            .query_vector(&model)
            .unwrap(),
        Some([1., 0.].as_slice())
    );
    for changed in ["text", "filter"] {
        let mut altered = second_page.clone();
        if changed == "text" {
            altered.text = "different query".into();
        } else {
            altered.filter.flagged = Some(true);
        }
        assert!(matches!(
            NativeTaskStore::search_semantic(
                ctx.snapshot(),
                &read,
                model.clone(),
                &[0., 1.],
                altered,
                read_limits(),
                NonZeroUsize::new(4096).unwrap()
            )
            .await,
            Err(TaskStoreError::InvalidCursor)
        ));
    }
    let (page, _) = hits(
        NativeTaskStore::search_semantic(
            ctx.snapshot(),
            &read,
            model.clone(),
            &[0., 1.],
            second_page.clone(),
            read_limits(),
            NonZeroUsize::new(4096).unwrap(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(page[0].task.id, first_id);
    assert!(matches!(
        NativeTaskStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(8),
            &[1., 0.],
            second_page,
            read_limits(),
            NonZeroUsize::new(4096).unwrap()
        )
        .await,
        Err(TaskStoreError::InvalidCursor)
    ));
    // A different model cannot reuse these vectors or accidentally score them.
    assert_eq!(pending(&ctx, &read, &identity(8)).await.len(), 2);
    let incompatible = NativeTaskStore::search_semantic(
        ctx.snapshot(),
        &read,
        identity(8),
        &[1., 0.],
        query.clone(),
        read_limits(),
        NonZeroUsize::new(4096).unwrap(),
    )
    .await
    .unwrap();
    assert!(hits(incompatible).0.is_empty());
    let different_width = TaskEmbeddingIdentity::new(
        [9; 32],
        NonZeroU32::new(3).unwrap(),
        TASK_DOCUMENT_RECIPE.into(),
    )
    .unwrap();
    let incompatible_width = NativeTaskStore::search_semantic(
        ctx.snapshot(),
        &read,
        different_width,
        &[1., 0., 0.],
        query.clone(),
        read_limits(),
        NonZeroUsize::new(4096).unwrap(),
    )
    .await
    .unwrap();
    assert!(
        hits(incompatible_width).0.is_empty(),
        "old vector widths are excluded before distance calculation"
    );
    let hybrid = NativeTaskStore::search_semantic(
        ctx.snapshot(),
        &read,
        model.clone(),
        &[1., 0.],
        search_input(TaskSearchMode::Hybrid),
        read_limits(),
        NonZeroUsize::new(4096).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(hits(hybrid).0.len(), 1);
    // Claims and metadata revisions preserve the content fingerprint and its vector.
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        first_id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    assert!(pending(&ctx, &read, &model).await.is_empty());
    let mut update = revision_input(&claim.token, &claim.record, "change public content");
    update.description = graphit_records::task_spec::FieldUpdate::Set("Changed source");
    NativeTaskStore::revise_task(
        ctx.snapshot(),
        &write,
        first_id,
        update,
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let next = pending(&ctx, &read, &model).await;
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].task_id(), first_id);
    assert_ne!(next[0].fingerprint(), a.fingerprint());
    let stale = attach(&ctx, &write, a, vec![1., 0.]).await;
    assert_eq!(stale.stale, 1);
    assert_eq!(stale.attached, 0);
    // The old vector is not used for the edited task, even before re-embedding.
    let results = hits(
        NativeTaskStore::search_semantic(
            ctx.snapshot(),
            &read,
            model.clone(),
            &[1., 0.],
            query.clone(),
            read_limits(),
            NonZeroUsize::new(4096).unwrap(),
        )
        .await
        .unwrap(),
    )
    .0;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].task.id, second_id);
    let mut filtered = query;
    filtered.filter.flagged = Some(true);
    assert!(
        hits(
            NativeTaskStore::search_semantic(
                ctx.snapshot(),
                &read,
                model,
                &[1., 0.],
                filtered,
                read_limits(),
                NonZeroUsize::new(4096).unwrap()
            )
            .await
            .unwrap()
        )
        .0
        .is_empty()
    );
    // Derived rows do not corrupt ordinary reads, FTS, histories or catalogues.
    let view = NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap();
    assert!(matches!(view, TaskReadView::Published { .. }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_embeddings_typed_storage_comments_branch_and_cancel() {
    use futures::TryStreamExt;
    use graphit_data::lance_branch_store::BranchSnapshot;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "feature");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("semantic-feature"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "semantic-feature-task"),
        AuthoredReferencesField::Omitted,
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
    let outcome = NativeTaskStore::add_task_comment(
        ctx.snapshot(),
        &write,
        task.specification.id,
        TaskCommentUpdate {
            claim: owner_input(&claim.token, &claim.record),
            lease: Duration::from_secs(120),
            idempotency_key: "semantic-comment".into(),
            kind: TaskCommentKind::Note,
            body: "Useful explanation".into(),
        },
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let TaskCommentOutcome::Created { comment, .. } = outcome else {
        panic!("new comment")
    };
    let model = identity(4);
    let docs = pending(&ctx, &read, &model).await;
    assert_eq!(docs.len(), 2);
    let comment_doc = docs
        .into_iter()
        .find(|d| matches!(d.location(), TaskLexicalMatchLocation::Comment { .. }))
        .unwrap();
    // Moving inference text out does not remove the association needed by attach.
    let mut comment_doc = comment_doc;
    let inference = comment_doc.take_text();
    assert!(inference.contains("Useful explanation"));
    assert!(comment_doc.text().is_empty());
    attach(&ctx, &write, comment_doc, vec![1., 0.]).await;
    let results = hits(
        NativeTaskStore::search_semantic(
            ctx.snapshot(),
            &read,
            model.clone(),
            &[1., 0.],
            search_input(TaskSearchMode::Semantic),
            read_limits(),
            NonZeroUsize::new(4096).unwrap(),
        )
        .await
        .unwrap(),
    )
    .0;
    assert_eq!(results.len(), 1);
    assert!(
        matches!(results[0].location,TaskLexicalMatchLocation::Comment { comment_id, .. } if comment_id == comment.id)
    );
    let uri = url::Url::from_file_path(f.project.runtime_root().join("task/index.lance"))
        .unwrap()
        .to_string();
    let branch = f.version.lance_branch();
    let pin = BranchSnapshot::capture_observed(&uri, Some(&branch), None)
        .await
        .unwrap();
    assert!(matches!(
        pin.schema()
            .field_with_name("embedding_vector")
            .unwrap()
            .data_type(),
        arrow_schema::DataType::List(_)
    ));
    let mut scan = pin.scan();
    scan.filter("kind = 'embedding'")
        .unwrap()
        .project(&["payload", "embedding_vector"])
        .unwrap();
    let batches = scan
        .try_into_stream()
        .await
        .unwrap()
        .try_collect::<Vec<_>>()
        .await
        .unwrap();
    assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 1);
    let payload = batches[0]
        .column_by_name("payload")
        .unwrap()
        .as_any()
        .downcast_ref::<arrow_array::StringArray>()
        .unwrap()
        .value(0);
    let encoded: serde_json::Value = serde_json::from_str(payload).unwrap();
    assert!(
        encoded["record"].get("vector").is_none(),
        "vector is typed Arrow, not duplicated JSON"
    );
    assert!(
        encoded["record"]["document"].get("text").is_none(),
        "public source body is not duplicated"
    );
    // This named branch evolved independently; default still contains no records.
    let default = BranchSnapshot::capture_observed(&uri, None, None)
        .await
        .unwrap();
    assert!(
        default
            .schema()
            .field_with_name("embedding_vector")
            .is_err()
    );
    let default_batches = default
        .scan()
        .try_into_stream()
        .await
        .unwrap()
        .try_collect::<Vec<_>>()
        .await
        .unwrap();
    assert_eq!(
        default_batches.iter().map(|b| b.num_rows()).sum::<usize>(),
        0
    );
    let refs = NativeTaskStore::references(
        ctx.snapshot(),
        &read,
        TaskReferencesQuery {
            focus: TaskReferenceRecord::Task(task.specification.id),
            page_size: NonZeroUsize::new(10).unwrap(),
            outgoing_cursor: None,
            incoming_cursor: None,
        },
        read_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(refs, TaskReferencesRead::Published { .. }));
    let pending = pending(&ctx, &read, &model).await;
    assert_eq!(
        pending.len(),
        1,
        "only the unembedded specification remains"
    );
    assert!(
        NativeTaskStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            vec![TaskEmbeddingAttachment {
                document: pending[0].clone(),
                vector: vec![f32::NAN, 0.]
            }],
            write_limits()
        )
        .await
        .is_err()
    );
    cancel.cancel();
    assert!(
        NativeTaskStore::capture_embeddings(ctx.snapshot(), &read, model.clone(), read_limits())
            .await
            .is_err()
    );
    assert!(
        NativeTaskStore::search_semantic(
            ctx.snapshot(),
            &read,
            model,
            &[1., 0.],
            search_input(TaskSearchMode::Semantic),
            read_limits(),
            NonZeroUsize::new(4096).unwrap()
        )
        .await
        .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_embeddings_one_captured_scan_batches_skip_covered_and_leave_stale_for_next_hint() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("stream-session"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    for key in ["stream-a", "stream-b", "stream-c"] {
        NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            task_input(session.id, key),
            AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
    }
    let model = identity(51);
    let covered = pending(&ctx, &read, &model).await.remove(0);
    let covered_id = covered.task_id();
    attach(&ctx, &write, covered, vec![1., 0.]).await;
    let TaskEmbeddingCorpusRead::Published(mut corpus) =
        NativeTaskStore::capture_embeddings(ctx.snapshot(), &read, model.clone(), read_limits())
            .await
            .unwrap()
    else {
        panic!("published")
    };
    let captured_generation = corpus.generation().clone();
    let mut first = corpus
        .next_documents(ctx.snapshot(), &read, NonZeroUsize::MIN)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.len(), 1);
    let stale = first.remove(0);
    assert_ne!(
        stale.task_id(),
        covered_id,
        "covered source is not emitted for inference"
    );
    let stale_id = stale.task_id();
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        stale_id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    let mut update = revision_input(
        &claim.token,
        &claim.record,
        "edit while captured inference runs",
    );
    update.description = graphit_records::task_spec::FieldUpdate::Set("Changed during inference");
    NativeTaskStore::revise_task(
        ctx.snapshot(),
        &write,
        stale_id,
        update,
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let created_after_capture = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "stream-later"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let publication = attach(&ctx, &write, stale, vec![1., 0.]).await;
    assert_eq!(publication.stale, 1);
    let mut second = corpus
        .next_documents(ctx.snapshot(), &read, NonZeroUsize::MIN)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second.len(), 1);
    let second = second.remove(0);
    assert_ne!(
        second.task_id(),
        stale_id,
        "stale source is not re-emitted in a captured pass"
    );
    assert_ne!(second.task_id(), covered_id);
    assert_ne!(
        second.task_id(),
        created_after_capture.specification.id,
        "this pass retains its original snapshot"
    );
    attach(&ctx, &write, second, vec![0., 1.]).await;
    assert!(
        corpus
            .next_documents(ctx.snapshot(), &read, NonZeroUsize::MIN)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        corpus
            .next_documents(ctx.snapshot(), &read, NonZeroUsize::MIN)
            .await
            .unwrap()
            .is_none(),
        "EOF remains EOF, without a recapture loop"
    );
    assert_eq!(corpus.generation(), &captured_generation);
    let remaining = pending(&ctx, &read, &model).await;
    let ids = remaining
        .iter()
        .map(|d| d.task_id())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&stale_id));
    assert!(ids.contains(&created_after_capture.specification.id));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_embeddings_inherited_stream_continues_after_own_branch_materialization() {
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
        session_input("inherited-embedding-session"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    for key in ["inherited-first", "inherited-second"] {
        NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            task_input(session.id, key),
            AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
    }
    let installation = InstallationPaths::resolve(
        f.installation.brand().clone(),
        &f.root,
        None,
        Some(f.installation.owner_root()),
    )
    .unwrap();
    let paths = ProjectPaths::at(&f.root, &installation).unwrap();
    let version =
        VersionContext::select(&paths, Checkout::NonGit, "embedding-inherited", None, "").unwrap();
    let mut scope = f.scope();
    scope.project.as_mut().unwrap().version = &version;
    let (bearer, reference, _run) = f.issue_for(&issuer, &ctx, scope);
    let feature_grant = issuer
        .verify(bearer.expose_for_transport(), &reference, scope, &ctx)
        .unwrap();
    let feature_read = authority
        .admit_read(&feature_grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let feature_write = authority
        .admit_write(&feature_grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    let model = identity(71);
    let TaskEmbeddingCorpusRead::Published(mut corpus) = NativeTaskStore::capture_embeddings(
        ctx.snapshot(),
        &feature_read,
        model.clone(),
        read_limits(),
    )
    .await
    .unwrap() else {
        panic!("published")
    };
    let captured = corpus.generation().clone();
    assert!(captured.physical_branch.is_none());
    assert_eq!(captured.inherited_target, Some(version.lance_branch()));
    let mut first = corpus
        .next_documents(ctx.snapshot(), &feature_read, NonZeroUsize::MIN)
        .await
        .unwrap()
        .unwrap();
    let first = first.remove(0);
    let first_id = first.task_id();
    let publication = attach(&ctx, &feature_write, first, vec![1., 0.]).await;
    assert_eq!(
        publication.generation.physical_branch,
        Some(version.lance_branch())
    );
    assert!(publication.generation.inherited_target.is_none());
    // The already-open default snapshot remains authoritative for this pass,
    // although this same worker just materialized its selected named branch.
    let mut second = corpus
        .next_documents(ctx.snapshot(), &feature_read, NonZeroUsize::MIN)
        .await
        .unwrap()
        .unwrap();
    let second = second.remove(0);
    assert_ne!(second.task_id(), first_id);
    attach(&ctx, &feature_write, second, vec![0., 1.]).await;
    assert!(
        corpus
            .next_documents(ctx.snapshot(), &feature_read, NonZeroUsize::MIN)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(corpus.generation(), &captured);
    let mut query = search_input(TaskSearchMode::Semantic);
    query.page_size = NonZeroUsize::new(10).unwrap();
    let feature = hits(
        NativeTaskStore::search_semantic(
            ctx.snapshot(),
            &feature_read,
            model.clone(),
            &[1., 0.],
            query.clone(),
            read_limits(),
            NonZeroUsize::new(4096).unwrap(),
        )
        .await
        .unwrap(),
    )
    .0;
    assert_eq!(feature.len(), 2);
    assert!(pending(&ctx, &feature_read, &model).await.is_empty());
    let main = hits(
        NativeTaskStore::search_semantic(
            ctx.snapshot(),
            &read,
            model.clone(),
            &[1., 0.],
            query,
            read_limits(),
            NonZeroUsize::new(4096).unwrap(),
        )
        .await
        .unwrap(),
    )
    .0;
    assert!(
        main.is_empty(),
        "derived vectors stayed in the selected named branch"
    );
    assert_eq!(
        pending(&ctx, &read, &model).await.len(),
        2,
        "default records remain unchanged"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_hybrid_fuses_all_candidates_and_counts_unique_tasks_before_top_k() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("hybrid-complete"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    for n in 0..6 {
        let mut input = task_input(session.id, &format!("hybrid-{n}"));
        input.description = "needle complete public projection".into();
        NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            input,
            AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
    }
    let maximum = NonZeroUsize::new(4096).unwrap();
    let mut query = search_input(TaskSearchMode::Fts);
    query.text = "needle".into();
    query.top_k = NonZeroUsize::new(6).unwrap();
    query.page_size = NonZeroUsize::new(6).unwrap();
    let all = hits(
        NativeTaskStore::search_lexical(
            ctx.snapshot(),
            &read,
            query.clone(),
            read_limits(),
            maximum,
        )
        .await
        .unwrap(),
    )
    .0;
    assert_eq!(all.len(), 6);
    let first_semantic = all[4].task.id;
    let second_semantic = all[5].task.id;
    let model = identity(81);
    for document in pending(&ctx, &read, &model).await {
        if document.task_id() == first_semantic {
            attach(&ctx, &write, document, vec![1., 0.]).await;
        } else if document.task_id() == second_semantic {
            attach(&ctx, &write, document, vec![0.6, 0.8]).await;
        }
    }
    query.top_k = NonZeroUsize::new(2).unwrap();
    query.page_size = NonZeroUsize::MIN;
    let lexical = NativeTaskStore::search_lexical(
        ctx.snapshot(),
        &read,
        query.clone(),
        read_limits(),
        maximum,
    )
    .await
    .unwrap();
    assert!(matches!(
        lexical,
        TaskLexicalSearchRead::Published { total_hits: 6, .. }
    ));
    query.mode = TaskSearchMode::Hybrid;
    let hybrid = NativeTaskStore::search_semantic(
        ctx.snapshot(),
        &read,
        model.clone(),
        &[1., 0.],
        query.clone(),
        read_limits(),
        maximum,
    )
    .await
    .unwrap();
    let TaskLexicalSearchRead::Published {
        total_hits,
        results,
        next_cursor,
        ..
    } = hybrid
    else {
        panic!("published")
    };
    assert_eq!(
        total_hits, 6,
        "uncapped union retains all four lexical-only Tasks"
    );
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].task.id, first_semantic,
        "lexical rank below top_k still contributes to fusion"
    );
    query.cursor = next_cursor;
    let continuation = NativeTaskStore::search_semantic(
        ctx.snapshot(),
        &read,
        model,
        &[0., 1.],
        query.clone(),
        read_limits(),
        maximum,
    )
    .await
    .unwrap();
    let TaskLexicalSearchRead::Published {
        total_hits,
        results,
        next_cursor,
        ..
    } = continuation
    else {
        panic!("published")
    };
    assert_eq!(total_hits, 6);
    assert_eq!(results[0].task.id, second_semantic);
    assert!(next_cursor.is_none());
    query.cursor = None;
    query.page_size = NonZeroUsize::new(2).unwrap();
    let no_vectors = NativeTaskStore::search_semantic(
        ctx.snapshot(),
        &read,
        identity(82),
        &[1., 0.],
        query,
        read_limits(),
        maximum,
    )
    .await
    .unwrap();
    let TaskLexicalSearchRead::Published {
        total_hits,
        results,
        ..
    } = no_vectors
    else {
        panic!("published")
    };
    assert_eq!(
        total_hits, 6,
        "hybrid keeps the full lexical result even with no ready vectors"
    );
    assert_eq!(
        results.iter().map(|h| h.task.id).collect::<Vec<_>>(),
        all[..2].iter().map(|h| h.task.id).collect::<Vec<_>>()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_search_pages_keep_snapshot_with_new_requests_and_background_vector_commits() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("continuation"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    for key in ["first", "second", "pending"] {
        NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            task_input(session.id, key),
            AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
    }
    let model = identity(92);
    let mut documents = pending(&ctx, &read, &model).await;
    let last = documents.pop().unwrap();
    let pending_id = last.task_id();
    for (document, vector) in documents.into_iter().zip([vec![0., 1.], vec![0.6, 0.8]]) {
        attach(&ctx, &write, document, vector).await;
    }
    let mut captures = Vec::new();
    for mode in [
        TaskSearchMode::Fts,
        TaskSearchMode::Semantic,
        TaskSearchMode::Hybrid,
    ] {
        let mut query = search_input(mode);
        query.top_k = NonZeroUsize::new(2).unwrap();
        let result = if mode == TaskSearchMode::Fts {
            NativeTaskStore::search_lexical(
                ctx.snapshot(),
                &read,
                query.clone(),
                read_limits(),
                NonZeroUsize::new(4096).unwrap(),
            )
            .await
            .unwrap()
        } else {
            NativeTaskStore::search_semantic(
                ctx.snapshot(),
                &read,
                model.clone(),
                &[1., 0.],
                query.clone(),
                read_limits(),
                NonZeroUsize::new(4096).unwrap(),
            )
            .await
            .unwrap()
        };
        let TaskLexicalSearchRead::Published {
            generation,
            total_hits,
            next_cursor,
            ..
        } = result
        else {
            panic!("published")
        };
        let mut complete = query.clone();
        complete.page_size = NonZeroUsize::new(2).unwrap();
        let all = if mode == TaskSearchMode::Fts {
            NativeTaskStore::search_lexical(
                ctx.snapshot(),
                &read,
                complete,
                read_limits(),
                NonZeroUsize::new(4096).unwrap(),
            )
            .await
            .unwrap()
        } else {
            NativeTaskStore::search_semantic(
                ctx.snapshot(),
                &read,
                model.clone(),
                &[1., 0.],
                complete,
                read_limits(),
                NonZeroUsize::new(4096).unwrap(),
            )
            .await
            .unwrap()
        };
        let expected = hits(all).0.remove(1);
        query.cursor = next_cursor;
        assert!(query.cursor.is_some());
        captures.push((query, generation, total_hits, expected));
    }
    let publication = attach(&ctx, &write, last, vec![1., 0.]).await;
    for (query, generation, total_hits, expected) in captures {
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
        let result = if query.mode == TaskSearchMode::Fts {
            NativeTaskStore::search_lexical(
                fresh.snapshot(),
                &new_read,
                query,
                read_limits(),
                NonZeroUsize::new(4096).unwrap(),
            )
            .await
            .unwrap()
        } else {
            NativeTaskStore::search_semantic(
                fresh.snapshot(),
                &new_read,
                model.clone(),
                &[0., 1.],
                query,
                read_limits(),
                NonZeroUsize::new(4096).unwrap(),
            )
            .await
            .unwrap()
        };
        let TaskLexicalSearchRead::Published {
            generation: actual,
            total_hits: total,
            results,
            next_cursor,
        } = result
        else {
            panic!("published")
        };
        assert_eq!(actual, generation);
        assert!(actual.version < publication.generation.version);
        assert_eq!(total, total_hits);
        assert_eq!(results, vec![expected]);
        assert!(next_cursor.is_none());
    }
    let current = NativeTaskStore::search_semantic(
        ctx.snapshot(),
        &read,
        model,
        &[1., 0.],
        search_input(TaskSearchMode::Semantic),
        read_limits(),
        NonZeroUsize::new(4096).unwrap(),
    )
    .await
    .unwrap();
    let TaskLexicalSearchRead::Published {
        total_hits,
        results,
        generation,
        ..
    } = current
    else {
        panic!("published")
    };
    assert_eq!(generation, publication.generation);
    assert_eq!(total_hits, 3);
    assert_eq!(results[0].task.id, pending_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_search_inherited_cursor_keeps_parent_snapshot_after_child_materialization() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("inherited-pages"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    for key in ["first", "second", "third", "pending"] {
        NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            task_input(session.id, key),
            AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
    }
    let model = identity(93);
    let mut documents = pending(&ctx, &read, &model).await;
    let last = documents.pop().unwrap();
    for (document, vector) in
        documents
            .into_iter()
            .zip([vec![0., 1.], vec![0.6, 0.8], vec![0.8, 0.6]])
    {
        attach(&ctx, &write, document, vector).await;
    }
    let install = graphit_core::project_paths::InstallationPaths::resolve(
        f.installation.brand().clone(),
        &f.root,
        None,
        Some(f.installation.owner_root()),
    )
    .unwrap();
    let paths = graphit_core::project_paths::ProjectPaths::at(&f.root, &install).unwrap();
    let feature = graphit_core::version_context::VersionContext::select(
        &paths,
        graphit_core::version_context::Checkout::NonGit,
        "semantic-pages",
        None,
        "",
    )
    .unwrap();
    let mut scope = f.scope();
    scope.project.as_mut().unwrap().version = &feature;
    let (bearer, reference, _run) = f.issue_for(&issuer, &ctx, scope);
    let feature_grant = issuer
        .verify(bearer.expose_for_transport(), &reference, scope, &ctx)
        .unwrap();
    let feature_read = authority
        .admit_read(&feature_grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let feature_write = authority
        .admit_write(&feature_grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    let mut query = search_input(TaskSearchMode::Hybrid);
    query.top_k = NonZeroUsize::new(3).unwrap();
    let result = NativeTaskStore::search_semantic(
        ctx.snapshot(),
        &feature_read,
        model.clone(),
        &[1., 0.],
        query.clone(),
        read_limits(),
        NonZeroUsize::new(4096).unwrap(),
    )
    .await
    .unwrap();
    let TaskLexicalSearchRead::Published {
        generation,
        total_hits,
        next_cursor,
        ..
    } = result
    else {
        panic!("published")
    };
    assert_eq!(generation.physical_branch, None);
    assert_eq!(
        generation.inherited_target.as_deref(),
        Some(feature.lance_branch().as_str())
    );
    let mut complete = query.clone();
    complete.page_size = NonZeroUsize::new(3).unwrap();
    let expected = hits(
        NativeTaskStore::search_semantic(
            ctx.snapshot(),
            &feature_read,
            model.clone(),
            &[1., 0.],
            complete,
            read_limits(),
            NonZeroUsize::new(4096).unwrap(),
        )
        .await
        .unwrap(),
    )
    .0;
    query.cursor = next_cursor;
    assert!(serde_json::to_value(query.cursor.as_ref().unwrap()).unwrap()["binding"]["branch_provenance"].is_null());
    let publication = attach(&ctx, &feature_write, last, vec![1., 0.]).await;
    assert_eq!(
        publication.generation.physical_branch.as_deref(),
        Some(feature.lance_branch().as_str())
    );
    for expected in expected.into_iter().skip(1) {
        let fresh_cancel = Cancellation::default();
        let fresh = request(&fresh_cancel);
        let fresh_issuer = f.issuer(&fresh);
        let (raw, reference, _run) = f.issue_for(&fresh_issuer, &fresh, scope);
        let fresh_grant = fresh_issuer
            .verify(raw.expose_for_transport(), &reference, scope, &fresh)
            .unwrap();
        let fresh_authority = graphit_auth::ProjectRecordAuthority::new(&fresh_issuer);
        let fresh_read = fresh_authority
            .admit_read(&fresh_grant, &fresh, ProjectReadModule::Task)
            .unwrap();
        let result = NativeTaskStore::search_semantic(
            fresh.snapshot(),
            &fresh_read,
            model.clone(),
            &[0., 1.],
            query.clone(),
            read_limits(),
            NonZeroUsize::new(4096).unwrap(),
        )
        .await
        .unwrap();
        let TaskLexicalSearchRead::Published {
            generation: actual,
            total_hits: total,
            results,
            next_cursor,
        } = result
        else {
            panic!("published")
        };
        assert_eq!(actual, generation);
        assert_eq!(total, total_hits);
        assert_eq!(results, vec![expected]);
        if let Some(next) = &next_cursor {
            assert!(
                serde_json::to_value(next).unwrap()["binding"]["branch_provenance"].is_object()
            );
        }
        query.cursor = next_cursor;
    }
    assert!(query.cursor.is_none());
}
