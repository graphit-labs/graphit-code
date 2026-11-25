use arrow_array::RecordBatch;
use arrow_schema::Schema;
use graphit_data::lance_module_store::{RecordBatchStreamAdapter, SendableRecordBatchStream};
use std::sync::Arc;

// Finite test images declare their actual first-batch schema; empty input retains
// the explicitly supplied schema so EOF validation still reaches the real SDK.
pub fn batches(batches: Vec<RecordBatch>, empty_schema: Arc<Schema>) -> SendableRecordBatchStream {
    let schema = batches
        .first()
        .map(RecordBatch::schema)
        .unwrap_or(empty_schema);
    Box::pin(RecordBatchStreamAdapter::new(
        schema,
        futures::stream::iter(batches.into_iter().map(Ok)),
    ))
}
