//! Searchable observations of every semantic graph node, independently of embeddings.
use crate::semantic_graph::{AstGraphParts, AstGraphValue};
use arrow_array::{
    Array, ArrayRef, FixedSizeBinaryArray, Float32Array, RecordBatch, StringArray, UInt64Array,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use graphit_core::file_io::SnapshotContext;
use graphit_data::{
    CapturedLocalGraph, graph_search_sidecar::FullTextSearchQuery,
    lance_module_store::ModuleSnapshot,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    io,
    sync::Arc,
};

pub const TEXT_COLUMNS: &[&str] = &["name", "content"];
/// One native corpus snapshot. Search does not capture Source or the graph.
/// Published local artifacts additionally retain their immutable-file guard.
pub struct EntitySearch {
    corpus: graphit_data::lance_branch_store::BranchSnapshot,
    integrity: Option<Arc<CapturedLocalGraph>>,
    original: Arc<SnapshotContext>,
}
impl EntitySearch {
    pub async fn open(
        graph: Arc<CapturedLocalGraph>,
        original: Arc<SnapshotContext>,
    ) -> io::Result<Option<Self>> {
        graph.revalidate(&original).map_err(io::Error::other)?;
        let Some(corpus) = graph.open_corpus(&original).await? else {
            return Ok(None);
        };
        let mut search = Self::from_corpus(corpus, original)?;
        search.integrity = Some(graph);
        search.revalidate()?;
        Ok(Some(search))
    }
    pub fn from_corpus(
        corpus: graphit_data::lance_branch_store::BranchSnapshot,
        original: Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        original.check()?;
        crate::corpus::validate_schema(corpus.schema())?;
        Ok(Self {
            corpus,
            integrity: None,
            original,
        })
    }
    pub fn nn_present(&self) -> bool {
        crate::corpus::vector_dimensions(self.corpus.schema()).is_some()
    }
    pub async fn nn(
        &self,
        model: &crate::native_nn::NativeNnModel<'_>,
    ) -> io::Result<Option<graphit_data::lance_branch_store::BranchSnapshot>> {
        self.revalidate()?;
        let expected = crate::corpus::vector_fields(model)?;
        match self
            .corpus
            .schema()
            .field_with_name(crate::corpus::VECTOR)
            .ok()
        {
            None => Ok(None),
            Some(field) if field == expected.field(0) => Ok(Some(self.corpus.clone())),
            Some(_) => Err(io::Error::other(
                graphit_data::lance_branch_store::BranchError::SchemaMismatch,
            )),
        }
    }
    pub fn revalidate(&self) -> io::Result<()> {
        self.original.check()?;
        if let Some(graph) = &self.integrity {
            graph.revalidate(&self.original).map_err(io::Error::other)?;
        }
        Ok(())
    }
    pub async fn query(&self, text: &str, k: usize) -> io::Result<Vec<Hit>> {
        self.revalidate()?;
        let result = query(&self.corpus.module_snapshot(), text, k, &self.original).await?;
        self.revalidate()?;
        Ok(result)
    }
    /// Observe actual primary keys for NN identities in one bounded SDK predicate.
    /// No identity or graph key is reconstructed from UID/data_key.
    pub async fn entities(&self, identities: &[(&str, &str)]) -> io::Result<Vec<Entity>> {
        self.revalidate()?;
        if identities.is_empty() {
            return Ok(Vec::new());
        }
        use lance::deps::datafusion::logical_expr::{col, lit};
        let filter = identities
            .iter()
            .map(|(uid, data_key)| {
                col("uid")
                    .eq(lit(*uid))
                    .and(col("data_key").eq(lit(*data_key)))
            })
            .reduce(|left, right| left.or(right))
            .ok_or_else(invalid)?;
        let mut scan = self.corpus.module_snapshot().scan();
        let columns = self
            .corpus
            .schema()
            .fields()
            .iter()
            .map(|f| f.name().as_str())
            .filter(|name| *name != "content" && schema().field_with_name(name).is_ok())
            .collect::<Vec<_>>();
        scan.project(&columns).map_err(io::Error::other)?;
        scan.filter_expr(filter);
        let result = async {
            let mut stream = scan.try_into_stream().await.map_err(io::Error::other)?;
            let mut entities = Vec::new();
            while let Some(batch) = stream.try_next().await.map_err(io::Error::other)? {
                self.revalidate()?;
                entities.extend(decode_entities(&batch)?);
                if entities.len() > identities.len() {
                    return Err(invalid());
                }
            }
            Ok(entities)
        };
        let result = tokio::select! {
            biased;
            ()=self.original.cancellation().cancelled()=>Err(io::ErrorKind::Interrupted.into()),
            ()=tokio::time::sleep_until(self.original.required_deadline()?.into())=>Err(io::ErrorKind::TimedOut.into()),
            result=result=>result,
        };
        self.revalidate()?;
        result
    }

    /// Read rerank text from the same published corpus and pin as the candidates.
    /// Lexical-only entities need no exact Source span or embedding to be ranked.
    pub async fn rerank_documents(
        &self,
        identities: &[(&str, &str)],
        max_bytes: usize,
    ) -> io::Result<Vec<Option<String>>> {
        self.revalidate()?;
        if identities.is_empty() {
            return Ok(Vec::new());
        }
        if max_bytes == 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let slots = identities
            .iter()
            .copied()
            .enumerate()
            .map(|(index, identity)| (identity, index))
            .collect::<HashMap<_, _>>();
        if slots.len() != identities.len() {
            return Err(invalid());
        }
        use lance::deps::datafusion::logical_expr::{col, lit};
        let filter = identities
            .iter()
            .map(|(uid, key)| col("uid").eq(lit(*uid)).and(col("data_key").eq(lit(*key))))
            .reduce(|left, right| left.or(right))
            .ok_or_else(invalid)?;
        let mut columns = schema()
            .fields()
            .iter()
            .map(|field| field.name().clone())
            .collect::<Vec<_>>();
        columns.extend(["projection".into(), "projection_sha256".into()]);
        let columns = columns.iter().map(String::as_str).collect::<Vec<_>>();
        let mut scan = self.corpus.module_snapshot().scan();
        scan.project(&columns).map_err(io::Error::other)?;
        scan.filter_expr(filter);
        let operation = async {
            let mut stream = scan.try_into_stream().await.map_err(io::Error::other)?;
            let mut documents = vec![None; identities.len()];
            let mut bytes = 0usize;
            while let Some(batch) = stream.try_next().await.map_err(io::Error::other)? {
                self.revalidate()?;
                let projected = array::<StringArray>(&batch, "projection")?;
                let fingerprints = array::<FixedSizeBinaryArray>(&batch, "projection_sha256")?;
                let content = array::<StringArray>(&batch, "content")?;
                for (row, entity) in decode_entities(&batch)?.into_iter().enumerate() {
                    self.original.check()?;
                    let index = *slots
                        .get(&(entity.uid.as_str(), entity.data_key.as_str()))
                        .ok_or_else(invalid)?;
                    if documents[index].is_some() {
                        return Err(invalid());
                    }
                    let text = if !projected.is_null(row) {
                        let text = projected.value(row);
                        if text.is_empty()
                            || fingerprints.is_null(row)
                            || Sha256::digest(text.as_bytes()).as_slice() != fingerprints.value(row)
                        {
                            return Err(invalid());
                        }
                        text.to_owned()
                    } else if !content.is_null(row) && !content.value(row).trim().is_empty() {
                        content.value(row).to_owned()
                    } else {
                        // Some SCIP symbols have only metadata, without source
                        // extents or documentation. Rank their published identity.
                        format!("{} {} in {}", entity.kind, entity.name, entity.path)
                    };
                    bytes = bytes
                        .checked_add(text.len())
                        .filter(|bytes| *bytes <= max_bytes)
                        .ok_or(io::ErrorKind::InvalidInput)?;
                    documents[index] = Some(text);
                }
            }
            self.revalidate()?;
            Ok(documents)
        };
        tokio::select! {
            biased;
            () = self.original.cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
            () = tokio::time::sleep_until(self.original.required_deadline()?.into()) => Err(io::ErrorKind::TimedOut.into()),
            result = operation => result,
        }
    }

    /// Search compatible vectors already present in this corpus, independently of graph/Source progress.
    pub async fn query_nn(
        &self,
        model: &crate::native_nn::NativeNnModel<'_>,
        vector: &[f32],
        k: usize,
        metric: crate::native_nn_query::NativeNnMetric,
    ) -> io::Result<Option<crate::native_nn_query::NativeNnMatches>> {
        self.revalidate()?;
        let Some(snapshot) = self.nn(model).await? else {
            return Ok(None);
        };
        let matches = crate::native_nn_query::query_corpus_nn(
            &snapshot,
            model,
            vector,
            k,
            metric,
            &self.original,
        )
        .await
        .map_err(io::Error::other)?;
        self.revalidate()?;
        Ok(Some(matches))
    }

    /// Validate an explicitly materialized export to EOF, one SDK batch at a time.
    /// No inference, reparse, whole-corpus identity map or rewritten row versions.
    pub async fn validate_export(
        &self,
        model: Option<&crate::native_nn::NativeNnModel<'_>>,
    ) -> io::Result<()> {
        let mut scan = self.corpus.module_snapshot().scan();
        let columns = self
            .corpus
            .schema()
            .fields()
            .iter()
            .map(|field| field.name().as_str())
            .filter(|name| *name != "content" && schema().field_with_name(name).is_ok())
            .collect::<Vec<_>>();
        scan.project(&columns).map_err(io::Error::other)?;
        let operation = async {
            let mut rows = scan.try_into_stream().await.map_err(io::Error::other)?;
            while let Some(batch) = rows.try_next().await.map_err(io::Error::other)? {
                self.original.check()?;
                decode_entities(&batch)?;
            }
            match (
                crate::corpus::vector_dimensions(self.corpus.schema()),
                model,
            ) {
                (None, None) => (),
                (Some(_), Some(model)) => {
                    let snapshot = self.nn(model).await?.ok_or_else(invalid)?;
                    let mut rows = snapshot
                        .scan()
                        .try_into_stream()
                        .await
                        .map_err(io::Error::other)?;
                    while let Some(batch) = rows.try_next().await.map_err(io::Error::other)? {
                        crate::corpus::vectors(&batch, model, &self.original)?;
                    }
                }
                _ => return Err(invalid()),
            }
            self.revalidate()
        };
        tokio::select! {
            biased;
            ()=self.original.cancellation().cancelled()=>Err(io::ErrorKind::Interrupted.into()),
            ()=tokio::time::sleep_until(self.original.required_deadline()?.into())=>Err(io::ErrorKind::TimedOut.into()),
            result=operation=>result,
        }
    }
}
pub fn schema() -> Arc<Schema> {
    let mut fields = vec![
        "entity_key",
        "uid",
        "data_key",
        "name",
        "kind",
        "path",
        "language",
        "grammar",
        "source_sha256",
        "extent_status",
    ]
    .into_iter()
    .map(|name| Field::new(name, DataType::Utf8, false))
    .collect::<Vec<_>>();
    fields.extend([
        Field::new("content", DataType::Utf8, true),
        Field::new("raw_version", DataType::UInt64, false),
        Field::new("start_byte", DataType::UInt64, true),
        Field::new("end_byte", DataType::UInt64, true),
        Field::new("start_line", DataType::UInt64, true),
        Field::new("end_line", DataType::UInt64, true),
    ]);
    Arc::new(Schema::new_with_metadata(
        fields,
        HashMap::from([
            ("graphit.module".into(), "ast-entity-search".into()),
            ("graphit.ast.search.format".into(), "all-entity-2".into()),
            ("graphit.ast.search.analyzer".into(), "code".into()),
        ]),
    ))
}

/// An observed entity from the retained all-entity pin. These fields grant no access.
#[derive(Clone, Debug, Serialize)]
pub struct Entity {
    pub entity_key: String,
    pub uid: String,
    pub data_key: String,
    pub name: String,
    pub kind: String,
    pub path: String,
    pub language: String,
    pub grammar: String,
    pub source_sha256: String,
    pub extent_status: String,
    pub raw_version: u64,
    pub start_byte: Option<u64>,
    pub end_byte: Option<u64>,
    pub start_line: Option<u64>,
    pub end_line: Option<u64>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Hit {
    #[serde(flatten)]
    pub entity: Entity,
    pub score: f32,
}

fn array<'a, T: Array + 'static>(batch: &'a RecordBatch, name: &str) -> io::Result<&'a T> {
    batch
        .column_by_name(name)
        .and_then(|a| a.as_any().downcast_ref())
        .ok_or_else(invalid)
}
pub(crate) fn decode_entities(batch: &RecordBatch) -> io::Result<Vec<Entity>> {
    let text = |name, row| -> io::Result<String> {
        let values = array::<StringArray>(batch, name)?;
        if values.is_null(row) {
            return Err(invalid());
        }
        Ok(values.value(row).to_owned())
    };
    let integer = |name, row| -> io::Result<Option<u64>> {
        let values = array::<UInt64Array>(batch, name)?;
        Ok((!values.is_null(row)).then(|| values.value(row)))
    };
    (0..batch.num_rows())
        .map(|row| {
            Ok(Entity {
                entity_key: text("entity_key", row)?,
                uid: text("uid", row)?,
                data_key: text("data_key", row)?,
                name: text("name", row)?,
                kind: text("kind", row)?,
                path: text("path", row)?,
                language: text("language", row)?,
                grammar: text("grammar", row)?,
                source_sha256: text("source_sha256", row)?,
                extent_status: text("extent_status", row)?,
                raw_version: integer("raw_version", row)?.ok_or_else(invalid)?,
                start_byte: integer("start_byte", row)?,
                end_byte: integer("end_byte", row)?,
                start_line: integer("start_line", row)?,
                end_line: integer("end_line", row)?,
            })
        })
        .collect()
}
fn decode(batch: &RecordBatch) -> io::Result<Vec<Hit>> {
    let scores = array::<Float32Array>(batch, "_score")?;
    decode_entities(batch)?
        .into_iter()
        .enumerate()
        .map(|(row, entity)| {
            if scores.is_null(row) || !scores.value(row).is_finite() {
                return Err(invalid());
            }
            Ok(Hit {
                entity,
                score: scores.value(row),
            })
        })
        .collect()
}

/// Use the SDK's BM25 indices on the captured all-entity corpus, reaching EOF.
/// The caller keeps its physical graph capture and authorization alive and
/// revalidates them around this call and before delivering observations.
pub async fn query(
    snapshot: &ModuleSnapshot,
    text: &str,
    k: usize,
    original: &Arc<SnapshotContext>,
) -> io::Result<Vec<Hit>> {
    original.check()?;
    if crate::corpus::validate_schema(snapshot.schema()).is_err()
        || text.trim().is_empty()
        || text.contains('\0')
        || k == 0
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let query = FullTextSearchQuery::new(text.to_owned())
        .with_columns(
            &TEXT_COLUMNS
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<Vec<_>>(),
        )
        .map_err(io::Error::other)?
        .limit(Some(
            i64::try_from(k).map_err(|_| io::ErrorKind::InvalidInput)?,
        ));
    let mut scanner = snapshot.scan();
    let mut columns = snapshot
        .schema()
        .fields()
        .iter()
        .map(|f| f.name().as_str())
        .filter(|name| *name != "content" && schema().field_with_name(name).is_ok())
        .collect::<Vec<_>>();
    columns.push("_score");
    scanner.project(&columns).map_err(io::Error::other)?;
    scanner.prefilter(true);
    scanner.full_text_search(query).map_err(io::Error::other)?;
    let result = async {
        let mut stream = scanner.try_into_stream().await.map_err(io::Error::other)?;
        let mut hits = Vec::new();
        while let Some(batch) = stream.try_next().await.map_err(io::Error::other)? {
            original.check()?;
            hits.extend(decode(&batch)?);
            if hits.len() > k {
                return Err(invalid());
            }
        }
        original.check()?;
        Ok(hits)
    };
    let result = tokio::select! {
        biased;
        () = original.cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
        () = tokio::time::sleep_until(original.required_deadline()?.into()) => Err(io::ErrorKind::TimedOut.into()),
        result = result => result,
    };
    original.check()?;
    result
}
fn invalid() -> io::Error {
    io::ErrorKind::InvalidData.into()
}
#[derive(Default, Deserialize)]
struct Attached {
    #[serde(default)]
    attachments: Vec<Attachment>,
}
#[derive(Deserialize)]
struct Attachment {
    node_index: usize,
    literal_source: Option<String>,
}
#[derive(Deserialize)]
struct Contribution {
    go_declaration: Option<Attached>,
    ecma_declaration: Option<Attached>,
    callable: Option<Attached>,
}
#[derive(Deserialize)]
struct Documentation {
    contributions: Vec<Contribution>,
}
fn append_documentation(content: &mut Option<String>, metadata: &str) -> io::Result<()> {
    let documentation: Documentation = serde_json::from_str(metadata).map_err(io::Error::other)?;
    let mut seen = HashSet::new();
    for contribution in documentation.contributions {
        for declaration in [
            contribution.go_declaration,
            contribution.ecma_declaration,
            contribution.callable,
        ]
        .into_iter()
        .flatten()
        {
            for attachment in declaration.attachments {
                if seen.insert(attachment.node_index)
                    && let Some(text) = attachment.literal_source
                {
                    let target = content.get_or_insert_with(String::new);
                    if !target.is_empty() {
                        target.push('\n');
                    }
                    target.push_str(&text);
                }
            }
        }
    }
    Ok(())
}
/// The rows come from the same complete graph batch, never NativeNN's selected projections.
/// Text is a borrowed original FILE. Non-exact nodes retain name/path observations only.
pub fn encode_file(
    parts: &AstGraphParts,
    text: &str,
    original: &Arc<SnapshotContext>,
) -> io::Result<RecordBatch> {
    original.check()?;
    if Sha256::digest(text.as_bytes()).as_slice() != parts.association.source_sha256() {
        return Err(invalid());
    }
    let mut strings = vec![Vec::<String>::new(); 10];
    let mut content = Vec::new();
    let mut raw = Vec::new();
    let mut ranges = vec![Vec::<Option<u64>>::new(); 4];
    for node in &parts.nodes {
        original.check()?;
        let props = node.properties();
        let string = |key: &str| match props.get(key) {
            Some(AstGraphValue::String(value)) => Ok(value.as_str()),
            _ => Err(invalid()),
        };
        let extent = string("extent_status")?;
        for (i, value) in [
            node.key(),
            string("uid")?,
            string("data_key")?,
            string("name")?,
            node.label(),
            string("path")?,
            string("lang")?,
            string("grammar")?,
            string("source_sha256")?,
            extent,
        ]
        .into_iter()
        .enumerate()
        {
            strings[i].push(value.to_owned());
        }
        raw.push(parts.association.raw_version());
        let integer = |key: &str| match props.get(key) {
            Some(AstGraphValue::Int64(value)) => {
                u64::try_from(*value).map(Some).map_err(|_| invalid())
            }
            None => Ok(None),
            _ => Err(invalid()),
        };
        let mut values = if extent == "Exact" {
            [
                integer("start_byte")?,
                integer("end_byte")?,
                integer("start_line")?,
                integer("end_line")?,
            ]
        } else {
            [None; 4]
        };
        let mut searchable = match (values[0], values[1]) {
            (Some(start), Some(end)) => Some(
                text.get(
                    usize::try_from(start).map_err(|_| invalid())?
                        ..usize::try_from(end).map_err(|_| invalid())?,
                )
                .ok_or_else(invalid)?
                .to_owned(),
            ),
            _ => None,
        };
        // Byte ranges stay half-open. A range ending just after a newline
        // occupies the preceding line, as in EntityExtent and NativeNN; the
        // graph's end cursor itself points to the next line at column zero.
        // Inspect the already selected bytes, without another source traversal.
        if searchable.as_ref().is_some_and(|text| text.ends_with('\n')) {
            values[3] = Some(
                values[3]
                    .and_then(|line| line.checked_sub(1))
                    .filter(|end| values[2].is_some_and(|start| *end >= start))
                    .ok_or_else(invalid)?,
            );
        }
        if let Some(AstGraphValue::String(metadata)) = props.get("metadata_json") {
            append_documentation(&mut searchable, metadata)?;
        }
        content.push(searchable);
        for (column, value) in ranges.iter_mut().zip(values) {
            column.push(value);
        }
    }
    let mut arrays = strings
        .into_iter()
        .map(|column| Arc::new(StringArray::from(column)) as ArrayRef)
        .collect::<Vec<_>>();
    arrays.push(Arc::new(StringArray::from(content)));
    arrays.push(Arc::new(UInt64Array::from(raw)));
    arrays.extend(
        ranges
            .into_iter()
            .map(|column| Arc::new(UInt64Array::from(column)) as ArrayRef),
    );
    original.check()?;
    RecordBatch::try_new(schema(), arrays).map_err(io::Error::other)
}
