use super::fixture::Fixture;
use arrow_array::{Array, RecordBatch, StringArray, StructArray, UInt8Array};
use futures::TryStreamExt;
use graphit_data::{knowledge_references::*, lance_branch_store::BranchSnapshot};
use std::sync::Arc;

pub async fn rows(f: &Fixture) -> Vec<RecordBatch> {
    let uri = f
        .source
        .directory()
        .as_path()
        .join("index.lance")
        .to_str()
        .unwrap()
        .to_owned();
    let pin = BranchSnapshot::capture_observed(&uri, None, None)
        .await
        .unwrap();
    let scan = pin.scan();
    let mut stream = scan.try_into_stream().await.unwrap();
    let mut result = vec![];
    while let Some(batch) = stream.try_next().await.unwrap() {
        result.extend((0..batch.num_rows()).map(|i| batch.slice(i, 1)));
    }
    result
}
pub async fn replace(f: &Fixture, mut rows: Vec<RecordBatch>) -> u64 {
    let uri = f
        .source
        .directory()
        .as_path()
        .join("index.lance")
        .to_str()
        .unwrap()
        .to_owned();
    let pin = BranchSnapshot::capture_observed(&uri, None, None)
        .await
        .unwrap();
    if rows.is_empty() {
        rows.push(RecordBatch::new_empty(Arc::new(pin.schema().clone())));
    }
    pin.prepare_overwrite(rows)
        .await
        .unwrap()
        .commit()
        .await
        .unwrap()
        .version()
}
pub fn row<'a>(rows: &'a [RecordBatch], slug: &str) -> &'a RecordBatch {
    rows.iter()
        .find(|r| {
            r.column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .value(0)
                == slug
        })
        .unwrap()
}
/// Only isolated test rows are changed; no live Graphit store is touched.
pub fn namespace(
    batch: &RecordBatch,
    scope: KnowledgeReferenceScope<'_>,
    context: Option<&str>,
) -> RecordBatch {
    assert_eq!(batch.num_rows(), 1);
    let projection = batch
        .column(17)
        .as_any()
        .downcast_ref::<StructArray>()
        .unwrap();
    let (kind, id) = match scope {
        KnowledgeReferenceScope::Project(id) => (1u8, id.to_string()),
        KnowledgeReferenceScope::User(id) => (2u8, id.to_owned()),
    };
    let mut fields = projection.columns().to_vec();
    fields[0] = Arc::new(UInt8Array::from(vec![kind]));
    fields[1] = Arc::new(StringArray::from(vec![id]));
    fields[2] = Arc::new(StringArray::from(vec![context]));
    let mut columns = batch.columns().to_vec();
    columns[17] = Arc::new(StructArray::new(
        projection.fields().clone(),
        fields,
        projection.nulls().cloned(),
    ));
    RecordBatch::try_new(batch.schema(), columns).unwrap()
}
