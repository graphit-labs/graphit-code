use super::*;
use graphit_core::file_lock::Cancellation;
use std::num::NonZeroUsize;
fn query(text: &str, archive: bool) -> MemoryLexicalQuery {
    MemoryLexicalQuery {
        text: text.into(),
        filter: MemoryListFilter::default(),
        include_deleted_history: archive,
        top_k: NonZeroUsize::new(10).unwrap(),
    }
}
#[tokio::test]
async fn actual_index_fts_archive_and_current_filters_use_user_generations() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    assert_eq!(
        NativeUserMemoryStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("quartz", false),
            read_limits()
        )
        .await
        .unwrap(),
        UserMemoryLexicalSearchRead::NoCorpus
    );
    let first =
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("search"), write_limits())
            .await
            .unwrap();
    let second = NativeUserMemoryStore::update(
        ctx.snapshot(),
        &write,
        first.id,
        update("search-update", 1),
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeUserMemoryStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("quartz", false),
            read_limits()
        )
        .await
        .unwrap(),
        UserMemoryLexicalSearchRead::Published { total_hits: 1, .. }
    ));
    let UserMemoryLexicalSearchRead::Published {
        hits,
        total_hits,
        generation,
    } = NativeUserMemoryStore::search_lexical(
        ctx.snapshot(),
        &read,
        query("quartz", true),
        read_limits(),
    )
    .await
    .unwrap()
    else {
        panic!("archive match")
    };
    assert_eq!(total_hits, 1);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].matched, first);
    assert_eq!(hits[0].current, second);
    assert!(
        matches!(hits[0].location,MemoryMatchLocation::Archived{revision} if revision.get()==1)
    );
    assert_eq!(generation.owner.identity.subject(), SUBJECT);
    let mut filtered = query("quartz", true);
    filtered.filter.mandatory = Some(false);
    assert!(matches!(
        NativeUserMemoryStore::search_lexical(ctx.snapshot(), &read, filtered, read_limits())
            .await
            .unwrap(),
        UserMemoryLexicalSearchRead::Published { total_hits: 0, .. }
    ));
    let UserMemoryListRead::Published { memories, .. } = NativeUserMemoryStore::list(
        ctx.snapshot(),
        &read,
        MemoryListFilter {
            tag: Some(" tag ".into()),
            mandatory: Some(true),
            ..Default::default()
        },
        read_limits(),
    )
    .await
    .unwrap() else {
        panic!("published list")
    };
    assert_eq!(memories, vec![second.clone()]);
    let deleted = NativeUserMemoryStore::delete(
        ctx.snapshot(),
        &write,
        first.id,
        DeleteMemory {
            expected_revision: 2,
            idempotency_key: "delete-search".into(),
            reason: "archive only".into(),
        },
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeUserMemoryStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("topaz", false),
            read_limits()
        )
        .await
        .unwrap(),
        UserMemoryLexicalSearchRead::Published { total_hits: 0, .. }
    ));
    let UserMemoryLexicalSearchRead::Published { hits, .. } =
        NativeUserMemoryStore::search_lexical(
            ctx.snapshot(),
            &read,
            query("topaz", true),
            read_limits(),
        )
        .await
        .unwrap()
    else {
        panic!("deleted archive")
    };
    assert!(hits.iter().all(|h| h.deleted
        && h.current == deleted
        && h.matched == deleted
        && matches!(h.location,MemoryMatchLocation::Archived{revision} if revision.get()==3)));
}

#[tokio::test]
async fn lexical_search_validates_only_candidate_histories_on_the_user_pin() {
    use super::integrity::{physical_rows, replace_text, text};
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let mut authored = input("candidate");
    authored.body = "uniquecandidatebody".into();
    let wanted = NativeUserMemoryStore::create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let unrelated =
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("unrelated"), write_limits())
            .await
            .unwrap();
    NativeUserMemoryStore::update(
        ctx.snapshot(),
        &write,
        unrelated.id,
        update("unrelated-event", 1),
        write_limits(),
    )
    .await
    .unwrap();
    let rows = physical_rows(&f).await;
    let revision = rows
        .iter()
        .find(|row| text(row, "kind") == "revision")
        .unwrap();
    let mut payload: serde_json::Value = serde_json::from_str(&text(revision, "payload")).unwrap();
    payload["before"]["body"] = "differentpastbody".into();
    let revision = replace_text(
        revision,
        "payload",
        &serde_json::to_string(&payload).unwrap(),
    );
    let uri = url::Url::from_file_path(f.user.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string();
    let store =
        graphit_data::lance_branch_store::LanceBranchStore::open(&uri, user_memory_schema(), None)
            .await
            .unwrap();
    store
        .capture_target(None)
        .await
        .unwrap()
        .prepare_merge(vec![revision], vec!["row_key".into()])
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    let mut limits = read_limits();
    limits.batch_rows = std::num::NonZeroU32::MIN;
    let UserMemoryLexicalSearchRead::Published {
        hits, total_hits, ..
    } = NativeUserMemoryStore::search_lexical(
        ctx.snapshot(),
        &read,
        query("uniquecandidatebody", false),
        limits,
    )
    .await
    .unwrap()
    else {
        panic!("candidate query")
    };
    assert_eq!(total_hits, 1);
    assert_eq!(hits[0].current, wanted);
    assert!(matches!(
        NativeUserMemoryStore::search_lexical(ctx.snapshot(), &read, query("absentcandidatebody", false), limits)
            .await.unwrap(),
        UserMemoryLexicalSearchRead::Published { total_hits: 0, hits, .. } if hits.is_empty()
    ));
    let mut filtered = query("topaz", false);
    filtered.filter.tag = Some("excluded".into());
    assert!(matches!(
        NativeUserMemoryStore::search_lexical(ctx.snapshot(), &read, filtered, limits).await,
        Err(MemoryStoreError::Invalid("USER history fork"))
    ));
}
