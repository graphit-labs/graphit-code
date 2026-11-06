use arrow_array::{FixedSizeListArray, Float32Array, RecordBatch, StringArray, types::Float32Type};
use arrow_schema::{DataType, Field, Schema};
use graphit_core::file_io::SnapshotContext;
use graphit_data::{
    lance_branch_store::{AST_INDEX_BATCH, BranchSnapshot, cleanup_ast_corpus, col, lit},
    lance_module_store::LanceModuleStore,
};
use lance_index::scalar::FullTextSearchQuery;
use std::{collections::BTreeSet, error::Error, sync::Arc};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn rows(path: &str, text: &str, embedding: Option<[f32; 2]>) -> RecordBatch {
    let vector = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
        [embedding.map(|values| values.map(Some))],
        2,
    );
    let schema = Arc::new(Schema::new(
        ["path", "uid", "data_key", "name", "content"]
            .map(|field| Field::new(field, DataType::Utf8, false))
            .into_iter()
            .chain([Field::new(
                "vector",
                DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), 2),
                true,
            )])
            .collect::<Vec<_>>(),
    ));
    let column = |s: &str| Arc::new(StringArray::from(vec![s]));
    RecordBatch::try_new(
        schema,
        vec![
            column(path),
            column(path),
            column(path),
            column(text),
            column(text),
            Arc::new(vector),
        ],
    )
    .unwrap()
}

async fn publish(
    snapshot: &BranchSnapshot,
    path: &str,
    text: &str,
    embedding: Option<[f32; 2]>,
) -> TestResult<BranchSnapshot> {
    let next = snapshot
        .prepare_merge(vec![rows(path, text, embedding)], vec!["path".into()])
        .await?
        .commit()
        .await?;
    assert_eq!(next.version(), snapshot.version() + 1);
    Ok(next)
}

async fn fts(
    snapshot: &BranchSnapshot,
    query: &str,
    window: Option<(i64, i64)>,
) -> TestResult<Vec<String>> {
    let mut scan = snapshot.scan();
    scan.project(&["path", "_score"])?;
    let query = FullTextSearchQuery::new(query.into())
        .with_columns(&["name".into(), "content".into()])?
        .limit(window.map(|(limit, offset)| limit + offset));
    scan.full_text_search(query)?;
    if let Some((limit, offset)) = window {
        scan.limit(Some(limit), Some(offset))?;
    }
    let batch = scan.try_into_batch().await?;
    let scores = batch
        .column(1)
        .as_any()
        .downcast_ref::<Float32Array>()
        .unwrap();
    assert!(scores.values().windows(2).all(|pair| pair[0] >= pair[1]));
    Ok(batch
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap()
        .iter()
        .flatten()
        .map(str::to_owned)
        .collect())
}

async fn hits(snapshot: &BranchSnapshot, query: &str) -> TestResult<BTreeSet<String>> {
    let ordered = fts(snapshot, query, None).await?;
    let count = ordered.len();
    let unique = ordered.into_iter().collect::<BTreeSet<_>>();
    assert_eq!(
        unique.len(),
        count,
        "indexed and residual fragments must not duplicate hits"
    );
    Ok(unique)
}

async fn nearest(snapshot: &BranchSnapshot) -> TestResult<String> {
    let mut scan = snapshot.scan();
    scan.project(&["path"])?;
    scan.nearest("vector", &Float32Array::from(vec![1., 0.]), 1)?;
    let batch = scan.try_into_batch().await?;
    Ok(batch
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap()
        .value(0)
        .into())
}

#[tokio::test]
async fn progressive_files_and_native_branch_survive_batched_maintenance() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("corpus.lance");
    let original = SnapshotContext::service(Default::default());
    let schema = rows("", "", None).schema();
    LanceModuleStore::create(
        path.to_str().unwrap(),
        schema.clone(),
        vec![RecordBatch::new_empty(schema)],
    )
    .await?;
    let mut root = BranchSnapshot::capture_ast_corpus(&path, None, &original).await?;
    root = root.prepare_ast_indices().await?.unwrap().commit().await?;
    let first = publish(&root, "keep", "original_snake_case", Some([1., 0.])).await?;
    drop(root);
    assert_eq!(
        hits(&first, "original_snake_case").await?,
        BTreeSet::from(["keep".into()])
    );
    assert!(hits(&first, "snake").await?.is_empty());
    let mut child = BranchSnapshot::capture_ast_corpus(&path, Some("feature"), &original)
        .await?
        .materialize()
        .await?;
    child = publish(&child, "child", "childonly", None).await?;
    let mut current = publish(&first, "keep", "newroot", Some([1., 0.])).await?;
    for number in 0..AST_INDEX_BATCH {
        current = publish(&current, &format!("file{number}"), "commonword", None).await?;
    }
    current = current
        .prepare_delete_expr(col("path").eq(lit("file0")))
        .await?
        .commit()
        .await?;
    let expected = hits(&current, "commonword").await?;
    assert_eq!(expected.len(), AST_INDEX_BATCH - 1);
    assert!(!expected.contains("file0"));
    assert_eq!(nearest(&current).await?, "keep");
    assert!(hits(&current, "childonly").await?.is_empty());
    assert!(hits(&current, "original_snake_case").await?.is_empty());
    let before_head = current.version();
    assert!(current.maintain_ast_indices(&original, &|| Ok(())).await?);
    let latest = BranchSnapshot::capture_ast_corpus(&path, None, &original).await?;
    assert!(
        latest.version() > before_head,
        "maintenance publishes a newer snapshot"
    );
    assert_eq!(hits(&latest, "commonword").await?, expected);
    assert_eq!(nearest(&latest).await?, "keep");
    let mixed = publish(&latest, "new", "commonword incoming", None).await?;
    let expected_root_head = mixed.version();
    let expected_child_head = child.version();
    assert_eq!(hits(&mixed, "commonword").await?.len(), expected.len() + 1);
    assert_eq!(
        hits(&mixed, "incoming").await?,
        BTreeSet::from(["new".into()])
    );
    assert_eq!(fts(&mixed, "commonword", Some((3, 0))).await?.len(), 3);
    assert_eq!(fts(&mixed, "commonword", Some((3, 3))).await?.len(), 3);
    drop(mixed);
    assert_eq!(
        hits(&first, "original_snake_case").await?,
        BTreeSet::from(["keep".into()])
    );
    assert!(cleanup_ast_corpus(&path, &original).await?.is_none());

    drop((first, current, child, latest));
    let report = cleanup_ast_corpus(&path, &original).await?.unwrap();
    let (root_before, root_after) = report
        .iter()
        .find(|(before, _)| before.branch.is_none())
        .unwrap();
    assert!(root_before.manifests > AST_INDEX_BATCH as u64);
    assert!(
        root_after.manifests >= 1 && root_after.manifests < root_before.manifests,
        "cleanup trims obsolete versions while retaining the current head"
    );
    assert_eq!(report.len(), 2);
    for (before, after) in &report {
        assert_eq!(before.branch, after.branch);
        assert_eq!(before.head, after.head);
        assert_eq!(before.rows, after.rows);
    }
    assert!(root_after.fragments < AST_INDEX_BATCH / 2);
    assert!(root_after.index_segments >= 5);

    let root = BranchSnapshot::capture_ast_corpus(&path, None, &original).await?;
    let child = BranchSnapshot::capture_ast_corpus(&path, Some("feature"), &original).await?;
    assert_eq!(root.version(), expected_root_head);
    assert_eq!(child.version(), expected_child_head);
    assert_eq!(child.physical_branch(), Some("feature"));
    let mut expected = expected;
    expected.insert("new".into());
    assert_eq!(hits(&root, "commonword").await?, expected);
    assert_eq!(
        hits(&child, "original_snake_case").await?,
        BTreeSet::from(["keep".into()])
    );
    assert_eq!(
        hits(&child, "childonly").await?,
        BTreeSet::from(["child".into()])
    );
    assert!(hits(&child, "newroot").await?.is_empty());
    assert_eq!(nearest(&root).await?, "keep");
    assert_eq!(nearest(&child).await?, "keep");
    Ok(())
}
