use super::*;
use arrow_array::{Array, RecordBatch, StringArray};
use futures::TryStreamExt;
use graphit_core::file_lock::Cancellation;
use graphit_data::{lance_branch_store::LanceBranchStore, lance_module_store::LanceModuleStore};
use std::{num::NonZeroUsize, sync::Arc};
fn uri(f: &Fixture) -> String {
    url::Url::from_file_path(f.user.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string()
}
pub(super) async fn physical_rows(f: &Fixture) -> Vec<RecordBatch> {
    let store = LanceBranchStore::open(&uri(f), user_memory_schema(), None)
        .await
        .unwrap();
    let pin = store.capture_target(None).await.unwrap();
    let mut scanner = pin.scan();
    scanner.batch_size(1);
    scanner
        .try_into_stream()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap()
}
pub(super) fn text(batch: &RecordBatch, name: &str) -> String {
    batch
        .column_by_name(name)
        .unwrap()
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap()
        .value(0)
        .to_owned()
}
pub(super) fn replace_text(batch: &RecordBatch, name: &str, value: &str) -> RecordBatch {
    let mut columns = batch.columns().to_vec();
    columns[batch.schema().index_of(name).unwrap()] = Arc::new(StringArray::from(vec![value]));
    RecordBatch::try_new(batch.schema(), columns).unwrap()
}
pub(super) async fn replace(f: &Fixture, batches: Vec<RecordBatch>) {
    // Deliberately corrupt only the SDK dataset created in this isolated fixture.
    let store = LanceModuleStore::open(&uri(f), user_memory_schema())
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
#[tokio::test]
async fn full_eof_rejects_late_duplicate_orphan_marker_foreign_owner_and_missing_audit() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let memory =
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("integrity"), write_limits())
            .await
            .unwrap();
    let good = physical_rows(&f).await;
    assert_eq!(good.len(), 3);
    let marker = good
        .iter()
        .find(|b| text(b, "kind") == "references")
        .unwrap();
    let creation = good.iter().find(|b| text(b, "kind") == "creation").unwrap();
    let head = good.iter().find(|b| text(b, "kind") == "head").unwrap();
    let mut duplicate = good.clone();
    duplicate.push(marker.clone());
    let corrupt = replace_text(
        &replace_text(marker, "row_key", "references/orphan/999"),
        "payload",
        "{broken",
    );
    let mut late = good.clone();
    late.push(corrupt);
    let foreign = replace_text(marker, "user_subject", "another literal principal");
    let missing = vec![head.clone(), marker.clone()];
    let bad_creation = replace_text(creation, "payload", "{}");
    for rows in [
        duplicate,
        late,
        vec![head.clone(), creation.clone(), foreign],
        missing,
        vec![head.clone(), marker.clone(), bad_creation],
    ] {
        replace(&f, rows).await;
        let mut bounds = read_limits();
        bounds.batch_rows = std::num::NonZeroU32::new(1).unwrap();
        // Catalogue reads only live heads. History and indexing validate audits and markers.
        assert!(
            NativeUserMemoryStore::list(ctx.snapshot(), &read, MemoryListFilter::default(), bounds)
                .await
                .is_ok()
        );
        assert!(
            NativeUserMemoryStore::history(ctx.snapshot(), &read, memory.id, bounds)
                .await
                .is_err()
        );
        assert!(
            NativeUserMemoryStore::maintain_lexical_index(
                ctx.snapshot(),
                &write,
                MemoryLexicalIndexOperation::Refresh,
                write_limits()
            )
            .await
            .is_err()
        );
    }
    replace(&f, good).await;
    assert_eq!(
        NativeUserMemoryStore::get(ctx.snapshot(), &read, memory.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory,
        memory
    );
}
#[tokio::test]
async fn exact_input_guard_and_complete_eof_remain_independent_of_query_bound() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let input = input("exact-input");
    let bytes = input.idempotency_key.len()
        + input.title.len()
        + input.body.len()
        + input.tags.iter().map(String::len).sum::<usize>();
    let mut limits = write_limits();
    limits.max_input_bytes = NonZeroUsize::new(bytes - 1).unwrap();
    assert!(
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input.clone(), limits)
            .await
            .is_err()
    );
    assert!(!f.user.runtime_root().exists());
    limits.max_input_bytes = NonZeroUsize::new(bytes).unwrap();
    let memory = NativeUserMemoryStore::create(ctx.snapshot(), &write, input, limits)
        .await
        .unwrap();
    let mut exact = read_limits();
    exact.max_query_bytes = NonZeroUsize::MIN;
    exact.batch_rows = std::num::NonZeroU32::MIN;
    assert_eq!(
        NativeUserMemoryStore::get(ctx.snapshot(), &read, memory.id, exact)
            .await
            .unwrap()
            .unwrap()
            .memory,
        memory
    );
    assert!(matches!(
        NativeUserMemoryStore::list(ctx.snapshot(), &read, MemoryListFilter::default(), exact)
            .await
            .unwrap(),
        UserMemoryListRead::Published { .. }
    ));
}

#[tokio::test]
async fn metadata_ignores_body_but_owner_projection_and_eof_still_apply() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let first = NativeUserMemoryStore::create(
        ctx.snapshot(),
        &write,
        input("metadata-one"),
        write_limits(),
    )
    .await
    .unwrap();
    NativeUserMemoryStore::create(
        ctx.snapshot(),
        &write,
        input("metadata-two"),
        write_limits(),
    )
    .await
    .unwrap();
    let mut rows = physical_rows(&f).await;
    for row in &mut rows {
        if text(row, "kind") == "head" {
            *row = replace_text(row, "payload", "{malformed excluded body");
        }
    }
    replace(&f, rows).await;
    let UserMemoryHeadRead::Published { heads, .. } =
        NativeUserMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .unwrap()
    else {
        panic!("metadata projection")
    };
    assert_eq!(heads.len(), 2);
    assert!(heads.iter().any(|h| h.id == first.id));
    assert!(
        NativeUserMemoryStore::list(
            ctx.snapshot(),
            &read,
            MemoryListFilter::default(),
            read_limits()
        )
        .await
        .is_err()
    );
    let mut exact = head_limits();
    exact.batch_rows = std::num::NonZeroU32::MIN;
    let UserMemoryHeadRead::Published { heads, .. } =
        NativeUserMemoryStore::observe_heads(ctx.snapshot(), &read, exact)
            .await
            .unwrap()
    else {
        panic!("published heads")
    };
    assert_eq!(heads.len(), 2);
}

#[tokio::test]
async fn actual_empty_dataset_has_published_empty_and_missing_index_is_integrity_error() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let first =
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("empty-start"), write_limits())
            .await
            .unwrap();
    replace(&f, vec![RecordBatch::new_empty(user_memory_schema())]).await;
    assert!(matches!(
        NativeUserMemoryStore::list(
            ctx.snapshot(),
            &read,
            MemoryListFilter::default(),
            read_limits()
        )
        .await
        .unwrap(),
        UserMemoryListRead::Published { memories, .. } if memories.is_empty()
    ));
    assert!(
        matches!(NativeUserMemoryStore::observe_heads(ctx.snapshot(),&read,head_limits()).await.unwrap(),UserMemoryHeadRead::Published { heads,.. } if heads.is_empty())
    );
    assert!(
        NativeUserMemoryStore::get(ctx.snapshot(), &read, first.id, read_limits())
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        NativeUserMemoryStore::search_lexical(
            ctx.snapshot(),
            &read,
            MemoryLexicalQuery {
                text: "quartz".into(),
                filter: Default::default(),
                include_deleted_history: false,
                top_k: NonZeroUsize::new(2).unwrap()
            },
            read_limits()
        )
        .await
        .unwrap(),
        UserMemoryLexicalSearchRead::PublishedEmpty { .. }
    ));
    std::fs::rename(
        f.user.runtime_root().join("memory/index.lance"),
        f.user.runtime_root().join("memory/moved.lance"),
    )
    .unwrap();
    assert!(matches!(
        NativeUserMemoryStore::get(ctx.snapshot(), &read, first.id, read_limits()).await,
        Err(MemoryStoreError::IncompleteCorpus)
    ));
    assert!(!f.user.runtime_root().join("memory/index.lance").exists());
}

#[tokio::test]
async fn catalogue_rejects_late_malformed_and_duplicate_live_heads_without_prefix() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    NativeUserMemoryStore::create(ctx.snapshot(), &write, input("valid-head"), write_limits())
        .await
        .unwrap();
    let second =
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("late-head"), write_limits())
            .await
            .unwrap();
    let good = physical_rows(&f).await;
    let second_head = good
        .iter()
        .find(|b| text(b, "kind") == "head" && text(b, "record_id") == second.id.to_string())
        .unwrap();
    let mut duplicate = good.clone();
    duplicate.push(second_head.clone());
    let malformed: Vec<_> = good
        .iter()
        .map(|b| {
            if text(b, "kind") == "head" && text(b, "record_id") == second.id.to_string() {
                replace_text(b, "payload", "{}")
            } else {
                b.clone()
            }
        })
        .collect();
    let mut exact = read_limits();
    exact.max_query_bytes = NonZeroUsize::MIN;
    exact.batch_rows = std::num::NonZeroU32::MIN;
    assert!(
        NativeUserMemoryStore::list(ctx.snapshot(), &read, MemoryListFilter::default(), exact)
            .await
            .is_ok()
    );
    for rows in [duplicate, malformed] {
        replace(&f, rows).await;
        assert!(
            NativeUserMemoryStore::list(
                ctx.snapshot(),
                &read,
                MemoryListFilter::default(),
                read_limits()
            )
            .await
            .is_err()
        );
    }
}
