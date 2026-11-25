use super::*;
use arrow_array::{RecordBatch, StringArray};
use futures::TryStreamExt;
use graphit_data::{
    lance_branch_store::{BranchSnapshot, LanceBranchStore},
    lance_module_store::LanceModuleStore,
};
pub fn uri(f: &Fixture) -> String {
    url::Url::from_file_path(f.user.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string()
}
pub async fn pin(f: &Fixture) -> BranchSnapshot {
    LanceBranchStore::open(&uri(f), user_memory_schema(), None)
        .await
        .unwrap()
        .capture_target(None)
        .await
        .unwrap()
}
pub async fn rows(f: &Fixture) -> Vec<RecordBatch> {
    let p = pin(f).await;
    let mut scan = p.scan();
    scan.batch_size(1);
    scan.try_into_stream()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap()
}
pub fn text(b: &RecordBatch, name: &str) -> String {
    b.column_by_name(name)
        .unwrap()
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap()
        .value(0)
        .into()
}
pub fn text_column(b: &RecordBatch, name: &str, value: &str) -> RecordBatch {
    let mut columns = b.columns().to_vec();
    columns[b.schema().index_of(name).unwrap()] = Arc::new(StringArray::from(vec![value]));
    RecordBatch::try_new(b.schema(), columns).unwrap()
}
pub async fn replace(f: &Fixture, rows: Vec<RecordBatch>) {
    LanceModuleStore::open(&uri(f), user_memory_schema())
        .await
        .unwrap()
        .snapshot()
        .prepare_replace(rows)
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
}
pub async fn empty(f: &Fixture, ctx: &graphit_hub::policy::RequestContext<'_>) {
    ctx.snapshot().check().unwrap();
    std::fs::create_dir_all(f.user.runtime_root().join("memory")).unwrap();
    let schema = user_memory_schema();
    LanceModuleStore::create(
        &uri(f),
        Arc::clone(&schema),
        vec![RecordBatch::new_empty(schema)],
    )
    .await
    .unwrap();
}
