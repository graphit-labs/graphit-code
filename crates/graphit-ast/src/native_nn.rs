//! Arrow representation of native parser entities and caller-observed embeddings.
//!
//! Metadata and rows are observations. They do not grant authorization, freshness,
//! model bindings, or native entity provenance to a deserialized record.

use std::{
    collections::{HashMap, HashSet, TryReserveError},
    error::Error,
    fmt, io,
    sync::Arc,
};

use arrow_array::{
    Array, ArrayRef, FixedSizeBinaryArray, FixedSizeListArray, Float32Array, ListArray,
    RecordBatch, StringArray, StructArray, UInt64Array,
    builder::{
        ArrayBuilder, FixedSizeBinaryBuilder, FixedSizeListBuilder, Float32Builder, ListBuilder,
        StringBuilder, StructBuilder, UInt64Builder,
    },
};
use arrow_schema::{ArrowError, DataType, Field, Fields, Schema};
use graphit_core::file_io::SnapshotContext;
use sha2::{Digest, Sha256};

use crate::{
    RangeError, SourceRange, SourceText,
    antlr_engine::model::{AntlrModelError, AntlrSemanticDocument},
    entity_extent::EntityExtent,
    indexed_source::{IndexedSourceError, IndexedSourceSnapshot, MissingSourceReason},
    native_nn_physical_source::{NativeNnPhysicalSource, NativeNnPhysicalSourceError},
    semantic_fields::ExtentObservation,
    semantic_model::{JsonProjection, JsonSemanticDocument, SemanticModelError},
    source_inspection::IndexedSource,
    tree_sitter_generic_model::{GenericModelError, GenericSemanticDocument},
    tree_sitter_model::{TreeSitterModelError, TreeSitterSemanticDocument},
};

#[path = "native_nn_antlr.rs"]
mod antlr;
#[path = "native_nn_antlr_generic.rs"]
mod antlr_generic;
#[path = "native_nn_generic.rs"]
mod generic;
#[path = "native_nn_scip.rs"]
mod scip;
#[path = "native_nn_tree_sitter.rs"]
mod tree_sitter;
pub use scip::prepare_native_nn_scip;

pub use antlr_generic::{
    NativeNnAntlrGenericProjection, NativeNnAntlrGenericProjectionIssue,
    projections as native_nn_antlr_generic_projections,
};

/// Native index schema; incompatible generations are rebuilt by normal publication.
pub const NATIVE_NN_FORMAT: &str = "ast-native-nn-4";

/// Exact original parser bytes, without a model SDK prefix.
pub const NATIVE_ENTITY_PROJECTION_RECIPE: &str = "ast-parser-entity-original-1";

/// Identity observed from one resolved model slot, not a binding or permission.
pub struct NativeNnModel<'a> {
    pub vector_id: [u8; 32],
    pub dimensions: u64,
    pub model_id: &'a str,
    pub revision: &'a str,
}

/// Source allocation and the retained raw storage observation for one input.
pub enum NativeNnSource<'a> {
    Indexed(&'a IndexedSourceSnapshot),
    Physical {
        capture: &'a NativeNnPhysicalSource<'a>,
        raw: &'a IndexedSourceSnapshot,
    },
}

/// Selection from a retained genuine JSON semantic owner, in vector order.
pub struct NativeNnJsonInput<'a, 'doc, 'profile, 'source, 'prepared, 'request> {
    pub document: &'a JsonSemanticDocument<'doc, 'profile, 'source, 'prepared, 'request>,
    pub source: NativeNnSource<'a>,
    pub entity_indices: &'a [usize],
    pub vectors: &'a [Vec<f32>],
    /// Observed local model generation, absent for remote embeddings.
    pub model_generation: Option<u64>,
}

/// Selection from a retained genuine Oracle or DB2 semantic owner, in vector order.
pub struct NativeNnAntlrInput<'a, 'doc, 'profile, 'source, 'prepared, 'request> {
    pub document: &'a AntlrSemanticDocument<'doc, 'profile, 'source, 'prepared, 'request>,
    pub source: NativeNnSource<'a>,
    pub entity_indices: &'a [usize],
    pub vectors: &'a [Vec<f32>],
    /// Observed local model generation, absent for remote embeddings.
    pub model_generation: Option<u64>,
}

/// Selection from the same retained PostgreSQL, TSQL or COBOL parser owner.
pub struct NativeNnAntlrGenericInput<'a, 'profile: 'source, 'source, 'prepared, 'request> {
    pub document: &'a crate::antlr_engine::generic_model::ParsedGenericAntlrDocument<
        'profile,
        'source,
        'prepared,
        'request,
    >,
    pub source: NativeNnSource<'a>,
    pub entity_indices: &'a [usize],
    pub vectors: &'a [Vec<f32>],
    /// Observed local model generation, absent for remote embeddings.
    pub model_generation: Option<u64>,
}

/// Selection from a retained genuine Tree-sitter semantic owner, in vector order.
pub struct NativeNnTsInput<'a, 'doc, 'profile, 'source, 'prepared, 'request> {
    pub document: &'a TreeSitterSemanticDocument<'doc, 'profile, 'source, 'prepared, 'request>,
    pub source: NativeNnSource<'a>,
    pub entity_indices: &'a [usize],
    pub vectors: &'a [Vec<f32>],
    /// Observed local model generation, absent for remote embeddings.
    pub model_generation: Option<u64>,
}

/// Selection from a retained generic native model, in vector order.
pub struct NativeNnGenericInput<'a, 'doc, 'profile, 'source, 'prepared, 'request> {
    pub document: &'a GenericSemanticDocument<'doc, 'profile, 'source, 'prepared, 'request>,
    pub source: NativeNnSource<'a>,
    pub entity_indices: &'a [usize],
    pub vectors: &'a [Vec<f32>],
    /// Observed local model generation, absent for remote embeddings.
    pub model_generation: Option<u64>,
}

#[derive(Debug)]
pub enum NativeNnError {
    Operation(io::Error),
    Source(IndexedSourceError),
    PhysicalSource(Box<NativeNnPhysicalSourceError>),
    Arrow(ArrowError),
    Range(RangeError),
    Extent(Box<crate::entity_extent::EntityExtentError>),
    Semantic(Box<SemanticModelError>),
    Antlr(Box<AntlrModelError>),
    TreeSitter(Box<TreeSitterModelError>),
    Generic(Box<GenericModelError>),
    Allocation(TryReserveError),
    InvalidModel,
    SchemaMismatch,
    InvalidInput,
    InvalidRow,
    InvalidVector,
    DuplicateIdentity,
    Representation,
    RequestMismatch,
}

impl fmt::Display for NativeNnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(e) => write!(f, "native NN operation: {e}"),
            Self::Source(e) => write!(f, "native NN raw source: {e}"),
            Self::PhysicalSource(e) => write!(f, "native NN physical source: {e}"),
            Self::Arrow(e) => write!(f, "native NN Arrow: {e}"),
            Self::Semantic(e) => write!(f, "native NN semantic model: {e}"),
            Self::Antlr(e) => write!(f, "native NN ANTLR model: {e}"),
            Self::TreeSitter(e) => write!(f, "native NN Tree-sitter model: {e}"),
            Self::Generic(e) => write!(f, "native NN generic model: {e}"),
            Self::Range(e) => write!(f, "native NN range: {e}"),
            Self::Extent(e) => write!(f, "native NN extent: {e}"),
            Self::Allocation(e) => write!(f, "native NN allocation: {e}"),
            Self::InvalidModel => f.write_str("invalid observed native NN model"),
            Self::SchemaMismatch => f.write_str("native NN schema or model identity mismatch"),
            Self::InvalidInput => f.write_str("invalid native NN input association"),
            Self::InvalidRow => f.write_str("invalid persisted native NN row"),
            Self::InvalidVector => f.write_str("invalid observed native NN vector"),
            Self::DuplicateIdentity => f.write_str("duplicate native NN identity"),
            Self::Representation => f.write_str("native NN Arrow representation overflow"),
            Self::RequestMismatch => f.write_str("native NN Original owner mismatch"),
        }
    }
}

impl Error for NativeNnError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(e) => Some(e),
            Self::Source(e) => Some(e),
            Self::PhysicalSource(e) => Some(e.as_ref()),
            Self::Arrow(e) => Some(e),
            Self::Range(e) => Some(e),
            Self::Extent(e) => Some(e.as_ref()),
            Self::Semantic(e) => Some(e.as_ref()),
            Self::Antlr(e) => Some(e.as_ref()),
            Self::TreeSitter(e) => Some(e.as_ref()),
            Self::Generic(e) => Some(e.as_ref()),
            Self::Allocation(e) => Some(e),
            _ => None,
        }
    }
}

fn check(original: &SnapshotContext) -> Result<(), NativeNnError> {
    original.check().map_err(NativeNnError::Operation)
}

fn text_valid(value: &str) -> bool {
    !value.is_empty() && !value.contains('\0')
}

fn width(model: &NativeNnModel<'_>) -> Result<i32, NativeNnError> {
    if !text_valid(model.model_id) || !text_valid(model.revision) || model.dimensions == 0 {
        return Err(NativeNnError::InvalidModel);
    }
    i32::try_from(model.dimensions).map_err(|_| NativeNnError::Representation)
}

fn range_fields() -> Fields {
    [
        "start",
        "end",
        "start_row",
        "start_column",
        "end_row",
        "end_column",
    ]
    .map(|name| Arc::new(Field::new(name, DataType::UInt64, false)))
    .into()
}

fn extent_fields() -> Fields {
    vec![
        Field::new("range", DataType::Struct(range_fields()), false),
        Field::new("start_line", DataType::UInt64, false),
        Field::new("end_line", DataType::UInt64, false),
    ]
    .into()
}

fn owner_fields() -> Fields {
    vec![
        Field::new("kind", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("name_anchor", DataType::Struct(range_fields()), false),
        Field::new("extent", DataType::Struct(extent_fields()), false),
        Field::new("name_kind", DataType::Utf8, false),
        Field::new("raw_name", DataType::Utf8, false),
    ]
    .into()
}

/// Build the complete model identity even when no rows are present.
pub fn native_nn_schema(model: &NativeNnModel<'_>) -> Result<Arc<Schema>, NativeNnError> {
    let dimensions = width(model)?;
    let mut vector = String::new();
    vector
        .try_reserve_exact(64)
        .map_err(NativeNnError::Allocation)?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in model.vector_id {
        vector.push(char::from(HEX[usize::from(byte >> 4)]));
        vector.push(char::from(HEX[usize::from(byte & 15)]));
    }
    let metadata: HashMap<String, String> = [
        ("graphit.module", "ast".to_owned()),
        ("graphit.ast.nn.format", NATIVE_NN_FORMAT.to_owned()),
        ("graphit.ast.nn.vector_id", vector),
        ("graphit.ast.nn.dimensions", model.dimensions.to_string()),
        ("graphit.ast.nn.model_id", model.model_id.to_owned()),
        ("graphit.ast.nn.revision", model.revision.to_owned()),
        ("graphit.ast.nn.role", "CodeDocument".to_owned()),
        (
            "graphit.ast.nn.projection_recipe",
            NATIVE_ENTITY_PROJECTION_RECIPE.to_owned(),
        ),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value))
    .collect();
    Ok(Arc::new(Schema::new_with_metadata(
        nn_fields(Some(dimensions)),
        metadata,
    )))
}

fn nn_fields(dimensions: Option<i32>) -> Vec<Field> {
    let mut fields = vec![
        Field::new("uid", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("kind", DataType::Utf8, false),
        Field::new("path", DataType::Utf8, false),
        Field::new(
            "path_components",
            DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
            false,
        ),
        Field::new("language", DataType::Utf8, false),
        Field::new("grammar", DataType::Utf8, false),
        Field::new("name_anchor", DataType::Struct(range_fields()), false),
        Field::new("extent", DataType::Struct(extent_fields()), false),
        Field::new(
            "namespace",
            DataType::List(Arc::new(Field::new(
                "item",
                DataType::Struct(owner_fields()),
                true,
            ))),
            false,
        ),
        Field::new("raw_version", DataType::UInt64, false),
        Field::new("raw_origin", DataType::Utf8, false),
        Field::new("raw_physical_branch", DataType::Utf8, true),
        Field::new("raw_inherited_target", DataType::Utf8, true),
        Field::new("full_source_sha256", DataType::FixedSizeBinary(32), false),
        Field::new("projection", DataType::Utf8, false),
        Field::new("projection_sha256", DataType::FixedSizeBinary(32), false),
        Field::new("data_key", DataType::Utf8, false),
        Field::new("name_kind", DataType::Utf8, false),
    ];
    if let Some(dimensions) = dimensions {
        fields.insert(17, Field::new("model_generation", DataType::UInt64, true));
        fields.insert(
            18,
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    dimensions,
                ),
                false,
            ),
        );
    }
    fields
}

/// Persistable parser projections with their recipe identity, before
/// inference. No vector or generation column exists until real inference succeeds.
/// These observations do not restore Source provenance or authorize a later job.
pub fn native_nn_projection_schema() -> Arc<Schema> {
    let metadata = [
        ("graphit.module", "ast"),
        ("graphit.ast.nn.format", NATIVE_NN_FORMAT),
        ("graphit.ast.nn.role", "CodeDocument"),
        (
            "graphit.ast.nn.projection_recipe",
            NATIVE_ENTITY_PROJECTION_RECIPE,
        ),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value.to_owned()))
    .collect();
    Arc::new(Schema::new_with_metadata(nn_fields(None), metadata))
}

/// Attach actual inference output in projection row order. Arrow metadata buffers
/// are shared; parser owners and source text allocations are no longer required.
/// The caller still admits the current FILE/source SHA before publishing results.
pub fn complete_native_nn(
    prepared: &RecordBatch,
    model: &NativeNnModel<'_>,
    model_generation: Option<u64>,
    vectors: &[Vec<f32>],
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    complete_rows(
        prepared,
        model,
        vectors
            .iter()
            .map(|vector| (model_generation, vector.as_slice())),
        original,
    )
}

fn complete_rows<'a>(
    prepared: &RecordBatch,
    model: &NativeNnModel<'_>,
    vectors: impl IntoIterator<Item = (Option<u64>, &'a [f32])>,
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    check(original)?;
    if prepared.schema().as_ref() != native_nn_projection_schema().as_ref() {
        return Err(NativeNnError::SchemaMismatch);
    }
    let dimensions = width(model)?;
    let mut values = FixedSizeListBuilder::new(Float32Builder::new(), dimensions);
    let mut generations = UInt64Builder::new();
    for (generation, vector) in vectors {
        check(original)?;
        if generations.len() >= prepared.num_rows() || vector.len() != dimensions as usize {
            return Err(NativeNnError::InvalidVector);
        }
        finite(vector, original)?;
        generations.append_option(generation);
        for chunk in vector.chunks(16_384) {
            check(original)?;
            values.values().append_slice(chunk);
        }
        values.append(true);
    }
    if generations.len() != prepared.num_rows() {
        return Err(NativeNnError::InvalidVector);
    }
    let generation: ArrayRef = Arc::new(generations.finish());
    let vector: ArrayRef = Arc::new(values.finish());
    let schema = native_nn_schema(model)?;
    let columns = schema
        .fields()
        .iter()
        .map(|field| match field.name().as_str() {
            "model_generation" => Ok(generation.clone()),
            "vector" => Ok(vector.clone()),
            name => prepared
                .column_by_name(name)
                .cloned()
                .ok_or(NativeNnError::SchemaMismatch),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let batch = RecordBatch::try_new(schema, columns).map_err(NativeNnError::Arrow)?;
    validate_native_nn(&batch, model, original)?;
    Ok(batch)
}

fn finish_prepared(
    result: Result<RecordBatch, NativeNnError>,
    source: &NativeNnSource<'_>,
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    let revalidation = match source {
        NativeNnSource::Indexed(_) => Ok(()),
        NativeNnSource::Physical { capture, .. } => capture
            .revalidate(original)
            .map_err(|error| NativeNnError::PhysicalSource(Box::new(error))),
    };
    check(original)?;
    revalidation?;
    let batch = result?;
    validate_native_nn_projection(&batch, original)?;
    Ok(batch)
}

/// Prepare one FILE's genuine ts projections without inference.
pub fn prepare_native_nn_ts(
    document: &TreeSitterSemanticDocument<'_, '_, '_, '_, '_>,
    source: NativeNnSource<'_>,
    entity_indices: &[usize],
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    check(original)?;
    let input = NativeNnTsInput {
        document,
        source,
        entity_indices,
        vectors: &[],
        model_generation: None,
    };
    let result = tree_sitter::encode(std::slice::from_ref(&input), original);
    finish_prepared(result, &input.source, original)
}

/// Prepare one FILE's genuine json projections without inference.
pub fn prepare_native_nn_json(
    document: &JsonSemanticDocument<'_, '_, '_, '_, '_>,
    source: NativeNnSource<'_>,
    entity_indices: &[usize],
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    check(original)?;
    let input = NativeNnJsonInput {
        document,
        source,
        entity_indices,
        vectors: &[],
        model_generation: None,
    };
    let result = encode_json(std::slice::from_ref(&input), original);
    finish_prepared(result, &input.source, original)
}

/// Prepare one FILE's genuine antlr projections without inference.
pub fn prepare_native_nn_antlr(
    document: &AntlrSemanticDocument<'_, '_, '_, '_, '_>,
    source: NativeNnSource<'_>,
    entity_indices: &[usize],
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    check(original)?;
    let input = NativeNnAntlrInput {
        document,
        source,
        entity_indices,
        vectors: &[],
        model_generation: None,
    };
    let result = antlr::encode(std::slice::from_ref(&input), original);
    finish_prepared(result, &input.source, original)
}

/// Prepare one FILE's genuine antlr generic projections without inference.
pub fn prepare_native_nn_antlr_generic(
    document: &crate::antlr_engine::generic_model::ParsedGenericAntlrDocument<'_, '_, '_, '_>,
    source: NativeNnSource<'_>,
    entity_indices: &[usize],
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    check(original)?;
    let input = NativeNnAntlrGenericInput {
        document,
        source,
        entity_indices,
        vectors: &[],
        model_generation: None,
    };
    let result = antlr_generic::encode(std::slice::from_ref(&input), original);
    finish_prepared(result, &input.source, original)
}

/// Prepare one FILE's genuine generic projections without inference.
pub fn prepare_native_nn_generic(
    document: &GenericSemanticDocument<'_, '_, '_, '_, '_>,
    source: NativeNnSource<'_>,
    entity_indices: &[usize],
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    check(original)?;
    let input = NativeNnGenericInput {
        document,
        source,
        entity_indices,
        vectors: &[],
        model_generation: None,
    };
    let result = generic::encode(std::slice::from_ref(&input), original);
    finish_prepared(result, &input.source, original)
}

fn range_builder() -> StructBuilder {
    StructBuilder::new(
        range_fields(),
        (0..6)
            .map(|_| Box::new(UInt64Builder::new()) as Box<dyn ArrayBuilder>)
            .collect(),
    )
}

fn extent_builder() -> StructBuilder {
    StructBuilder::new(
        extent_fields(),
        vec![
            Box::new(range_builder()),
            Box::new(UInt64Builder::new()),
            Box::new(UInt64Builder::new()),
        ],
    )
}

fn owner_builder() -> StructBuilder {
    StructBuilder::new(
        owner_fields(),
        vec![
            Box::new(StringBuilder::new()),
            Box::new(StringBuilder::new()),
            Box::new(range_builder()),
            Box::new(extent_builder()),
            Box::new(StringBuilder::new()),
            Box::new(StringBuilder::new()),
        ],
    )
}

fn builder<T: ArrayBuilder>(b: &mut StructBuilder, i: usize) -> Result<&mut T, NativeNnError> {
    b.field_builder(i).ok_or(NativeNnError::SchemaMismatch)
}

fn push_text(b: &mut StringBuilder, value: &str) -> Result<(), NativeNnError> {
    let end = b
        .values_slice()
        .len()
        .checked_add(value.len())
        .ok_or(NativeNnError::Representation)?;
    i32::try_from(end).map_err(|_| NativeNnError::Representation)?;
    b.append_value(value);
    Ok(())
}

fn push_optional(b: &mut StringBuilder, value: Option<&str>) -> Result<(), NativeNnError> {
    if let Some(value) = value {
        push_text(b, value)
    } else {
        b.append_null();
        Ok(())
    }
}

fn push_range(b: &mut StructBuilder, range: SourceRange<'_>) -> Result<(), NativeNnError> {
    let start = range.start_point();
    let end = range.end_point();
    for (i, value) in [
        range.start(),
        range.end(),
        start.row,
        start.column,
        end.row,
        end.column,
    ]
    .into_iter()
    .enumerate()
    {
        builder::<UInt64Builder>(b, i)?
            .append_value(u64::try_from(value).map_err(|_| NativeNnError::Representation)?);
    }
    b.append(true);
    Ok(())
}

fn push_extent(b: &mut StructBuilder, extent: &EntityExtent<'_>) -> Result<(), NativeNnError> {
    push_range(builder::<StructBuilder>(b, 0)?, extent.range())?;
    builder::<UInt64Builder>(b, 1)?.append_value(
        u64::try_from(extent.start_line()).map_err(|_| NativeNnError::Representation)?,
    );
    builder::<UInt64Builder>(b, 2)?
        .append_value(u64::try_from(extent.end_line()).map_err(|_| NativeNnError::Representation)?);
    b.append(true);
    Ok(())
}

// Shared Arrow storage for genuine parser producers.
struct NnColumns {
    strings: [StringBuilder; 10],
    paths: ListBuilder<StringBuilder>,
    names: StructBuilder,
    extents: StructBuilder,
    owners: ListBuilder<StructBuilder>,
    raw_versions: UInt64Builder,
    source_sha: FixedSizeBinaryBuilder,
    projection_sha: FixedSizeBinaryBuilder,
    data_keys: StringBuilder,
    name_kinds: StringBuilder,
}
impl NnColumns {
    fn new() -> Self {
        let strings: [StringBuilder; 10] = std::array::from_fn(|_| StringBuilder::new());
        let paths = ListBuilder::new(StringBuilder::new());
        let names = range_builder();
        let extents = extent_builder();
        let owners = ListBuilder::new(owner_builder());
        let raw_versions = UInt64Builder::new();
        let source_sha = FixedSizeBinaryBuilder::new(32);
        let projection_sha = FixedSizeBinaryBuilder::new(32);
        let data_keys = StringBuilder::new();
        let name_kinds = StringBuilder::new();
        Self {
            strings,
            paths,
            names,
            extents,
            owners,
            raw_versions,
            source_sha,
            projection_sha,
            data_keys,
            name_kinds,
        }
    }
    fn push_projection(
        &mut self,
        text: &str,
        original: &SnapshotContext,
    ) -> Result<(), NativeNnError> {
        self.projection_sha
            .append_value(digest(text.as_bytes(), original)?)
            .map_err(NativeNnError::Arrow)
    }
    fn finish(&mut self, schema: Arc<Schema>) -> Result<RecordBatch, NativeNnError> {
        let Self {
            strings,
            paths,
            names,
            extents,
            owners,
            raw_versions,
            source_sha,
            projection_sha,
            data_keys,
            name_kinds,
        } = self;
        let columns: Vec<ArrayRef> = vec![
            Arc::new(strings[0].finish()),
            Arc::new(strings[1].finish()),
            Arc::new(strings[2].finish()),
            Arc::new(strings[3].finish()),
            Arc::new(paths.finish()),
            Arc::new(strings[4].finish()),
            Arc::new(strings[5].finish()),
            Arc::new(names.finish()),
            Arc::new(extents.finish()),
            Arc::new(owners.finish()),
            Arc::new(raw_versions.finish()),
            Arc::new(strings[6].finish()),
            Arc::new(strings[8].finish()),
            Arc::new(strings[9].finish()),
            Arc::new(source_sha.finish()),
            Arc::new(strings[7].finish()),
            Arc::new(projection_sha.finish()),
            Arc::new(data_keys.finish()),
            Arc::new(name_kinds.finish()),
        ];
        RecordBatch::try_new(schema, columns).map_err(NativeNnError::Arrow)
    }
}

/// Encode only Available projections belonging to the supplied Oracle or DB2 model.
pub fn encode_native_nn_antlr(
    model: &NativeNnModel<'_>,
    inputs: &[NativeNnAntlrInput<'_, '_, '_, '_, '_, '_>],
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    check(original)?;
    let result = (|| {
        width(model)?;
        if inputs
            .iter()
            .any(|input| input.vectors.len() != input.entity_indices.len())
        {
            return Err(NativeNnError::InvalidInput);
        }
        let prepared = antlr::encode(inputs, original)?;
        complete_rows(
            &prepared,
            model,
            inputs.iter().flat_map(|input| {
                input
                    .vectors
                    .iter()
                    .map(move |vector| (input.model_generation, vector.as_slice()))
            }),
            original,
        )
    })();
    let revalidation = inputs.iter().try_for_each(|input| match &input.source {
        NativeNnSource::Indexed(_) => Ok(()),
        NativeNnSource::Physical { capture, .. } => capture
            .revalidate(original)
            .map_err(|e| NativeNnError::PhysicalSource(Box::new(e))),
    });
    check(original)?;
    revalidation?;
    result
}

/// Encode available projections from the retained generic ANTLR acquisition.
pub fn encode_native_nn_antlr_generic(
    model: &NativeNnModel<'_>,
    inputs: &[NativeNnAntlrGenericInput<'_, '_, '_, '_, '_>],
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    check(original)?;
    let result = (|| {
        width(model)?;
        if inputs
            .iter()
            .any(|input| input.vectors.len() != input.entity_indices.len())
        {
            return Err(NativeNnError::InvalidInput);
        }
        let prepared = antlr_generic::encode(inputs, original)?;
        complete_rows(
            &prepared,
            model,
            inputs.iter().flat_map(|input| {
                input
                    .vectors
                    .iter()
                    .map(move |vector| (input.model_generation, vector.as_slice()))
            }),
            original,
        )
    })();
    let revalidation = inputs.iter().try_for_each(|input| match &input.source {
        NativeNnSource::Indexed(_) => Ok(()),
        NativeNnSource::Physical { capture, .. } => capture
            .revalidate(original)
            .map_err(|e| NativeNnError::PhysicalSource(Box::new(e))),
    });
    check(original)?;
    revalidation?;
    result
}

/// Encode only Available projections belonging to the supplied JSON model.
pub fn encode_native_nn_json(
    model: &NativeNnModel<'_>,
    inputs: &[NativeNnJsonInput<'_, '_, '_, '_, '_, '_>],
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    check(original)?;
    let result = (|| {
        width(model)?;
        if inputs
            .iter()
            .any(|input| input.vectors.len() != input.entity_indices.len())
        {
            return Err(NativeNnError::InvalidInput);
        }
        let prepared = encode_json(inputs, original)?;
        complete_rows(
            &prepared,
            model,
            inputs.iter().flat_map(|input| {
                input
                    .vectors
                    .iter()
                    .map(move |vector| (input.model_generation, vector.as_slice()))
            }),
            original,
        )
    })();
    let revalidation = inputs.iter().try_for_each(|input| match &input.source {
        NativeNnSource::Indexed(_) => Ok(()),
        NativeNnSource::Physical { capture, .. } => capture
            .revalidate(original)
            .map_err(|e| NativeNnError::PhysicalSource(Box::new(e))),
    });
    check(original)?;
    revalidation?;
    result
}

/// Encode available projections while retaining their genuine parser and Source owners.
pub fn encode_native_nn_ts(
    model: &NativeNnModel<'_>,
    inputs: &[NativeNnTsInput<'_, '_, '_, '_, '_, '_>],
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    check(original)?;
    let result = (|| {
        width(model)?;
        if inputs
            .iter()
            .any(|input| input.vectors.len() != input.entity_indices.len())
        {
            return Err(NativeNnError::InvalidInput);
        }
        let prepared = tree_sitter::encode(inputs, original)?;
        complete_rows(
            &prepared,
            model,
            inputs.iter().flat_map(|input| {
                input
                    .vectors
                    .iter()
                    .map(move |vector| (input.model_generation, vector.as_slice()))
            }),
            original,
        )
    })();
    let revalidation = inputs.iter().try_for_each(|input| match &input.source {
        NativeNnSource::Indexed(_) => Ok(()),
        NativeNnSource::Physical { capture, .. } => capture
            .revalidate(original)
            .map_err(|e| NativeNnError::PhysicalSource(Box::new(e))),
    });
    check(original)?;
    revalidation?;
    result
}

/// Encode generic native projections without a display-name conversion.
pub fn encode_native_nn_generic(
    model: &NativeNnModel<'_>,
    inputs: &[NativeNnGenericInput<'_, '_, '_, '_, '_, '_>],
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    check(original)?;
    let result = (|| {
        width(model)?;
        if inputs
            .iter()
            .any(|input| input.vectors.len() != input.entity_indices.len())
        {
            return Err(NativeNnError::InvalidInput);
        }
        let prepared = generic::encode(inputs, original)?;
        complete_rows(
            &prepared,
            model,
            inputs.iter().flat_map(|input| {
                input
                    .vectors
                    .iter()
                    .map(move |vector| (input.model_generation, vector.as_slice()))
            }),
            original,
        )
    })();
    let revalidation = inputs.iter().try_for_each(|input| match &input.source {
        NativeNnSource::Indexed(_) => Ok(()),
        NativeNnSource::Physical { capture, .. } => capture
            .revalidate(original)
            .map_err(|e| NativeNnError::PhysicalSource(Box::new(e))),
    });
    check(original)?;
    revalidation?;
    result
}

pub(crate) fn associated_source(
    binding: &NativeNnSource<'_>,
    identity: &crate::symbols::IdentityDocument<'_, '_>,
    source: &str,
    original: &Arc<SnapshotContext>,
) -> Result<(String, [u8; 32]), NativeNnError> {
    let path = identity.relative_components().join("/");
    let hash = digest(source.as_bytes(), original)?;
    let (raw, physical) = match binding {
        NativeNnSource::Indexed(raw) => (*raw, None),
        NativeNnSource::Physical { capture, raw } => (*raw, Some(*capture)),
    };
    let text = match (
        physical,
        raw.source(original).map_err(NativeNnError::Source)?,
    ) {
        (None, IndexedSource::Available(text)) => text,
        (Some(capture), IndexedSource::Missing)
            if raw.missing_reason() == Some(MissingSourceReason::TextNotStored) =>
        {
            if capture.relative_path() != path
                || capture.content_sha256() != hash
                || capture
                    .relative_components()
                    .iter()
                    .map(String::as_str)
                    .ne(identity.relative_components().iter().copied())
            {
                return Err(NativeNnError::InvalidInput);
            }
            capture
                .source(original)
                .map_err(|e| NativeNnError::PhysicalSource(Box::new(e)))?
        }
        _ => return Err(NativeNnError::InvalidInput),
    };
    if source.as_ptr() != text.as_ptr() || source.len() != text.len() || raw.path() != path {
        return Err(NativeNnError::InvalidInput);
    }
    Ok((path, hash))
}

fn encode_json(
    inputs: &[NativeNnJsonInput<'_, '_, '_, '_, '_, '_>],
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    let schema = native_nn_projection_schema();
    let mut columns = NnColumns::new();
    let mut seen = HashSet::new();
    for input in inputs {
        check(original)?;
        let parsed = input.document.parsed();
        if !std::ptr::eq(parsed.original_context(), original.as_ref()) {
            return Err(NativeNnError::RequestMismatch);
        }
        let identity = parsed.identity_document();
        let (path, hash) = associated_source(&input.source, identity, parsed.source(), original)?;
        let raw = match &input.source {
            NativeNnSource::Indexed(raw) | NativeNnSource::Physical { raw, .. } => *raw,
        };
        let projections = input
            .document
            .code_projections()
            .map_err(|e| NativeNnError::Semantic(Box::new(e)))?;
        let available: HashMap<_, _> = projections
            .iter()
            .filter_map(|p| match p {
                JsonProjection::Available(p) => Some((p.entity_index, p)),
                JsonProjection::Unavailable(_) => None,
            })
            .collect();
        let observation = raw.branch_observation();
        for &index in input.entity_indices {
            check(original)?;
            let p = available.get(&index).ok_or(NativeNnError::InvalidInput)?;
            let entity = p.entity;
            if !seen.insert((entity.identity().uid().as_str(), entity.data_key())) {
                return Err(NativeNnError::DuplicateIdentity);
            }
            for (i, value) in [
                entity.identity().uid().as_str(),
                entity.name(),
                entity.identity().label(),
                &path,
                p.language,
                p.grammar,
                if observation.is_some() {
                    "branch"
                } else {
                    "module"
                },
                p.text,
            ]
            .into_iter()
            .enumerate()
            {
                push_text(&mut columns.strings[i], value)?;
            }
            push_optional(
                &mut columns.strings[8],
                observation.and_then(|o| o.physical_branch),
            )?;
            push_optional(
                &mut columns.strings[9],
                observation.and_then(|o| o.inherited_target),
            )?;
            let path_end = columns
                .paths
                .values()
                .len()
                .checked_add(identity.relative_components().len())
                .ok_or(NativeNnError::Representation)?;
            i32::try_from(path_end).map_err(|_| NativeNnError::Representation)?;
            for component in identity.relative_components() {
                push_text(columns.paths.values(), component)?;
            }
            columns.paths.append(true);
            push_text(&mut columns.name_kinds, NameKind::JsonName.wire())?;
            push_range(&mut columns.names, entity.name_raw())?;
            push_extent(&mut columns.extents, p.extent)?;
            let owner_end = columns
                .owners
                .values()
                .len()
                .checked_add(entity.namespace().len())
                .ok_or(NativeNnError::Representation)?;
            i32::try_from(owner_end).map_err(|_| NativeNnError::Representation)?;
            for owner in entity.namespace() {
                check(original)?;
                let ExtentObservation::Exact(extent) = &owner.extent else {
                    return Err(NativeNnError::InvalidInput);
                };
                let b = columns.owners.values();
                push_text(builder::<StringBuilder>(b, 0)?, owner.label)?;
                push_text(builder::<StringBuilder>(b, 1)?, &owner.name)?;
                push_range(builder::<StructBuilder>(b, 2)?, owner.anchor)?;
                push_extent(builder::<StructBuilder>(b, 3)?, extent)?;
                push_owner_name(
                    b,
                    NameDomain::Json,
                    NameKind::JsonName,
                    owner.anchor,
                    &owner.name,
                )?;
                b.append(true);
            }
            columns.owners.append(true);
            push_text(&mut columns.data_keys, entity.data_key())?;
            columns.raw_versions.append_value(raw.version());
            columns
                .source_sha
                .append_value(hash)
                .map_err(NativeNnError::Arrow)?;
            columns.push_projection(p.text, original)?;
        }
    }
    columns.finish(schema)
}

fn array<T: Array + 'static>(value: &dyn Array) -> Result<&T, NativeNnError> {
    value
        .as_any()
        .downcast_ref()
        .ok_or(NativeNnError::SchemaMismatch)
}

fn column<'a, T: Array + 'static>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a T, NativeNnError> {
    array(
        batch
            .column_by_name(name)
            .ok_or(NativeNnError::SchemaMismatch)?
            .as_ref(),
    )
}

fn field<'a, T: Array + 'static>(
    value: &'a StructArray,
    name: &str,
) -> Result<&'a T, NativeNnError> {
    array(
        value
            .column_by_name(name)
            .ok_or(NativeNnError::SchemaMismatch)?
            .as_ref(),
    )
}

fn required(value: &dyn Array) -> Result<(), NativeNnError> {
    if value.null_count() != 0 {
        return Err(NativeNnError::InvalidRow);
    }
    Ok(())
}

fn string_at(value: &StringArray, row: usize) -> Result<&str, NativeNnError> {
    if value.is_null(row) || !text_valid(value.value(row)) {
        return Err(NativeNnError::InvalidRow);
    }
    Ok(value.value(row))
}

#[derive(Clone, Copy)]
struct Range {
    start: u64,
    end: u64,
    start_row: u64,
    start_column: u64,
    end_row: u64,
    end_column: u64,
}

fn read_range(value: &StructArray, row: usize) -> Result<Range, NativeNnError> {
    required(value)?;
    let mut numbers = [0; 6];
    for (i, name) in [
        "start",
        "end",
        "start_row",
        "start_column",
        "end_row",
        "end_column",
    ]
    .into_iter()
    .enumerate()
    {
        let values = field::<UInt64Array>(value, name)?;
        required(values)?;
        numbers[i] = values.value(row);
    }
    let [start, end, start_row, start_column, end_row, end_column] = numbers;
    if start >= end
        || start_row > end_row
        || start_row > start
        || end_row > end
        || start_column > start
        || end_column > end
        || (start_row == end_row && end_column.checked_sub(start_column) != end.checked_sub(start))
    {
        return Err(NativeNnError::InvalidRow);
    }
    Ok(Range {
        start,
        end,
        start_row,
        start_column,
        end_row,
        end_column,
    })
}

fn read_extent(value: &StructArray, row: usize) -> Result<Range, NativeNnError> {
    required(value)?;
    let range = read_range(field(value, "range")?, row)?;
    let start = field::<UInt64Array>(value, "start_line")?;
    let end = field::<UInt64Array>(value, "end_line")?;
    required(start)?;
    required(end)?;
    let expected_start = range
        .start_row
        .checked_add(1)
        .ok_or(NativeNnError::Representation)?;
    let expected_end = range
        .end_row
        .checked_add(u64::from(range.end_column != 0))
        .ok_or(NativeNnError::Representation)?;
    if start.value(row) != expected_start
        || end.value(row) != expected_end
        || expected_start > expected_end
    {
        return Err(NativeNnError::InvalidRow);
    }
    Ok(range)
}

fn contains(parent: Range, child: Range) -> bool {
    let start_order = parent.start.cmp(&child.start);
    let end_order = child.end.cmp(&parent.end);
    parent.start <= child.start
        && child.end <= parent.end
        && start_order
            == (parent.start_row, parent.start_column).cmp(&(child.start_row, child.start_column))
        && end_order == (child.end_row, child.end_column).cmp(&(parent.end_row, parent.end_column))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NameKind {
    Identifier,
    LiteralSource,
    JsonName,
    RustType,
    NativeSource,
    ScipSource,
}
impl NameKind {
    fn wire(self) -> &'static str {
        match self {
            Self::Identifier => "Identifier",
            Self::LiteralSource => "LiteralSource",
            Self::JsonName => "JsonName",
            Self::RustType => "RustType",
            Self::NativeSource => "NativeSource",
            Self::ScipSource => "ScipSource",
        }
    }
    fn read(value: &str) -> Result<Self, NativeNnError> {
        match value {
            "Identifier" => Ok(Self::Identifier),
            "LiteralSource" => Ok(Self::LiteralSource),
            "JsonName" => Ok(Self::JsonName),
            "RustType" => Ok(Self::RustType),
            "NativeSource" => Ok(Self::NativeSource),
            "ScipSource" => Ok(Self::ScipSource),
            _ => Err(NativeNnError::InvalidRow),
        }
    }
}

#[derive(Clone, Copy)]
enum NameDomain {
    Ecma(crate::ecma_names::EcmaNameDialect),
    Rust,
    Go,
    Python,
    Json,
    Oracle,
    Db2,
    Generic,
    Scip,
}
impl NameDomain {
    fn matches(self, kind: NameKind, lexical: &str, decoded: &str) -> bool {
        match kind {
            NameKind::Identifier if !matches!(self, Self::Json | Self::Generic) => {
                self.decoded_name_matches(lexical, decoded)
            }
            NameKind::LiteralSource if matches!(self, Self::Go | Self::Python | Self::Ecma(_)) => {
                lexical == decoded
            }
            NameKind::JsonName if matches!(self, Self::Json) => {
                self.decoded_name_matches(lexical, decoded)
            }
            NameKind::RustType if matches!(self, Self::Rust) => {
                lexical == decoded
                    && syn::parse_str::<syn::Type>(lexical)
                        .is_ok_and(|t| !matches!(t, syn::Type::Verbatim(_)))
            }
            NameKind::NativeSource if matches!(self, Self::Generic) => lexical == decoded,
            NameKind::ScipSource if matches!(self, Self::Scip) => lexical == decoded,
            _ => false,
        }
    }
    fn decoded_name_matches(self, lexical: &str, decoded: &str) -> bool {
        match self {
            Self::Ecma(dialect) => crate::ecma_names::decode_ecma_identifier_name(lexical, dialect)
                .is_ok_and(|name| name == decoded),
            Self::Rust => rust_name(lexical).is_some_and(|name| name == decoded),
            Self::Go => {
                static IDENTIFIER: std::sync::LazyLock<regex::Regex> =
                    std::sync::LazyLock::new(|| {
                        regex::Regex::new(r"\A[_\p{L}][_\p{L}\p{Nd}]*\z")
                            .expect("constant Go identifier category expression")
                    });
                lexical == decoded && IDENTIFIER.is_match(lexical)
            }
            Self::Python => {
                crate::python_names::canonical_identifier(lexical).is_ok_and(|name| name == decoded)
            }
            Self::Json => json_name_matches(lexical, decoded),
            Self::Generic | Self::Scip => false,
            Self::Db2 => crate::antlr_engine::model::db2::decode_name(lexical)
                .is_ok_and(|parts| parts.last().is_some_and(|part| part.value == decoded)),
            Self::Oracle => {
                use sqlparser::{
                    dialect::GenericDialect,
                    tokenizer::{Token, Tokenizer},
                };
                let Ok(tokens) = Tokenizer::new(&GenericDialect, lexical)
                    .with_unescape(true)
                    .tokenize()
                else {
                    return false;
                };
                matches!(tokens.as_slice(), [Token::Word(word)]
                    if matches!(word.quote_style, None | Some('"')) && word.value == decoded)
            }
        }
    }
}

fn points_for(
    projection: &str,
    extent: Range,
    name: Range,
    text: &str,
    domain: NameDomain,
    kind: NameKind,
) -> Result<(), NativeNnError> {
    let source =
        SourceText::new(projection.as_bytes(), projection.len()).map_err(NativeNnError::Range)?;
    let end = source
        .point_at(projection.len())
        .map_err(NativeNnError::Range)?;
    let point = |row: usize, column: usize| -> Result<(u64, u64), NativeNnError> {
        let column = u64::try_from(column).map_err(|_| NativeNnError::Representation)?;
        let column = if row == 0 {
            column
                .checked_add(extent.start_column)
                .ok_or(NativeNnError::Representation)?
        } else {
            column
        };
        Ok((
            extent
                .start_row
                .checked_add(u64::try_from(row).map_err(|_| NativeNnError::Representation)?)
                .ok_or(NativeNnError::Representation)?,
            column,
        ))
    };
    if point(end.row, end.column)? != (extent.end_row, extent.end_column) {
        return Err(NativeNnError::InvalidRow);
    }
    let start = usize::try_from(
        name.start
            .checked_sub(extent.start)
            .ok_or(NativeNnError::InvalidRow)?,
    )
    .map_err(|_| NativeNnError::Representation)?;
    let end = usize::try_from(
        name.end
            .checked_sub(extent.start)
            .ok_or(NativeNnError::InvalidRow)?,
    )
    .map_err(|_| NativeNnError::Representation)?;
    let candidate = source.range(start, end).map_err(NativeNnError::Range)?;
    let a = candidate.start_point();
    let b = candidate.end_point();
    let name_matches = domain.matches(kind, candidate.text(), text);
    if !name_matches
        || point(a.row, a.column)? != (name.start_row, name.start_column)
        || point(b.row, b.column)? != (name.end_row, name.end_column)
    {
        return Err(NativeNnError::InvalidRow);
    }
    Ok(())
}

fn push_owner_name(
    fields: &mut StructBuilder,
    domain: NameDomain,
    kind: NameKind,
    lexical: SourceRange<'_>,
    name: &str,
) -> Result<(), NativeNnError> {
    if !domain.matches(kind, lexical.text(), name) {
        return Err(NativeNnError::InvalidInput);
    }
    push_text(builder::<StringBuilder>(fields, 4)?, kind.wire())?;
    push_text(builder::<StringBuilder>(fields, 5)?, lexical.text())?;
    Ok(())
}

fn check_owner_lexeme(
    lexical: &str,
    name: &str,
    range: Range,
    domain: NameDomain,
    kind: NameKind,
) -> Result<(), NativeNnError> {
    if !domain.matches(kind, lexical, name)
        || u64::try_from(lexical.len()).map_err(|_| NativeNnError::Representation)?
            != range.end - range.start
    {
        return Err(NativeNnError::InvalidRow);
    }
    let source =
        SourceText::new(lexical.as_bytes(), lexical.len()).map_err(NativeNnError::Range)?;
    let end = source
        .point_at(lexical.len())
        .map_err(NativeNnError::Range)?;
    let end_row = range
        .start_row
        .checked_add(u64::try_from(end.row).map_err(|_| NativeNnError::Representation)?)
        .ok_or(NativeNnError::Representation)?;
    let column = u64::try_from(end.column).map_err(|_| NativeNnError::Representation)?;
    let end_column = if end.row == 0 {
        range
            .start_column
            .checked_add(column)
            .ok_or(NativeNnError::Representation)?
    } else {
        column
    };
    if (end_row, end_column) != (range.end_row, range.end_column) {
        return Err(NativeNnError::InvalidRow);
    }
    Ok(())
}

fn rust_name(lexical: &str) -> Option<String> {
    use syn::{ext::IdentExt, parse::Parser};
    let name = syn::Ident::parse_any
        .parse_str(lexical)
        .ok()?
        .unraw()
        .to_string();
    (lexical == name || lexical.strip_prefix("r#") == Some(name.as_str())).then_some(name)
}

fn json_name_matches(lexical: &str, decoded: &str) -> bool {
    if lexical.starts_with('"') {
        serde_json::from_str::<String>(lexical).is_ok_and(|name| name == decoded)
    } else {
        serde_json::from_str::<&serde_json::value::RawValue>(lexical).is_ok_and(|value| {
            matches!(
                value.get().as_bytes().first(),
                Some(b'-' | b'0'..=b'9' | b't' | b'f' | b'n')
            ) && value.get() == decoded
        })
    }
}

fn digest(bytes: &[u8], original: &SnapshotContext) -> Result<[u8; 32], NativeNnError> {
    let mut hash = Sha256::new();
    for chunk in bytes.chunks(16_384) {
        check(original)?;
        hash.update(chunk);
    }
    check(original)?;
    Ok(hash.finalize().into())
}

fn finite(values: &[f32], original: &SnapshotContext) -> Result<(), NativeNnError> {
    for chunk in values.chunks(16_384) {
        check(original)?;
        if chunk.iter().any(|value| !value.is_finite()) {
            return Err(NativeNnError::InvalidVector);
        }
    }
    Ok(())
}

/// Validate persisted representation and identity; this does not restore provenance.
pub fn validate_native_nn(
    batch: &RecordBatch,
    model: &NativeNnModel<'_>,
    original: &SnapshotContext,
) -> Result<(), NativeNnError> {
    check(original)?;
    let result = validate(batch, model, original);
    check(original)?;
    result
}

/// Validate durable projections before scheduling inference. This validates their
/// representation and projection recipe, not authorization or current FILE freshness.
pub fn validate_native_nn_projection(
    batch: &RecordBatch,
    original: &SnapshotContext,
) -> Result<(), NativeNnError> {
    check(original)?;
    if batch.schema().as_ref() != native_nn_projection_schema().as_ref() {
        return Err(NativeNnError::SchemaMismatch);
    }
    validate_rows(batch, original)?;
    check(original)
}

fn validate(
    batch: &RecordBatch,
    model: &NativeNnModel<'_>,
    original: &SnapshotContext,
) -> Result<(), NativeNnError> {
    if batch.schema().as_ref() != native_nn_schema(model)?.as_ref() {
        return Err(NativeNnError::SchemaMismatch);
    }
    let vectors = column::<FixedSizeListArray>(batch, "vector")?;
    required(vectors)?;
    let values = array::<Float32Array>(vectors.values().as_ref())?;
    if values.null_count() != 0 {
        return Err(NativeNnError::InvalidVector);
    }
    finite(values.values(), original)?;
    validate_rows(batch, original)
}

fn validate_rows(batch: &RecordBatch, original: &SnapshotContext) -> Result<(), NativeNnError> {
    for (field, value) in batch.schema().fields().iter().zip(batch.columns()) {
        if !field.is_nullable() {
            required(value.as_ref())?;
        }
    }
    let uids = column::<StringArray>(batch, "uid")?;
    let data_keys = column::<StringArray>(batch, "data_key")?;
    let names = column::<StringArray>(batch, "name")?;
    let name_kinds = column::<StringArray>(batch, "name_kind")?;
    let kinds = column::<StringArray>(batch, "kind")?;
    let paths = column::<StringArray>(batch, "path")?;
    let components = column::<ListArray>(batch, "path_components")?;
    let languages = column::<StringArray>(batch, "language")?;
    let grammars = column::<StringArray>(batch, "grammar")?;
    let anchors = column::<StructArray>(batch, "name_anchor")?;
    let extents = column::<StructArray>(batch, "extent")?;
    let owners = column::<ListArray>(batch, "namespace")?;
    let origins = column::<StringArray>(batch, "raw_origin")?;
    let physical = column::<StringArray>(batch, "raw_physical_branch")?;
    let inherited = column::<StringArray>(batch, "raw_inherited_target")?;
    let projections = column::<StringArray>(batch, "projection")?;
    let hashes = column::<FixedSizeBinaryArray>(batch, "projection_sha256")?;
    let mut seen = HashSet::new();
    seen.try_reserve(batch.num_rows())
        .map_err(NativeNnError::Allocation)?;
    for row in 0..batch.num_rows() {
        check(original)?;
        if !seen.insert((string_at(uids, row)?, string_at(data_keys, row)?)) {
            return Err(NativeNnError::DuplicateIdentity);
        }
        let name_kind = NameKind::read(string_at(name_kinds, row)?)?;
        let domain = match (string_at(languages, row)?, string_at(grammars, row)?) {
            (language, grammar)
                if name_kind == NameKind::ScipSource
                    && data_keys.value(row) == "scip_definitions"
                    && text_valid(language)
                    && text_valid(grammar) =>
            {
                NameDomain::Scip
            }
            ("rust", "tree-sitter-rust") => NameDomain::Rust,
            ("go", "tree-sitter-go") => NameDomain::Go,
            ("python", "tree-sitter-python") => NameDomain::Python,
            ("javascript", "tree-sitter-javascript") => {
                NameDomain::Ecma(crate::ecma_names::EcmaNameDialect::JavaScript)
            }
            ("typescript", "tree-sitter-typescript") | ("tsx", "tree-sitter-tsx") => {
                NameDomain::Ecma(crate::ecma_names::EcmaNameDialect::TypeScript)
            }
            ("json", "tree-sitter-json") => NameDomain::Json,
            ("plsql", "antlr-plsql") => NameDomain::Oracle,
            ("db2", "antlr-db2") => NameDomain::Db2,
            (language, grammar)
                if name_kind == NameKind::NativeSource
                    && antlr_generic::language_matches(language, grammar) =>
            {
                NameDomain::Generic
            }
            (language, grammar)
                if name_kind == NameKind::NativeSource
                    && generic::language_is_generic(language, grammar) =>
            {
                NameDomain::Generic
            }
            _ => return Err(NativeNnError::InvalidRow),
        };
        let name = names.value(row);
        if name_kind == NameKind::RustType {
            return Err(NativeNnError::InvalidRow);
        }
        string_at(kinds, row)?;
        if matches!(domain, NameDomain::Rust) && !text_valid(name) {
            return Err(NativeNnError::InvalidRow);
        }
        if matches!(domain, NameDomain::Oracle) && !text_valid(name) {
            return Err(NativeNnError::InvalidRow);
        }
        let path = string_at(paths, row)?;
        let parts = components.value(row);
        let parts = array::<StringArray>(parts.as_ref())?;
        required(parts)?;
        if parts.is_empty() {
            return Err(NativeNnError::InvalidRow);
        }
        let mut remaining = path;
        for index in 0..parts.len() {
            let component = string_at(parts, index)?;
            if component == "." || component == ".." || component.contains('/') {
                return Err(NativeNnError::InvalidRow);
            }
            remaining = remaining
                .strip_prefix(component)
                .ok_or(NativeNnError::InvalidRow)?;
            if index + 1 != parts.len() {
                remaining = remaining
                    .strip_prefix('/')
                    .ok_or(NativeNnError::InvalidRow)?;
            }
        }
        if !remaining.is_empty() {
            return Err(NativeNnError::InvalidRow);
        }
        match string_at(origins, row)? {
            "module" if physical.is_null(row) && inherited.is_null(row) => {}
            "branch" => {
                if !physical.is_null(row) {
                    string_at(physical, row)?;
                }
                if !inherited.is_null(row) {
                    string_at(inherited, row)?;
                }
            }
            _ => return Err(NativeNnError::InvalidRow),
        }
        let extent = read_extent(extents, row)?;
        let anchor = read_range(anchors, row)?;
        let projection = projections.value(row);
        if !contains(extent, anchor)
            || u64::try_from(projection.len()).map_err(|_| NativeNnError::Representation)?
                != extent.end - extent.start
            || digest(projection.as_bytes(), original)?.as_slice() != hashes.value(row)
        {
            return Err(NativeNnError::InvalidRow);
        }
        points_for(projection, extent, anchor, name, domain, name_kind)?;
        let frames = owners.value(row);
        let frames = array::<StructArray>(frames.as_ref())?;
        required(frames)?;
        let mut outer = None;
        for i in 0..frames.len() {
            check(original)?;
            string_at(field(frames, "kind")?, i)?;
            let owner_names = field::<StringArray>(frames, "name")?;
            required(owner_names)?;
            let owner_name = owner_names.value(i);
            if matches!(domain, NameDomain::Oracle) && !text_valid(owner_name) {
                return Err(NativeNnError::InvalidRow);
            }
            let owner = read_extent(field(frames, "extent")?, i)?;
            let anchor = read_range(field(frames, "name_anchor")?, i)?;
            let owner_kind = NameKind::read(string_at(field(frames, "name_kind")?, i)?)?;
            let owner_raw = string_at(field(frames, "raw_name")?, i)?;
            check_owner_lexeme(owner_raw, owner_name, anchor, domain, owner_kind)?;
            if (matches!(domain, NameDomain::Rust) && !text_valid(owner_name))
                || !contains(owner, anchor)
                || !contains(owner, extent)
                || (owner.start == extent.start && owner.end == extent.end)
            {
                return Err(NativeNnError::InvalidRow);
            }
            if let Some(parent) = outer
                && (!contains(parent, owner)
                    || (parent.start == owner.start && parent.end == owner.end))
            {
                return Err(NativeNnError::InvalidRow);
            }
            outer = Some(owner);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "native_nn_prepared_tests.rs"]
mod prepared_tests;
