//! Icebug generation from per-file semantic rows. Publication remains with Core.
use graphit_core::artifact_store::SnapshotContext;
use parquet::{
    basic::{Compression, LogicalType, Repetition, Type as PhysicalType},
    data_type::{BoolType, ByteArray, ByteArrayType, Int64Type},
    file::{
        metadata::KeyValue,
        properties::{WriterProperties, WriterVersion},
        writer::SerializedFileWriter,
    },
    schema::types::Type,
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

mod delta;
pub use delta::{
    FileNodeShard, visit_fact_rows, visit_node_shard, write_csr_shard, write_empty_node_shard,
    write_fact_rows, write_file_nodes,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GraphValue {
    String(String),
    Int64(i64),
    Bool(bool),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub label: String,
    pub key: String,
    pub properties: BTreeMap<String, GraphValue>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub kind: String,
    pub source_key: String,
    pub target_key: String,
    pub uid: String,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    String,
    Int64,
    UInt64,
    Bool,
}
impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::String => "STRING",
            Self::Int64 => "INT64",
            Self::UInt64 => "UINT64",
            Self::Bool => "BOOL",
        }
    }
}
impl GraphValue {
    fn kind(&self) -> Kind {
        match self {
            Self::String(_) => Kind::String,
            Self::Int64(_) => Kind::Int64,
            Self::Bool(_) => Kind::Bool,
        }
    }
}
struct Table {
    file: PathBuf,
    columns: BTreeMap<String, Kind>,
    rows: u64,
    languages: BTreeMap<String, u64>,
}

/// Private unpublished staging. Only table schema remains in memory between FILEs.
pub struct IcebugWriter {
    database: Connection,
    staging: tempfile::TempDir,
    tables: BTreeMap<String, Table>,
    original: Arc<SnapshotContext>,
}
pub struct IcebugOutput {
    staging: tempfile::TempDir,
    original: Arc<SnapshotContext>,
}
impl IcebugOutput {
    /// Borrow the real creation context for sidecars published in this same output.
    pub fn original(&self) -> &Arc<SnapshotContext> {
        &self.original
    }
    pub fn directory(&self) -> PathBuf {
        self.staging.path().join("output")
    }
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn sqlite_error(original: &SnapshotContext, error: rusqlite::Error) -> io::Error {
    original
        .check()
        .err()
        .unwrap_or_else(|| io::Error::other(error))
}
fn checked_name(value: &str) -> io::Result<()> {
    if value.is_empty() || value.contains('\0') {
        return Err(invalid("empty or invalid graph identifier"));
    }
    Ok(())
}
fn quoted(value: &str) -> String {
    format!("`{}`", value.replace('`', "``"))
}
impl IcebugWriter {
    pub fn new(original: Arc<SnapshotContext>) -> io::Result<Self> {
        original.check()?;
        let staging = tempfile::tempdir()?;
        std::fs::create_dir(staging.path().join("output"))?;
        let database = Connection::open(staging.path().join("graph.sqlite"))
            .map_err(|error| sqlite_error(&original, error))?;
        database.execute_batch(
            "PRAGMA temp_store=FILE;
             CREATE TABLE entities (key TEXT PRIMARY KEY, label TEXT NOT NULL, row INTEGER NOT NULL);
             CREATE TABLE edges (kind TEXT NOT NULL, source TEXT NOT NULL, target TEXT NOT NULL, uid TEXT NOT NULL);
             CREATE INDEX edges_source ON edges(source, kind, target, uid);"
        ).map_err(|error| sqlite_error(&original, error))?;
        let cancellation = Arc::clone(&original);
        database
            .progress_handler(1024, Some(move || cancellation.check().is_err()))
            .map_err(|error| sqlite_error(&original, error))?;
        Ok(Self {
            database,
            staging,
            tables: BTreeMap::new(),
            original,
        })
    }
    /// The private job database also stages deferred semantic rows. Callers must
    /// finish their joins before publication; this connection never escapes the job.
    pub fn staging_database(&self) -> io::Result<&Connection> {
        self.original.check()?;
        Ok(&self.database)
    }

    /// A live FILE export includes durable unresolved intentions. After all
    /// FILEs have been staged, keep only edges whose endpoints are published.
    /// Ordinary writers retain their strict missing-endpoint validation.
    pub fn retain_resolved_edges(self) -> io::Result<Self> {
        self.original.check()?;
        self.database
            .execute_batch(
                "DELETE FROM edges WHERE NOT EXISTS (SELECT 1 FROM entities WHERE key=edges.source)
                OR NOT EXISTS (SELECT 1 FROM entities WHERE key=edges.target)",
            )
            .map_err(|error| sqlite_error(&self.original, error))?;
        self.original.check()?;
        Ok(self)
    }

    pub fn push_file(
        self,
        nodes: impl IntoIterator<Item = GraphNode>,
        edges: impl IntoIterator<Item = GraphEdge>,
    ) -> io::Result<Self> {
        let nodes = nodes.into_iter().collect::<Vec<_>>();
        let edges = edges.into_iter().collect::<Vec<_>>();
        self.push_file_ref(&nodes, &edges)
    }

    /// Stage an already assembled FILE without cloning its graph properties.
    /// The same owned rows can subsequently enter the live native transaction.
    pub fn push_file_ref(mut self, nodes: &[GraphNode], edges: &[GraphEdge]) -> io::Result<Self> {
        // Consuming staging on error prevents publication of a partial FILE.
        let transaction = self
            .database
            .transaction()
            .map_err(|error| sqlite_error(&self.original, error))?;
        let mut insert_node = transaction
            .prepare_cached("INSERT INTO entities (key,label,row) VALUES (?1,?2,?3)")
            .map_err(|error| sqlite_error(&self.original, error))?;
        let mut files = BTreeMap::<String, BufWriter<File>>::new();
        for node in nodes {
            self.original.check()?;
            checked_name(&node.label)?;
            checked_name(&node.key)?;
            if node.properties.contains_key("entity_key") {
                return Err(invalid("graph primary key is supplied separately"));
            }
            let next = self.tables.len();
            let table = self
                .tables
                .entry(node.label.clone())
                .or_insert_with(|| Table {
                    file: self.staging.path().join(format!("nodes-{next}.jsonl")),
                    columns: BTreeMap::new(),
                    rows: 0,
                    languages: BTreeMap::new(),
                });
            insert_node
                .execute(params![
                    node.key,
                    node.label,
                    i64::try_from(table.rows).map_err(|_| invalid("graph row offset overflow"))?
                ])
                .map_err(|error| sqlite_error(&self.original, error))?;
            for (name, value) in &node.properties {
                checked_name(name)?;
                if table
                    .columns
                    .insert(name.clone(), value.kind())
                    .is_some_and(|old| old != value.kind())
                {
                    return Err(invalid("inconsistent graph column type"));
                }
            }
            table.columns.insert("entity_key".into(), Kind::String);
            if let Some(GraphValue::String(language)) = node.properties.get("lang") {
                let count = table.languages.entry(language.clone()).or_default();
                *count = count
                    .checked_add(1)
                    .ok_or_else(|| invalid("graph row overflow"))?;
            }
            table.rows = table
                .rows
                .checked_add(1)
                .ok_or_else(|| invalid("graph row overflow"))?;
            let file = match files.entry(node.label.clone()) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) => entry.insert(BufWriter::new(
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&table.file)?,
                )),
            };
            use serde::ser::{SerializeMap, Serializer};
            let mut serializer = serde_json::Serializer::new(&mut *file);
            let mut properties = serializer.serialize_map(Some(node.properties.len() + 1))?;
            properties.serialize_entry("entity_key", &node.key)?;
            for (name, value) in &node.properties {
                properties.serialize_entry(name, value)?;
            }
            properties.end()?;
            file.write_all(b"\n")?;
        }
        for file in files.values_mut() {
            file.flush()?;
        }
        drop(insert_node);
        let mut insert_edge = transaction
            .prepare_cached("INSERT INTO edges (kind,source,target,uid) VALUES (?1,?2,?3,?4)")
            .map_err(|error| sqlite_error(&self.original, error))?;
        for edge in edges {
            self.original.check()?;
            checked_name(&edge.kind)?;
            checked_name(&edge.uid)?;
            insert_edge
                .execute(params![
                    edge.kind,
                    edge.source_key,
                    edge.target_key,
                    edge.uid
                ])
                .map_err(|error| sqlite_error(&self.original, error))?;
        }
        drop(insert_edge);
        transaction
            .commit()
            .map_err(|error| sqlite_error(&self.original, error))?;
        self.original.check()?;
        Ok(self)
    }
    pub fn finish(self) -> io::Result<IcebugOutput> {
        self.original.check()?;
        let output = self.staging.path().join("output");
        let mut node_tables = Vec::new();
        let mut schema = String::new();
        for (index, (label, table)) in self.tables.iter().enumerate() {
            self.original.check()?;
            let filename = format!("nodes-{index}.parquet");
            write_nodes(&output.join(&filename), table, &self.original)?;
            let columns: Vec<_> = table
                .columns
                .iter()
                .map(|(name, kind)| json!({"name":name,"type":kind.name()}))
                .collect();
            node_tables.push(json!({"label":label,"shards":[{"file":filename,"row_base":0,"rows":table.rows}],"rows":table.rows,"primary_key":"entity_key","columns":columns,"lang_counts":table.languages.iter().map(|(lang,rows)|json!({"lang":lang,"rows":rows})).collect::<Vec<_>>() }));
            let fields = table
                .columns
                .iter()
                .map(|(name, kind)| format!("{} {}", quoted(name), kind.name()))
                .collect::<Vec<_>>()
                .join(", ");
            schema.push_str(&format!(
                "CREATE NODE TABLE {} ({fields}, PRIMARY KEY (entity_key));\n",
                quoted(label)
            ));
        }
        let missing_endpoint: bool = self.database.query_row(
            "SELECT EXISTS(SELECT 1 FROM edges e LEFT JOIN entities s ON s.key=e.source LEFT JOIN entities t ON t.key=e.target WHERE s.key IS NULL OR t.key IS NULL)",
            [], |row| row.get(0),
        ).map_err(|error| sqlite_error(&self.original, error))?;
        if missing_endpoint {
            return Err(invalid("missing graph edge endpoint"));
        }
        self.database.execute_batch(
            "CREATE TABLE csr (kind TEXT NOT NULL, source_label TEXT NOT NULL, target_label TEXT NOT NULL,
               source_row INTEGER NOT NULL, target_row INTEGER NOT NULL, uid TEXT NOT NULL,
               UNIQUE(kind,source_label,target_label,uid));
             INSERT INTO csr SELECT e.kind,s.label,t.label,s.row,t.row,e.uid FROM edges e
               JOIN entities s ON s.key=e.source JOIN entities t ON t.key=e.target;
             CREATE INDEX csr_order ON csr(kind,source_label,target_label,source_row,target_row,uid);
             DROP TABLE edges; DROP TABLE entities;"
        ).map_err(|error| sqlite_error(&self.original,error))?;
        let mut member_statement = self.database.prepare(
            "SELECT DISTINCT kind,source_label,target_label FROM csr ORDER BY kind,source_label,target_label"
        ).map_err(|error| sqlite_error(&self.original, error))?;
        let members = member_statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|error| sqlite_error(&self.original, error))?;
        let mut relationships = BTreeMap::<String, Vec<serde_json::Value>>::new();
        let mut n_edges = 0u64;
        for (index, member) in members.enumerate() {
            self.original.check()?;
            let (kind, from, to) = member.map_err(|error| sqlite_error(&self.original, error))?;
            let stem = format!("edges-{index}");
            let indices = format!("{stem}.indices");
            let indptr = format!("{stem}.indptr");
            let rows = write_edges(
                &output.join(&indices),
                &self.database,
                (&kind, &from, &to),
                &self.original,
            )?;
            n_edges = n_edges
                .checked_add(rows)
                .ok_or_else(|| invalid("graph edge overflow"))?;
            write_indptr(
                &output.join(&indptr),
                self.tables[&from].rows,
                &self.database,
                (&kind, &from, &to),
                &self.original,
            )?;
            relationships.entry(kind).or_default().push(json!({"from":from,"to":to,"table":stem,"indices":indices,"indptr":indptr,"rows":rows,"source_row_base":0,"source_rows":self.tables[&from].rows,"target_row_base":0,"target_rows":self.tables[&to].rows}));
        }
        for (kind, members) in &relationships {
            let endpoints = members
                .iter()
                .map(|member| {
                    format!(
                        "FROM {} TO {}",
                        quoted(member["from"].as_str().unwrap()),
                        quoted(member["to"].as_str().unwrap())
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            schema.push_str(&format!(
                "CREATE REL TABLE GROUP {} ({endpoints}, uid STRING);\n",
                quoted(kind)
            ));
        }
        std::fs::write(output.join("schema.cypher"), schema)?;
        self.original.check()?;
        let manifest = json!({"manifest_version":3,"format":"icebug-canonical","storage":"direct-parquet","finished":true,"add_reverse_edges":false,"schema_file":"schema.cypher","node_tables":node_tables,"relationships":relationships.into_iter().map(|(kind,members)|json!({"type":kind,"members":members})).collect::<Vec<_>>(),"relation_uids":true,"generation":ulid::Ulid::generate().to_string(),"n_edges":n_edges,"repaired_strings":0,"invariants":{"indptr_row_groups":1,"self_loops":"forward-once"}});
        serde_json::to_writer(File::create(output.join("icebug.json"))?, &manifest)?;
        self.original.check()?;
        Ok(IcebugOutput {
            staging: self.staging,
            original: self.original,
        })
    }
}

fn file_writer(
    path: &Path,
    columns: impl IntoIterator<Item = (String, Kind, bool)>,
) -> io::Result<SerializedFileWriter<File>> {
    let mut fields = Vec::new();
    for (name, kind, optional) in columns {
        let physical = match kind {
            Kind::String => PhysicalType::BYTE_ARRAY,
            Kind::Int64 | Kind::UInt64 => PhysicalType::INT64,
            Kind::Bool => PhysicalType::BOOLEAN,
        };
        let mut builder =
            Type::primitive_type_builder(&name, physical).with_repetition(if optional {
                Repetition::OPTIONAL
            } else {
                Repetition::REQUIRED
            });
        if kind == Kind::String {
            builder = builder.with_logical_type(Some(LogicalType::String));
        } else if kind == Kind::UInt64 {
            builder = builder.with_logical_type(Some(LogicalType::integer(64, false)));
        }
        fields.push(Arc::new(builder.build().map_err(io::Error::other)?));
    }
    let schema = Arc::new(
        Type::group_type_builder("graph")
            .with_fields(fields)
            .build()
            .map_err(io::Error::other)?,
    );
    let properties = Arc::new(
        WriterProperties::builder()
            .set_writer_version(WriterVersion::PARQUET_1_0)
            .set_dictionary_enabled(false)
            .set_compression(Compression::ZSTD(Default::default()))
            .set_key_value_metadata(Some(vec![KeyValue::new(
                "icebug_disk_version".into(),
                Some("v1".into()),
            )]))
            .build(),
    );
    SerializedFileWriter::new(File::create(path)?, schema, properties).map_err(io::Error::other)
}
fn write_nodes(path: &Path, table: &Table, original: &SnapshotContext) -> io::Result<()> {
    let mut writer = file_writer(
        path,
        table
            .columns
            .iter()
            .map(|(name, kind)| (name.clone(), *kind, name != "entity_key")),
    )?;
    // Native scans materialize a row-group batch before applying predicates. Bound
    // the spool batch by bytes as well as rows, without rejecting a larger row.
    const GROUP_INPUT_BYTES: usize = 8 * 1024 * 1024;
    let mut spool = BufReader::new(File::open(&table.file)?);
    let mut line = String::new();
    loop {
        let mut rows = Vec::new();
        let mut input_bytes = 0;
        while rows.len() < 1024 {
            original.check()?;
            if line.is_empty() && spool.read_line(&mut line)? == 0 {
                break;
            }
            if !rows.is_empty() && line.len() > GROUP_INPUT_BYTES.saturating_sub(input_bytes) {
                break;
            }
            input_bytes += line.len();
            rows.push(
                serde_json::from_str::<BTreeMap<String, GraphValue>>(&line)
                    .map_err(io::Error::other)?,
            );
            line.clear();
        }
        if rows.is_empty() {
            break;
        }
        original.check()?;
        let mut group = writer.next_row_group().map_err(io::Error::other)?;
        for (name, kind) in &table.columns {
            original.check()?;
            let mut column = group
                .next_column()
                .map_err(io::Error::other)?
                .ok_or_else(|| invalid("missing parquet column"))?;
            let levels: Vec<i16> = rows
                .iter()
                .map(|row| i16::from(row.contains_key(name)))
                .collect();
            let definitions = (name != "entity_key").then_some(levels.as_slice());
            match kind {
                Kind::String => {
                    let values: Vec<_> = rows
                        .iter()
                        .filter_map(|row| match row.get(name) {
                            Some(GraphValue::String(value)) => {
                                Some(ByteArray::from(value.as_str()))
                            }
                            _ => None,
                        })
                        .collect();
                    column
                        .typed::<ByteArrayType>()
                        .write_batch(&values, definitions, None)
                        .map_err(io::Error::other)?;
                }
                Kind::Int64 | Kind::UInt64 => {
                    let values: Vec<_> = rows
                        .iter()
                        .filter_map(|row| match row.get(name) {
                            Some(GraphValue::Int64(value)) => Some(*value),
                            _ => None,
                        })
                        .collect();
                    column
                        .typed::<Int64Type>()
                        .write_batch(&values, definitions, None)
                        .map_err(io::Error::other)?;
                }
                Kind::Bool => {
                    let values: Vec<_> = rows
                        .iter()
                        .filter_map(|row| match row.get(name) {
                            Some(GraphValue::Bool(value)) => Some(*value),
                            _ => None,
                        })
                        .collect();
                    column
                        .typed::<BoolType>()
                        .write_batch(&values, definitions, None)
                        .map_err(io::Error::other)?;
                }
            }
            column.close().map_err(io::Error::other)?;
        }
        group.close().map_err(io::Error::other)?;
    }
    writer.close().map_err(io::Error::other)?;
    original.check()
}
fn write_edges(
    path: &Path,
    database: &Connection,
    member: (&str, &str, &str),
    original: &SnapshotContext,
) -> io::Result<u64> {
    let mut writer = file_writer(
        path,
        [
            ("target".into(), Kind::UInt64, false),
            ("uid".into(), Kind::String, false),
        ],
    )?;
    let mut statement = database.prepare(
        "SELECT target_row,uid FROM csr WHERE kind=?1 AND source_label=?2 AND target_label=?3 ORDER BY source_row,target_row,uid"
    ).map_err(|error| sqlite_error(original,error))?;
    let mut rows = statement
        .query(params![member.0, member.1, member.2])
        .map_err(|error| sqlite_error(original, error))?;
    let mut count = 0u64;
    loop {
        let mut targets = Vec::with_capacity(1024);
        let mut uids = Vec::with_capacity(1024);
        for _ in 0..1024 {
            original.check()?;
            let Some(row) = rows.next().map_err(|error| sqlite_error(original, error))? else {
                break;
            };
            targets.push(
                row.get::<_, i64>(0)
                    .map_err(|error| sqlite_error(original, error))?,
            );
            let uid: String = row.get(1).map_err(|error| sqlite_error(original, error))?;
            uids.push(ByteArray::from(uid.into_bytes()));
        }
        if targets.is_empty() {
            break;
        }
        count = count
            .checked_add(targets.len() as u64)
            .ok_or_else(|| invalid("graph edge overflow"))?;
        let mut group = writer.next_row_group().map_err(io::Error::other)?;
        let mut column = group
            .next_column()
            .map_err(io::Error::other)?
            .ok_or_else(|| invalid("missing edge target column"))?;
        column
            .typed::<Int64Type>()
            .write_batch(&targets, None, None)
            .map_err(io::Error::other)?;
        column.close().map_err(io::Error::other)?;
        let mut column = group
            .next_column()
            .map_err(io::Error::other)?
            .ok_or_else(|| invalid("missing edge uid column"))?;
        column
            .typed::<ByteArrayType>()
            .write_batch(&uids, None, None)
            .map_err(io::Error::other)?;
        column.close().map_err(io::Error::other)?;
        group.close().map_err(io::Error::other)?;
    }
    writer.close().map_err(io::Error::other)?;
    original.check()?;
    Ok(count)
}
fn write_indptr(
    path: &Path,
    rows: u64,
    database: &Connection,
    member: (&str, &str, &str),
    original: &SnapshotContext,
) -> io::Result<()> {
    let rows = i64::try_from(rows).map_err(|_| invalid("graph row offset overflow"))?;
    let mut writer = file_writer(path, [("ptr".into(), Kind::UInt64, false)])?;
    let mut group = writer.next_row_group().map_err(io::Error::other)?;
    let mut column = group
        .next_column()
        .map_err(io::Error::other)?
        .ok_or_else(|| invalid("missing edge ptr column"))?;
    let mut statement = database.prepare(
        "SELECT source_row,COUNT(*) FROM csr WHERE kind=?1 AND source_label=?2 AND target_label=?3 GROUP BY source_row ORDER BY source_row"
    ).map_err(|error| sqlite_error(original,error))?;
    let mut counts = statement
        .query_map(params![member.0, member.1, member.2], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|error| sqlite_error(original, error))?;
    let mut next = counts
        .next()
        .transpose()
        .map_err(|error| sqlite_error(original, error))?;
    let mut offset = 0i64;
    // Icebug requires one indptr row group; write bounded chunks into that group.
    let mut chunk = Vec::with_capacity(1024);
    for row in 0..=rows {
        original.check()?;
        while let Some((source, count)) = next.filter(|(source, _)| *source < row) {
            if source < 0 || source >= rows {
                return Err(invalid("graph edge source outside table"));
            }
            offset = offset
                .checked_add(count)
                .ok_or_else(|| invalid("graph edge offset overflow"))?;
            next = counts
                .next()
                .transpose()
                .map_err(|error| sqlite_error(original, error))?;
        }
        chunk.push(offset);
        if chunk.len() == 1024 {
            column
                .typed::<Int64Type>()
                .write_batch(&chunk, None, None)
                .map_err(io::Error::other)?;
            chunk.clear();
        }
    }
    if next.is_some() {
        return Err(invalid("graph edge source outside table"));
    }
    column
        .typed::<Int64Type>()
        .write_batch(&chunk, None, None)
        .map_err(io::Error::other)?;
    column.close().map_err(io::Error::other)?;
    group.close().map_err(io::Error::other)?;
    writer.close().map_err(io::Error::other)?;
    original.check()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GraphSampleLimits;
    use crate::tests::{capture, context, policy};

    fn node(key: &str, name: Option<&str>) -> GraphNode {
        let mut properties = BTreeMap::from([
            ("uid".into(), GraphValue::String("same genuine uid".into())),
            ("data_key".into(), GraphValue::String(key.into())),
            ("path".into(), GraphValue::String("ação.rs".into())),
            ("lang".into(), GraphValue::String("rust".into())),
            (
                "raw_version".into(),
                GraphValue::String(u64::MAX.to_string()),
            ),
            ("line_number".into(), GraphValue::Int64(7)),
            ("exported".into(), GraphValue::Bool(true)),
        ]);
        if let Some(name) = name {
            properties.insert("name".into(), GraphValue::String(name.into()));
        }
        GraphNode {
            label: "Function".into(),
            key: serde_json::to_string(&["same genuine uid", key]).unwrap(),
            properties,
        }
    }
    #[test]
    fn produced_icebug_reads_real_nodes_nullable_columns_and_csr() {
        let first = node("name", Some("ação 🚀"));
        let second = node("body", None);
        let edge = GraphEdge {
            kind: "CALLS".into(),
            source_key: first.key.clone(),
            target_key: second.key.clone(),
            uid: "call-1".into(),
        };
        let output = IcebugWriter::new(context())
            .unwrap()
            .push_file([first], [])
            .unwrap()
            .push_file([second], [edge])
            .unwrap()
            .finish()
            .unwrap();
        let graph = capture(&output.directory(), context(), policy()).unwrap();
        let sample = graph
            .sample_graph(GraphSampleLimits::new(2, 4, 10, 8192, 100, 100, 8).unwrap())
            .unwrap();
        assert_eq!(sample.nodes.len(), 2);
        assert_eq!(sample.edges.len(), 1);
        assert_ne!(sample.edges[0].from(), sample.edges[0].to());
        assert_eq!(sample.edges[0].uid(), "call-1");
        assert!(
            sample
                .nodes
                .iter()
                .any(|node| node.name() == Some("ação 🚀"))
        );
        assert!(sample.nodes.iter().any(|node| node.name().is_none()));
        assert!(sample.nodes.iter().all(|node| node.line() == Some(7)));
        let manifest: serde_json::Value =
            serde_json::from_reader(File::open(output.directory().join("icebug.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["node_tables"][0]["rows"], 2);
        assert_eq!(manifest["node_tables"][0]["primary_key"], "entity_key");
        assert_eq!(manifest["n_edges"], 1);
    }
    #[test]
    fn wide_node_groups_preserve_order_and_an_individually_larger_row() {
        use parquet::file::reader::{FileReader, SerializedFileReader};
        use parquet::record::RowAccessor;

        let output = IcebugWriter::new(context())
            .unwrap()
            .push_file(
                (0..16).map(|index| GraphNode {
                    label: "Wide".into(),
                    key: format!("key-{index}"),
                    properties: BTreeMap::from([(
                        "payload".into(),
                        GraphValue::String(format!(
                            "{}-{index}",
                            "x".repeat(if index == 9 { 9 } else { 1 } * 1024 * 1024)
                        )),
                    )]),
                }),
                [],
            )
            .unwrap()
            .finish()
            .unwrap();
        let manifest: serde_json::Value =
            serde_json::from_reader(File::open(output.directory().join("icebug.json")).unwrap())
                .unwrap();
        let file = manifest["node_tables"][0]["shards"][0]["file"]
            .as_str()
            .unwrap();
        let reader =
            SerializedFileReader::new(File::open(output.directory().join(file)).unwrap()).unwrap();
        assert_eq!(reader.metadata().file_metadata().num_rows(), 16);
        assert!(reader.num_row_groups() > 1);
        for group in reader.metadata().row_groups() {
            assert!(group.num_rows() == 1 || group.total_byte_size() < 8 * 1024 * 1024);
        }
        for (index, row) in reader.get_row_iter(None).unwrap().enumerate() {
            let row = row.unwrap();
            assert_eq!(row.get_string(0).unwrap(), &format!("key-{index}"));
            assert_eq!(
                row.get_string(1).unwrap(),
                &format!(
                    "{}-{index}",
                    "x".repeat(if index == 9 { 9 } else { 1 } * 1024 * 1024)
                )
            );
        }
    }
    #[test]
    fn duplicate_entity_and_type_conflict_discard_incomplete_staging() {
        let writer = IcebugWriter::new(context()).unwrap();
        let path = writer.staging.path().to_owned();
        assert!(
            writer
                .push_file([node("name", None), node("name", None)], [])
                .is_err()
        );
        assert!(!path.exists());
        let writer = IcebugWriter::new(context())
            .unwrap()
            .push_file([node("name", None)], [])
            .unwrap();
        let path = writer.staging.path().to_owned();
        let mut conflicting = node("body", None);
        conflicting
            .properties
            .insert("line_number".into(), GraphValue::String("7".into()));
        assert!(writer.push_file([conflicting], []).is_err());
        assert!(!path.exists());
    }
    #[test]
    fn missing_endpoint_discards_unpublished_graph() {
        let writer = IcebugWriter::new(context()).unwrap();
        let path = writer.staging.path().to_owned();
        let source = node("name", None);
        let edge = GraphEdge {
            kind: "CALLS".into(),
            source_key: source.key.clone(),
            target_key: "absent".into(),
            uid: "call-1".into(),
        };
        assert!(
            writer
                .push_file([source], [edge])
                .unwrap()
                .finish()
                .is_err()
        );
        assert!(!path.exists());
    }
    #[test]
    fn spilled_edges_resolve_later_files_and_cross_parquet_groups_in_order() {
        let root = node("root", Some("root"));
        let targets: Vec<_> = (0..1025)
            .map(|index| node(&format!("target-{index}"), None))
            .collect();
        let edges = targets
            .iter()
            .enumerate()
            .rev()
            .map(|(index, target)| GraphEdge {
                kind: "CALLS".into(),
                source_key: root.key.clone(),
                target_key: target.key.clone(),
                uid: format!("call-{index}"),
            })
            .collect::<Vec<_>>();
        let output = IcebugWriter::new(context())
            .unwrap()
            .push_file([root], edges)
            .unwrap()
            .push_file(targets, [])
            .unwrap()
            .finish()
            .unwrap();
        let graph = capture(&output.directory(), context(), policy()).unwrap();
        let sample = graph
            .sample_graph(GraphSampleLimits::new(1, 1025, 2051, 8192, 50000, 50000, 8).unwrap())
            .unwrap();
        assert_eq!(sample.nodes().len(), 1026);
        assert_eq!(sample.edges().len(), 1025);
        assert_eq!(sample.edges()[0].uid(), "call-0");
        assert_eq!(sample.edges()[1024].uid(), "call-1024");
    }
    #[test]
    fn duplicate_relationship_uid_discards_unpublished_graph() {
        let source = node("source", None);
        let target = node("target", None);
        let edges = (0..2)
            .map(|_| GraphEdge {
                kind: "CALLS".into(),
                source_key: source.key.clone(),
                target_key: target.key.clone(),
                uid: "duplicate".into(),
            })
            .collect::<Vec<_>>();
        let writer = IcebugWriter::new(context()).unwrap();
        let path = writer.staging.path().to_owned();
        assert!(
            writer
                .push_file([source, target], edges)
                .unwrap()
                .finish()
                .is_err()
        );
        assert!(!path.exists());
    }
}
