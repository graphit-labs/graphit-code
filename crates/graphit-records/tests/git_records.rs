#![cfg(target_os = "linux")]
#[path = "memory_store/fixture.rs"]
mod fixture;
use fixture::*;
use graphit_auth::{
    ProjectReadAdmission, ProjectReadModule, ProjectRecordAuthority, ProjectWriteAdmission,
    ProjectWriteOperation,
};
use graphit_core::{
    file_io::SnapshotContext,
    file_lock::Cancellation,
    project_paths::{InstallationPaths, ProjectPaths},
    version_context::{Checkout, VersionContext},
};
use graphit_records::{
    git_records::{self, GitRecordModule, GitRecordsOutcome, GitRecordsTransition as Transition},
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store as memory_api,
    memory_store::*,
    reference_projection::AuthoredReferencesField,
    task_spec::CheckKind,
    task_store::{self, NativeTaskStore, TaskReadView},
};
use std::{
    num::{NonZeroU32, NonZeroU64, NonZeroUsize},
    sync::Arc,
};
const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const C: &str = "cccccccccccccccccccccccccccccccccccccccc";

fn task_limits() -> task_store::TaskStoreWriteLimits {
    let memory = write_limits();
    task_store::TaskStoreWriteLimits {
        read: task_store::TaskStoreReadLimits {
            native: memory.read.native,
            batch_rows: memory.read.batch_rows,
        },
        references: task_store::TaskReferencesInputLimits {
            max_entries: NonZeroUsize::new(65536).unwrap(),
            max_text_bytes: NonZeroUsize::new(1048576).unwrap(),
        },
        artifacts: memory.artifacts,
    }
}
fn input(key: &str) -> CreateMemory {
    CreateMemory {
        idempotency_key: key.into(),
        title: format!("Memory {key}"),
        body: format!("Body {key}"),
        attributes: MemoryAttributes {
            classification: MemoryClass::Decision,
            mandatory: false,
            important: false,
        },
        tags: vec!["Unicode β".into()],
        references: None,
    }
}
fn update(memory: &Memory, key: &str) -> UpdateMemory {
    UpdateMemory {
        expected_revision: memory.revision,
        idempotency_key: key.into(),
        reason: "Actual user edit".into(),
        title: None,
        body: Some(key.into()),
        classification: None,
        mandatory: None,
        important: None,
        tags: None,
        references: None,
    }
}
fn version(f: &Fixture, name: &str) -> VersionContext {
    let installation = InstallationPaths::resolve(
        f.installation.brand().clone(),
        &f.root,
        None,
        Some(f.installation.owner_root()),
    )
    .unwrap();
    let paths = ProjectPaths::at(&f.root, &installation).unwrap();
    VersionContext::select(&paths, Checkout::NonGit, name, None, "").unwrap()
}
macro_rules! admit {
    ($authority:ident,$grant:ident,$ctx:ident,$mr:ident,$mw:ident,$tr:ident,$tw:ident) => {
        let $mr = $authority
            .admit_read(&$grant, &$ctx, ProjectReadModule::Memory)
            .unwrap();
        let $mw = $authority
            .admit_write(&$grant, &$ctx, ProjectWriteOperation::Memory)
            .unwrap();
        let $tr = $authority
            .admit_read(&$grant, &$ctx, ProjectReadModule::Task)
            .unwrap();
        let $tw = $authority
            .admit_write(&$grant, &$ctx, ProjectWriteOperation::Task)
            .unwrap();
    };
}
macro_rules! setup {
    ($ctx:ident,$f:ident,$issuer:ident,$authority:ident,$reference:ident,$mr:ident,$mw:ident,$tr:ident,$tw:ident) => {
        let $f = Fixture::new(&$ctx, "main");
        let $issuer = $f.issuer(&$ctx);
        let (bearer, $reference, _run, _admin) = $f.issue(&$issuer, &$ctx);
        let grant = $issuer
            .verify(
                bearer.expose_for_transport(),
                &$reference,
                $f.scope(),
                &$ctx,
            )
            .unwrap();
        let $authority = ProjectRecordAuthority::new(&$issuer);
        admit!($authority, grant, $ctx, $mr, $mw, $tr, $tw);
    };
}
macro_rules! branch {
    ($ctx:ident,$f:ident,$issuer:ident,$authority:ident,$reference:ident,$name:expr,$mr:ident,$mw:ident,$tr:ident,$tw:ident) => {
        let selected_version = version(&$f, $name);
        let mut selected_scope = $f.scope();
        selected_scope.project.as_mut().unwrap().version = &selected_version;
        let (selected_bearer, _selected_run) =
            $f.issue_registered_for(&$issuer, &$ctx, selected_scope, &$reference);
        let selected_grant = $issuer
            .verify(
                selected_bearer.expose_for_transport(),
                &$reference,
                selected_scope,
                &$ctx,
            )
            .unwrap();
        admit!($authority, selected_grant, $ctx, $mr, $mw, $tr, $tw);
    };
}
async fn apply(
    original: &Arc<SnapshotContext>,
    mw: &ProjectWriteAdmission<'_>,
    tw: &ProjectWriteAdmission<'_>,
    transition: Transition,
) -> GitRecordsOutcome {
    git_records::apply(original, mw, tw, transition, write_limits(), task_limits())
        .await
        .unwrap()
}
fn applied(outcome: GitRecordsOutcome) {
    assert!(
        matches!(outcome, GitRecordsOutcome::Applied { .. }),
        "{outcome:?}"
    );
}
async fn new_task(
    original: &Arc<SnapshotContext>,
    write: &ProjectWriteAdmission<'_>,
    key: &str,
) -> task_store::Task {
    let session = NativeTaskStore::create_session(
        original,
        write,
        task_store::CreateSession {
            idempotency_key: format!("session-{key}"),
            title: key.into(),
            description: "Current work".into(),
            strategy: "Implement actual behavior".into(),
        },
        AuthoredReferencesField::Omitted,
        task_limits(),
    )
    .await
    .unwrap();
    NativeTaskStore::create_task(
        original,
        write,
        task_store::CreateTask {
            commit: None,
            idempotency_key: format!("task-{key}"),
            session_id: session.id,
            parent_id: None,
            depends_on: vec![],
            title: key.into(),
            description: "Actual work".into(),
            task_type: "feature".into(),
            priority: 2,
            schedule: None,
            checks: vec![
                task_store::CreateCheck {
                    kind: CheckKind::Acceptance,
                    text: "Requested behavior works".into(),
                },
                task_store::CreateCheck {
                    kind: CheckKind::Test,
                    text: "Affected consumer passes".into(),
                },
            ],
        },
        AuthoredReferencesField::Omitted,
        task_limits(),
    )
    .await
    .unwrap()
}
async fn task_ids(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
) -> Vec<graphit_records::task_spec::TaskId> {
    match NativeTaskStore::read_view(original, read, task_limits().read)
        .await
        .unwrap()
    {
        TaskReadView::Published { tasks, .. } => tasks.into_iter().map(|task| task.id).collect(),
        TaskReadView::NoCorpus => vec![],
    }
}

#[tokio::test]
async fn git_initialize_empty_and_commit_are_real_idempotent_snapshots() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    setup!(ctx, f, issuer, authority, reference, mr, mw, tr, tw);
    applied(
        apply(
            ctx.snapshot(),
            &mw,
            &tw,
            Transition::Initialize { commit: A.into() },
        )
        .await,
    );
    let GitRecordsOutcome::Applied {
        memory: first_memory,
        task: first_task,
    } = apply(
        ctx.snapshot(),
        &mw,
        &tw,
        Transition::Initialize { commit: A.into() },
    )
    .await
    else {
        panic!("idempotent initialize")
    };
    assert!(first_memory.version > 0 && first_task.version > 0);
    let MemoryHeadRead::Published { heads, .. } =
        NativeMemoryStore::observe_heads(ctx.snapshot(), &mr, head_limits())
            .await
            .unwrap()
    else {
        panic!("empty Memory authoritative store")
    };
    assert!(heads.is_empty());
    assert!(task_ids(ctx.snapshot(), &tr).await.is_empty());
    let memory = NativeMemoryStore::create(ctx.snapshot(), &mw, input("new"), write_limits())
        .await
        .unwrap();
    let task = new_task(ctx.snapshot(), &tw, "new").await;
    applied(
        apply(
            ctx.snapshot(),
            &mw,
            &tw,
            Transition::Commit { commit: B.into() },
        )
        .await,
    );
    applied(
        apply(
            ctx.snapshot(),
            &mw,
            &tw,
            Transition::Commit { commit: B.into() },
        )
        .await,
    );
    assert_eq!(
        task_ids(ctx.snapshot(), &tr).await,
        vec![task.specification.id]
    );
    applied(
        apply(
            ctx.snapshot(),
            &mw,
            &tw,
            Transition::Checkout {
                commit: A.into(),
                previous_commit: Some(B.into()),
            },
        )
        .await,
    );
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &mr, memory.id, read_limits())
            .await
            .unwrap()
            .is_none()
    );
    assert!(task_ids(ctx.snapshot(), &tr).await.is_empty());
    // Restore brought back the genuinely empty tag, without an FTS index.
    // A subsequent raw Git delta must activate the normal searchable corpus.
    applied(
        apply(
            ctx.snapshot(),
            &mw,
            &tw,
            Transition::Merge {
                base: A.into(),
                theirs: B.into(),
            },
        )
        .await,
    );
    let MemoryLexicalSearchRead::Published {
        total_hits, hits, ..
    } = NativeMemoryStore::search_lexical(
        ctx.snapshot(),
        &mr,
        MemoryLexicalQuery {
            text: "new".into(),
            filter: MemoryListFilter::default(),
            include_deleted_history: false,
            top_k: NonZeroUsize::new(10).unwrap(),
        },
        read_limits(),
    )
    .await
    .unwrap()
    else {
        panic!("restored-and-merged Memory is searchable")
    };
    assert_eq!(total_hits, 1);
    assert_eq!(hits[0].current.id, memory.id);
    let task_store::TaskLexicalSearchRead::Published {
        total_hits,
        results,
        ..
    } = NativeTaskStore::search_lexical(
        ctx.snapshot(),
        &tr,
        task_store::TaskLexicalQuery {
            mode: task_store::TaskSearchMode::Fts,
            text: "new".into(),
            filter: task_store::TaskSearchFilter::default(),
            page_size: NonZeroUsize::new(10).unwrap(),
            top_k: NonZeroUsize::new(10).unwrap(),
            cursor: None,
        },
        task_limits().read,
        NonZeroUsize::new(65536).unwrap(),
    )
    .await
    .unwrap()
    else {
        panic!("restored-and-merged Task is searchable")
    };
    assert_eq!(total_hits, 1);
    assert_eq!(results[0].task.id, task.specification.id);
    let GitRecordsOutcome::Blocked { conflicts, missing } = apply(
        ctx.snapshot(),
        &mw,
        &tw,
        Transition::Commit { commit: A.into() },
    )
    .await
    else {
        panic!("immutable commit must not move")
    };
    assert!(missing.is_empty());
    assert!(!conflicts.is_empty());
}

#[tokio::test]
async fn git_checkout_uses_exact_tags_and_preserves_dirty_or_missing_work() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    setup!(ctx, f, issuer, authority, reference, mr, mw, tr, tw);
    let original =
        NativeMemoryStore::create(ctx.snapshot(), &mw, input("original"), write_limits())
            .await
            .unwrap();
    let first = new_task(ctx.snapshot(), &tw, "original").await;
    applied(
        apply(
            ctx.snapshot(),
            &mw,
            &tw,
            Transition::Commit { commit: A.into() },
        )
        .await,
    );
    branch!(
        ctx,
        f,
        issuer,
        authority,
        reference,
        "feature/checkout",
        bmr,
        bmw,
        btr,
        btw
    );
    applied(
        apply(
            ctx.snapshot(),
            &bmw,
            &btw,
            Transition::Checkout {
                commit: A.into(),
                previous_commit: None,
            },
        )
        .await,
    );
    for module in ["memory", "task"] {
        let canonical = format!("{module}/index.lance");
        for selected_version in ["main", "feature/checkout"] {
            assert_eq!(
                graphit_core::local_corpus::resolve(
                    &f.project,
                    selected_version,
                    module,
                    "index.lance",
                    read_limits().native,
                    ctx.snapshot(),
                )
                .unwrap(),
                std::path::PathBuf::from(&canonical),
            );
        }
        assert!(f.project.runtime_root().join(&canonical).is_dir());
        assert_eq!(
            std::fs::read_dir(f.project.runtime_root().join(module))
                .unwrap()
                .count(),
            1
        );
    }
    assert_eq!(
        NativeMemoryStore::get(ctx.snapshot(), &bmr, original.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory,
        original
    );
    let added = NativeMemoryStore::create(ctx.snapshot(), &bmw, input("feature"), write_limits())
        .await
        .unwrap();
    let second = new_task(ctx.snapshot(), &btw, "feature").await;
    applied(
        apply(
            ctx.snapshot(),
            &bmw,
            &btw,
            Transition::Commit { commit: B.into() },
        )
        .await,
    );
    assert_eq!(
        task_ids(ctx.snapshot(), &tr).await,
        vec![first.specification.id]
    );
    applied(
        apply(
            ctx.snapshot(),
            &mw,
            &tw,
            Transition::Checkout {
                commit: B.into(),
                previous_commit: Some(A.into()),
            },
        )
        .await,
    );
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &mr, added.id, read_limits())
            .await
            .unwrap()
            .is_some()
    );
    let ids = task_ids(ctx.snapshot(), &tr).await;
    assert!(ids.contains(&first.specification.id) && ids.contains(&second.specification.id));
    // Exercise the SDK's same-physical-branch Restore as well as the
    // cross-branch record delta above; both must retain exact public history.
    applied(
        apply(
            ctx.snapshot(),
            &mw,
            &tw,
            Transition::Commit { commit: C.into() },
        )
        .await,
    );
    applied(
        apply(
            ctx.snapshot(),
            &mw,
            &tw,
            Transition::Checkout {
                commit: A.into(),
                previous_commit: Some(C.into()),
            },
        )
        .await,
    );
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &mr, added.id, read_limits())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        task_ids(ctx.snapshot(), &tr).await,
        vec![first.specification.id]
    );
    applied(
        apply(
            ctx.snapshot(),
            &mw,
            &tw,
            Transition::Checkout {
                commit: C.into(),
                previous_commit: Some(A.into()),
            },
        )
        .await,
    );
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &mr, added.id, read_limits())
            .await
            .unwrap()
            .is_some()
    );
    let restored = task_ids(ctx.snapshot(), &tr).await;
    assert!(
        restored.contains(&first.specification.id) && restored.contains(&second.specification.id)
    );
    let dirty = NativeMemoryStore::create(ctx.snapshot(), &mw, input("dirty"), write_limits())
        .await
        .unwrap();
    let GitRecordsOutcome::Blocked { conflicts, missing } = apply(
        ctx.snapshot(),
        &mw,
        &tw,
        Transition::Checkout {
            commit: A.into(),
            previous_commit: Some(B.into()),
        },
    )
    .await
    else {
        panic!("dirty checkout must block")
    };
    assert!(missing.is_empty());
    assert!(
        conflicts
            .iter()
            .any(|c| c.module == GitRecordModule::Memory && c.reason == "dirty_records")
    );
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &mr, dirty.id, read_limits())
            .await
            .unwrap()
            .is_some()
    );
    let GitRecordsOutcome::Blocked { missing, .. } = apply(
        ctx.snapshot(),
        &mw,
        &tw,
        Transition::Checkout {
            commit: "dddddddddddddddddddddddddddddddddddddddd".into(),
            previous_commit: Some(B.into()),
        },
    )
    .await
    else {
        panic!("missing tags must block")
    };
    assert_eq!(missing.len(), 2);
    assert!(
        missing
            .iter()
            .all(|missing| missing.commit == "dddddddddddddddddddddddddddddddddddddddd")
    );
    assert_eq!(task_ids(ctx.snapshot(), &btr).await.len(), 2);
}

#[tokio::test]
async fn git_three_way_merge_preserves_disjoint_task_and_memory_families() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    setup!(ctx, f, issuer, authority, reference, mr, mw, tr, tw);
    applied(
        apply(
            ctx.snapshot(),
            &mw,
            &tw,
            Transition::Initialize { commit: A.into() },
        )
        .await,
    );
    branch!(
        ctx,
        f,
        issuer,
        authority,
        reference,
        "feature/merge",
        bmr,
        bmw,
        btr,
        btw
    );
    applied(
        apply(
            ctx.snapshot(),
            &bmw,
            &btw,
            Transition::Checkout {
                commit: A.into(),
                previous_commit: None,
            },
        )
        .await,
    );
    let theirs_memory =
        NativeMemoryStore::create(ctx.snapshot(), &bmw, input("theirs"), write_limits())
            .await
            .unwrap();
    let theirs_task = new_task(ctx.snapshot(), &btw, "theirs").await;
    applied(
        apply(
            ctx.snapshot(),
            &bmw,
            &btw,
            Transition::Commit { commit: B.into() },
        )
        .await,
    );
    let ours_memory = NativeMemoryStore::create(ctx.snapshot(), &mw, input("ours"), write_limits())
        .await
        .unwrap();
    let ours_task = new_task(ctx.snapshot(), &tw, "ours").await;
    applied(
        apply(
            ctx.snapshot(),
            &mw,
            &tw,
            Transition::Merge {
                base: A.into(),
                theirs: B.into(),
            },
        )
        .await,
    );
    for id in [ours_memory.id, theirs_memory.id] {
        assert!(
            NativeMemoryStore::get(ctx.snapshot(), &mr, id, read_limits())
                .await
                .unwrap()
                .is_some()
        );
    }
    let ids = task_ids(ctx.snapshot(), &tr).await;
    assert_eq!(ids.len(), 2);
    assert!(
        ids.contains(&ours_task.specification.id) && ids.contains(&theirs_task.specification.id)
    );
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &bmr, ours_memory.id, read_limits())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        task_ids(ctx.snapshot(), &btr).await,
        vec![theirs_task.specification.id]
    );
}

#[tokio::test]
async fn git_conflicting_memory_prevents_task_module_partial_merge() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    setup!(ctx, f, issuer, authority, reference, mr, mw, tr, tw);
    let base = NativeMemoryStore::create(ctx.snapshot(), &mw, input("base"), write_limits())
        .await
        .unwrap();
    applied(
        apply(
            ctx.snapshot(),
            &mw,
            &tw,
            Transition::Commit { commit: A.into() },
        )
        .await,
    );
    branch!(
        ctx,
        f,
        issuer,
        authority,
        reference,
        "feature/conflict",
        bmr,
        bmw,
        btr,
        btw
    );
    applied(
        apply(
            ctx.snapshot(),
            &bmw,
            &btw,
            Transition::Checkout {
                commit: A.into(),
                previous_commit: None,
            },
        )
        .await,
    );
    NativeMemoryStore::update(
        ctx.snapshot(),
        &bmw,
        base.id,
        update(&base, "theirs"),
        write_limits(),
    )
    .await
    .unwrap();
    let theirs_task = new_task(ctx.snapshot(), &btw, "theirs").await;
    applied(
        apply(
            ctx.snapshot(),
            &bmw,
            &btw,
            Transition::Commit { commit: B.into() },
        )
        .await,
    );
    let ours = NativeMemoryStore::update(
        ctx.snapshot(),
        &mw,
        base.id,
        update(&base, "ours"),
        write_limits(),
    )
    .await
    .unwrap();
    let before = NativeTaskStore::read_view(ctx.snapshot(), &tr, task_limits().read)
        .await
        .unwrap();
    let GitRecordsOutcome::Blocked { conflicts, missing } = apply(
        ctx.snapshot(),
        &mw,
        &tw,
        Transition::Merge {
            base: A.into(),
            theirs: B.into(),
        },
    )
    .await
    else {
        panic!("same record edits conflict")
    };
    assert!(missing.is_empty());
    assert!(conflicts.iter().any(|c| c.module == GitRecordModule::Memory
        && c.record_id == base.id.to_string()
        && c.reason == "concurrent_record_edit"));
    assert_eq!(
        NativeTaskStore::read_view(ctx.snapshot(), &tr, task_limits().read)
            .await
            .unwrap(),
        before
    );
    assert!(
        !task_ids(ctx.snapshot(), &tr)
            .await
            .contains(&theirs_task.specification.id)
    );
    assert_eq!(
        NativeMemoryStore::get(ctx.snapshot(), &mr, base.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory,
        ours
    );
    assert_eq!(
        NativeMemoryStore::get(ctx.snapshot(), &bmr, base.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory
            .body,
        "theirs"
    );
    assert_eq!(
        task_ids(ctx.snapshot(), &btr).await,
        vec![theirs_task.specification.id]
    );
}

#[tokio::test]
async fn git_commit_is_idempotent_and_reuses_post_tag_embeddings() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    setup!(ctx, f, issuer, authority, reference, mr, mw, tr, tw);
    let memory = NativeMemoryStore::create(ctx.snapshot(), &mw, input("vectors"), write_limits())
        .await
        .unwrap();
    let task = new_task(ctx.snapshot(), &tw, "vectors").await;
    // Public Git state is captured before background inference completes.
    let GitRecordsOutcome::Applied {
        memory: tag_memory,
        task: tag_task,
    } = apply(
        ctx.snapshot(),
        &mw,
        &tw,
        Transition::Commit { commit: A.into() },
    )
    .await
    else {
        panic!("original Git snapshot")
    };
    let identity = MemoryEmbeddingIdentity::new(
        [7; 32],
        NonZeroU32::new(2).unwrap(),
        MEMORY_DOCUMENT_RECIPE.into(),
    )
    .unwrap();
    let MemoryEmbeddingCorpusRead::Published(mut corpus) =
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &mr, identity.clone(), read_limits())
            .await
            .unwrap()
    else {
        panic!("Memory corpus")
    };
    let documents = corpus
        .next_documents(ctx.snapshot(), &mr)
        .await
        .unwrap()
        .unwrap();
    NativeMemoryStore::attach_embeddings(
        ctx.snapshot(),
        &mw,
        &corpus,
        documents
            .iter()
            .map(|d| MemoryEmbeddingAttachment {
                memory_id: d.memory_id(),
                revision: NonZeroU64::new(d.revision()).unwrap(),
                input_fingerprint: *d.fingerprint(),
                vector: vec![1., 0.],
            })
            .collect(),
        write_limits(),
    )
    .await
    .unwrap();
    let task_identity = task_store::TaskEmbeddingIdentity::new(
        [8; 32],
        NonZeroU32::new(2).unwrap(),
        task_store::TASK_DOCUMENT_RECIPE.into(),
    )
    .unwrap();
    let task_store::TaskEmbeddingCorpusRead::Published(mut task_corpus) =
        NativeTaskStore::capture_embeddings(
            ctx.snapshot(),
            &tr,
            task_identity.clone(),
            task_limits().read,
        )
        .await
        .unwrap()
    else {
        panic!("Task corpus")
    };
    let task_documents = task_corpus
        .next_documents(ctx.snapshot(), &tr, NonZeroUsize::new(10).unwrap())
        .await
        .unwrap()
        .unwrap();
    NativeTaskStore::attach_embeddings(
        ctx.snapshot(),
        &tw,
        task_documents
            .into_iter()
            .map(|document| task_store::TaskEmbeddingAttachment {
                document,
                vector: vec![1., 0.],
            })
            .collect(),
        task_limits(),
    )
    .await
    .unwrap();
    // Derived vectors cannot move an already published immutable Git tag.
    let GitRecordsOutcome::Applied {
        memory: unchanged_memory,
        task: unchanged_task,
    } = apply(
        ctx.snapshot(),
        &mw,
        &tw,
        Transition::Commit { commit: A.into() },
    )
    .await
    else {
        panic!("background-only publication is not a public edit")
    };
    assert_eq!(unchanged_memory.version, tag_memory.version);
    assert_eq!(unchanged_task.version, tag_task.version);
    applied(
        apply(
            ctx.snapshot(),
            &mw,
            &tw,
            Transition::Commit { commit: B.into() },
        )
        .await,
    );
    applied(
        apply(
            ctx.snapshot(),
            &mw,
            &tw,
            Transition::Commit { commit: B.into() },
        )
        .await,
    );
    branch!(
        ctx,
        f,
        issuer,
        authority,
        reference,
        "feature/post-tag",
        bmr,
        bmw,
        btr,
        btw
    );
    applied(
        apply(
            ctx.snapshot(),
            &bmw,
            &btw,
            Transition::Checkout {
                commit: A.into(),
                previous_commit: None,
            },
        )
        .await,
    );
    assert_eq!(
        NativeMemoryStore::get(ctx.snapshot(), &bmr, memory.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory,
        memory
    );
    assert_eq!(
        task_ids(ctx.snapshot(), &btr).await,
        vec![task.specification.id]
    );
    // Keep the exact old public snapshot and reuse compatible post-tag vectors
    // from the current corpus, without moving either immutable commit tag.
    let MemoryEmbeddingCorpusRead::Published(mut reused) = NativeMemoryStore::capture_embeddings(
        ctx.snapshot(),
        &bmr,
        identity.clone(),
        read_limits(),
    )
    .await
    .unwrap() else {
        panic!("Memory vector snapshot")
    };
    assert!(
        reused
            .next_documents(ctx.snapshot(), &bmr)
            .await
            .unwrap()
            .is_none()
    );
    let task_store::TaskEmbeddingCorpusRead::Published(mut reused) =
        NativeTaskStore::capture_embeddings(
            ctx.snapshot(),
            &btr,
            task_identity.clone(),
            task_limits().read,
        )
        .await
        .unwrap()
    else {
        panic!("Task vector snapshot")
    };
    assert!(
        reused
            .next_documents(ctx.snapshot(), &btr, NonZeroUsize::new(10).unwrap())
            .await
            .unwrap()
            .is_none()
    );
    let MemorySemanticSearchRead::Published {
        total_hits, hits, ..
    } = NativeMemoryStore::search_semantic(
        ctx.snapshot(),
        &bmr,
        identity,
        &[1., 0.],
        MemorySemanticSearchOptions {
            filter: MemoryListFilter::default(),
            include_deleted_history: false,
            top_k: NonZeroUsize::new(10).unwrap(),
        },
        read_limits(),
    )
    .await
    .unwrap()
    else {
        panic!("Memory semantic query uses grafted vectors")
    };
    assert_eq!(total_hits, 1);
    assert_eq!(hits[0].current.id, memory.id);
    let task_store::TaskLexicalSearchRead::Published {
        total_hits,
        results,
        ..
    } = NativeTaskStore::search_semantic(
        ctx.snapshot(),
        &btr,
        task_identity,
        &[1., 0.],
        task_store::TaskLexicalQuery {
            mode: task_store::TaskSearchMode::Semantic,
            text: "vectors".into(),
            filter: task_store::TaskSearchFilter::default(),
            page_size: NonZeroUsize::new(10).unwrap(),
            top_k: NonZeroUsize::new(10).unwrap(),
            cursor: None,
        },
        task_limits().read,
        NonZeroUsize::new(65536).unwrap(),
    )
    .await
    .unwrap()
    else {
        panic!("Task semantic query uses grafted vectors")
    };
    assert_eq!(total_hits, 1);
    assert_eq!(results[0].task.id, task.specification.id);
    cancel.cancel();
    let error = git_records::apply(
        ctx.snapshot(),
        &bmw,
        &btw,
        Transition::Commit { commit: C.into() },
        write_limits(),
        task_limits(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code(), "request_cancelled_or_expired");
    assert!(error.partial.is_empty());
}

#[tokio::test]
async fn git_cancel_after_admission_while_writer_is_busy_publishes_nothing() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    setup!(ctx, f, issuer, authority, reference, mr, mw, tr, tw);
    applied(
        apply(
            ctx.snapshot(),
            &mw,
            &tw,
            Transition::Initialize { commit: A.into() },
        )
        .await,
    );
    let memory_before = NativeMemoryStore::observe_heads(ctx.snapshot(), &mr, head_limits())
        .await
        .unwrap();
    let task_before = NativeTaskStore::read_view(ctx.snapshot(), &tr, task_limits().read)
        .await
        .unwrap();
    let limits = write_limits();
    let artifacts =
        graphit_core::artifact_store::ArtifactStore::new(f.project.clone(), limits.artifacts);
    let writer = artifacts.writer_with_context(ctx.snapshot()).unwrap();
    let pending = git_records::apply(
        ctx.snapshot(),
        &mw,
        &tw,
        Transition::Commit { commit: B.into() },
        write_limits(),
        task_limits(),
    );
    futures::pin_mut!(pending);
    assert!(futures::poll!(pending.as_mut()).is_pending());
    cancel.cancel();
    drop(writer);
    let error = pending.await.unwrap_err();
    assert!(error.partial.is_empty(), "{error:?}");
    let kind = match &error.cause {
        git_records::GitRecordsCause::Request(error)
        | git_records::GitRecordsCause::Memory(MemoryStoreError::Request(error)) => error.kind(),
        git_records::GitRecordsCause::Memory(MemoryStoreError::Artifact(error)) => error.kind(),
        other => panic!("request cancellation must retain its cause: {other:?}"),
    };
    assert!(matches!(
        kind,
        std::io::ErrorKind::Interrupted | std::io::ErrorKind::TimedOut
    ));
    let fresh_cancel = Cancellation::default();
    let fresh_ctx = request(&fresh_cancel);
    let (bearer, _run) = f.issue_registered_for(&issuer, &fresh_ctx, f.scope(), &reference);
    let grant = issuer
        .verify(
            bearer.expose_for_transport(),
            &reference,
            f.scope(),
            &fresh_ctx,
        )
        .unwrap();
    let fresh_memory = authority
        .admit_read(&grant, &fresh_ctx, ProjectReadModule::Memory)
        .unwrap();
    let fresh_task = authority
        .admit_read(&grant, &fresh_ctx, ProjectReadModule::Task)
        .unwrap();
    let fresh_memory_write = authority
        .admit_write(&grant, &fresh_ctx, ProjectWriteOperation::Memory)
        .unwrap();
    let fresh_task_write = authority
        .admit_write(&grant, &fresh_ctx, ProjectWriteOperation::Task)
        .unwrap();
    assert_eq!(
        NativeMemoryStore::observe_heads(fresh_ctx.snapshot(), &fresh_memory, head_limits())
            .await
            .unwrap(),
        memory_before
    );
    assert_eq!(
        NativeTaskStore::read_view(fresh_ctx.snapshot(), &fresh_task, task_limits().read)
            .await
            .unwrap(),
        task_before
    );
    let GitRecordsOutcome::Blocked { missing, .. } = apply(
        fresh_ctx.snapshot(),
        &fresh_memory_write,
        &fresh_task_write,
        Transition::Checkout {
            commit: B.into(),
            previous_commit: Some(A.into()),
        },
    )
    .await
    else {
        panic!("cancelled transition did not publish tags")
    };
    assert_eq!(missing.len(), 2);
}
