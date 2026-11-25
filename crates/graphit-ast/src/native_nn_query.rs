//! Native nearest queries on an already captured NN branch.
//!
//! Rows and generations are observations. Acquisition, authorization, model
//! binding and raw-source freshness remain with the actual caller.

use arrow_array::{Array, Float32Array, RecordBatch, StringArray};
use arrow_schema::ArrowError;
use futures::TryStreamExt;
use graphit_core::file_io::SnapshotContext;
use graphit_data::lance_branch_store::BranchSnapshot;
use std::{
    collections::{HashSet, TryReserveError},
    error::Error,
    fmt, io,
    sync::Arc,
};

use crate::native_nn::{NativeNnError, NativeNnModel};

#[derive(Clone, Copy, Debug)]
pub enum NativeNnMetric {
    L2,
    Cosine,
    Dot,
}

impl NativeNnMetric {
    fn sdk_name(self) -> &'static str {
        match self {
            Self::L2 => "l2",
            Self::Cosine => "cosine",
            Self::Dot => "dot",
        }
    }
}

#[derive(Debug)]
pub enum NativeNnQueryError {
    Operation(io::Error),
    Allocation(TryReserveError),
    Payload(NativeNnError),
    Arrow(ArrowError),
    Sdk(Box<lance::Error>),
    RequestMismatch,
    InvalidQuery,
    InvalidDistance,
    InvalidResult,
}

impl fmt::Display for NativeNnQueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(error) => write!(f, "native NN query operation: {error}"),
            Self::Allocation(error) => write!(f, "native NN query allocation: {error}"),
            Self::Payload(error) => write!(f, "native NN query payload: {error}"),
            Self::Arrow(error) => write!(f, "native NN query Arrow: {error}"),
            Self::Sdk(error) => write!(f, "native NN query SDK: {error}"),
            Self::RequestMismatch => f.write_str("native NN query request mismatch"),
            Self::InvalidQuery => f.write_str("invalid native NN query"),
            Self::InvalidDistance => f.write_str("invalid native NN query distance"),
            Self::InvalidResult => f.write_str("invalid native NN query result"),
        }
    }
}

impl Error for NativeNnQueryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(error) => Some(error),
            Self::Allocation(error) => Some(error),
            Self::Payload(error) => Some(error),
            Self::Arrow(error) => Some(error),
            Self::Sdk(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

/// Owned query rows and scores retain only their operation cancellation context.
pub struct NativeNnMatches {
    original: Arc<SnapshotContext>,
    rows: Vec<RecordBatch>,
    distances: Vec<Vec<f32>>,
}

impl NativeNnMatches {
    fn revalidate(&self, original: &Arc<SnapshotContext>) -> Result<(), NativeNnQueryError> {
        if !Arc::ptr_eq(&self.original, original) {
            return Err(NativeNnQueryError::RequestMismatch);
        }
        original.check().map_err(NativeNnQueryError::Operation)
    }

    /// Full validated payload batches in SDK order, with no distance column.
    pub fn rows(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> Result<&[RecordBatch], NativeNnQueryError> {
        self.revalidate(original)?;
        Ok(&self.rows)
    }

    /// Scores correspond exactly to each payload batch and its row order.
    pub fn distances(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> Result<&[Vec<f32>], NativeNnQueryError> {
        self.revalidate(original)?;
        Ok(&self.distances)
    }
}

fn sdk(error: lance::Error) -> NativeNnQueryError {
    NativeNnQueryError::Sdk(Box::new(error))
}

/// Native KNN over the same nullable-vector corpus used by lexical search.
/// Model identity and null-vector filtering are applied before nearest selection.
pub async fn query_corpus_nn(
    snapshot: &BranchSnapshot,
    model: &NativeNnModel<'_>,
    query: &[f32],
    k: usize,
    metric: NativeNnMetric,
    original: &Arc<SnapshotContext>,
) -> Result<NativeNnMatches, NativeNnQueryError> {
    original.check().map_err(NativeNnQueryError::Operation)?;
    crate::corpus::validate_schema(snapshot.schema()).map_err(NativeNnQueryError::Operation)?;
    if k == 0 || query.len() as u64 != model.dimensions || query.iter().any(|v| !v.is_finite()) {
        return Err(NativeNnQueryError::InvalidQuery);
    }
    let expected = crate::corpus::vector_fields(model).map_err(NativeNnQueryError::Operation)?;
    if snapshot
        .schema()
        .field_with_name(crate::corpus::VECTOR)
        .ok()
        != Some(expected.field(0))
    {
        return Err(NativeNnQueryError::Payload(NativeNnError::SchemaMismatch));
    }
    let mut scanner = snapshot.scan();
    scanner.filter_expr(crate::corpus::vector_filter(model));
    scanner.prefilter(true);
    // The maintained native index uses cosine. Other public metrics retain
    // their exact SDK distance semantics through the native flat scanner.
    scanner.use_index(matches!(metric, NativeNnMetric::Cosine));
    scanner
        .nearest("vector", &Float32Array::from(query.to_vec()), k)
        .map_err(sdk)?;
    scanner.distance_metric(
        metric
            .sdk_name()
            .try_into()
            .map_err(NativeNnQueryError::Arrow)?,
    );
    let mut stream = scanner.try_into_stream().await.map_err(sdk)?;
    let mut rows = Vec::new();
    let mut distances = Vec::new();
    let mut count = 0usize;
    let mut identities = HashSet::new();
    while let Some(batch) = stream.try_next().await.map_err(sdk)? {
        original.check().map_err(NativeNnQueryError::Operation)?;
        let scores = batch
            .column_by_name("_distance")
            .and_then(|a| a.as_any().downcast_ref::<Float32Array>())
            .ok_or(NativeNnQueryError::InvalidDistance)?;
        if scores.null_count() != 0 || scores.values().iter().any(|v| !v.is_finite()) {
            return Err(NativeNnQueryError::InvalidDistance);
        }
        let indices = batch
            .schema()
            .fields()
            .iter()
            .enumerate()
            .filter_map(|(i, f)| (f.name() != "_distance").then_some(i))
            .collect::<Vec<_>>();
        let payload = batch.project(&indices).map_err(NativeNnQueryError::Arrow)?;
        let payload = crate::corpus::vectors(&payload, model, original)
            .map_err(NativeNnQueryError::Operation)?;
        if payload.num_rows() != batch.num_rows() {
            return Err(NativeNnQueryError::InvalidResult);
        }
        let uid = payload
            .column_by_name("uid")
            .and_then(|a| a.as_any().downcast_ref::<StringArray>())
            .ok_or(NativeNnQueryError::InvalidResult)?;
        let key = payload
            .column_by_name("data_key")
            .and_then(|a| a.as_any().downcast_ref::<StringArray>())
            .ok_or(NativeNnQueryError::InvalidResult)?;
        for row in 0..payload.num_rows() {
            identities
                .try_reserve(1)
                .map_err(NativeNnQueryError::Allocation)?;
            if !identities.insert((uid.value(row).to_owned(), key.value(row).to_owned())) {
                return Err(NativeNnQueryError::Payload(
                    NativeNnError::DuplicateIdentity,
                ));
            }
        }

        count = count
            .checked_add(payload.num_rows())
            .ok_or(NativeNnQueryError::InvalidResult)?;
        if count > k {
            return Err(NativeNnQueryError::InvalidResult);
        }
        distances.push(scores.values().to_vec());
        rows.push(payload);
    }
    original.check().map_err(NativeNnQueryError::Operation)?;
    Ok(NativeNnMatches {
        original: original.clone(),
        rows,
        distances,
    })
}
