//! Immutable FILE-sized Icebug shards. Stable row allocation belongs to the publisher.
use super::*;
use parquet::file::{reader::FileReader, serialized_reader::SerializedFileReader};
use parquet::record::Field;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileNodeShard {
    pub label: String,
    pub file: String,
    pub rows: u64,
    pub columns: BTreeMap<String, String>,
    pub languages: BTreeMap<String, u64>,
}

pub fn write_empty_node_shard(
    path: &Path,
    columns: &BTreeMap<String, String>,
    original: &SnapshotContext,
) -> io::Result<()> {
    let temporary = tempfile::NamedTempFile::new()?;
    let mut kinds = columns
        .iter()
        .map(|(name, kind)| {
            let kind = match kind.as_str() {
                "STRING" => Kind::String,
                "INT64" => Kind::Int64,
                "BOOL" => Kind::Bool,
                _ => return Err(invalid("invalid empty node column type")),
            };
            Ok((name.clone(), kind))
        })
        .collect::<io::Result<BTreeMap<_, _>>>()?;
    kinds.insert("entity_key".into(), Kind::String);
    write_nodes(
        path,
        &Table {
            file: temporary.path().to_owned(),
            columns: kinds,
            rows: 0,
            languages: BTreeMap::new(),
        },
        original,
    )
}

/// Reuse the ordinary bounded writer for precisely one FILE, without its cross-FILE edges.
pub fn write_file_nodes(
    directory: &Path,
    nodes: &[GraphNode],
    original: &Arc<SnapshotContext>,
) -> io::Result<Vec<FileNodeShard>> {
    original.check()?;
    let output = IcebugWriter::new(Arc::clone(original))?
        .push_file_ref(nodes, &[])?
        .finish()?;
    let manifest: serde_json::Value =
        serde_json::from_reader(File::open(output.directory().join("icebug.json"))?)
            .map_err(io::Error::other)?;
    let tables = manifest["node_tables"]
        .as_array()
        .ok_or_else(|| invalid("missing FILE node tables"))?;
    let mut result = Vec::new();
    for table in tables {
        original.check()?;
        let string = |name: &str| {
            table[name]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("invalid FILE node metadata"))
        };
        let file = table["shards"][0]["file"]
            .as_str()
            .ok_or_else(|| invalid("missing FILE node shard"))?
            .to_owned();
        std::fs::copy(output.directory().join(&file), directory.join(&file))?;
        OpenOptions::new()
            .write(true)
            .open(directory.join(&file))?
            .sync_all()?;
        let columns = table["columns"]
            .as_array()
            .ok_or_else(|| invalid("missing FILE columns"))?
            .iter()
            .map(|column| {
                Ok((
                    column["name"]
                        .as_str()
                        .ok_or_else(|| invalid("invalid FILE column name"))?
                        .to_owned(),
                    column["type"]
                        .as_str()
                        .ok_or_else(|| invalid("invalid FILE column type"))?
                        .to_owned(),
                ))
            })
            .collect::<io::Result<_>>()?;
        let languages = table["lang_counts"]
            .as_array()
            .ok_or_else(|| invalid("missing FILE language counts"))?
            .iter()
            .map(|entry| {
                Ok((
                    entry["lang"]
                        .as_str()
                        .ok_or_else(|| invalid("invalid FILE language"))?
                        .to_owned(),
                    entry["rows"]
                        .as_u64()
                        .ok_or_else(|| invalid("invalid FILE language rows"))?,
                ))
            })
            .collect::<io::Result<_>>()?;
        result.push(FileNodeShard {
            label: string("label")?,
            file,
            rows: table["rows"]
                .as_u64()
                .ok_or_else(|| invalid("invalid FILE row count"))?,
            columns,
            languages,
        });
    }
    Ok(result)
}

pub fn visit_node_shard(
    path: &Path,
    label: &str,
    original: &SnapshotContext,
    mut visitor: impl FnMut(GraphNode) -> io::Result<()>,
) -> io::Result<()> {
    original.check()?;
    original.check()?;
    let reader = SerializedFileReader::new(File::open(path)?).map_err(io::Error::other)?;
    for row in reader.get_row_iter(None).map_err(io::Error::other)? {
        original.check()?;
        let row = row.map_err(io::Error::other)?;
        let mut properties = BTreeMap::new();
        let mut key = None;
        for (name, value) in row.get_column_iter() {
            if name == "entity_key" {
                let Field::Str(value) = value else {
                    return Err(invalid("invalid FILE primary key"));
                };
                key = Some(value.clone());
                continue;
            }
            let value = match value {
                Field::Null => continue,
                Field::Str(value) => GraphValue::String(value.clone()),
                Field::Long(value) => GraphValue::Int64(*value),
                Field::Bool(value) => GraphValue::Bool(*value),
                _ => return Err(invalid("unsupported FILE property")),
            };
            properties.insert(name.clone(), value);
        }
        visitor(GraphNode {
            label: label.to_owned(),
            key: key.ok_or_else(|| invalid("missing FILE primary key"))?,
            properties,
        })?;
    }
    original.check()
}

/// Nonpublic compiler facts stay in Parquet, outside the mounted Ladybug graph.
pub fn write_fact_rows(
    path: &Path,
    rows: impl IntoIterator<Item = io::Result<String>>,
    original: &SnapshotContext,
) -> io::Result<()> {
    let mut writer = file_writer(path, [("payload".into(), Kind::String, false)])?;
    let mut rows = rows.into_iter();
    loop {
        let mut values = Vec::new();
        let mut bytes = 0usize;
        for row in rows.by_ref().take(128) {
            original.check()?;
            let row = row?;
            bytes = bytes.saturating_add(row.len());
            values.push(ByteArray::from(row.into_bytes()));
            if bytes >= 8 * 1024 * 1024 {
                break;
            }
        }
        if values.is_empty() {
            break;
        }
        let mut group = writer.next_row_group().map_err(io::Error::other)?;
        let mut column = group
            .next_column()
            .map_err(io::Error::other)?
            .ok_or_else(|| invalid("missing fact payload column"))?;
        column
            .typed::<ByteArrayType>()
            .write_batch(&values, None, None)
            .map_err(io::Error::other)?;
        column.close().map_err(io::Error::other)?;
        group.close().map_err(io::Error::other)?;
    }
    writer.close().map_err(io::Error::other)?;
    OpenOptions::new().write(true).open(path)?.sync_all()?;
    original.check()
}

pub fn visit_fact_rows(
    path: &Path,
    original: &SnapshotContext,
    mut visitor: impl FnMut(&str) -> io::Result<()>,
) -> io::Result<()> {
    original.check()?;
    let reader = SerializedFileReader::new(File::open(path)?).map_err(io::Error::other)?;
    for row in reader.get_row_iter(None).map_err(io::Error::other)? {
        original.check()?;
        let row = row.map_err(io::Error::other)?;
        let mut columns = row.get_column_iter();
        match (columns.next(), columns.next()) {
            (Some((name, Field::Str(payload))), None) if name == "payload" => visitor(payload)?,
            _ => return Err(invalid("invalid fact payload row")),
        }
    }
    original.check()
}

/// Write one source-shard/target-shard member with local offsets and one indptr row group.
pub fn write_csr_shard(
    directory: &Path,
    source_rows: u64,
    target_rows: u64,
    rows: &[(u64, u64, String)],
    original: &Arc<SnapshotContext>,
) -> io::Result<()> {
    let staging = tempfile::tempdir()?;
    let database = Connection::open(staging.path().join("edges.sqlite"))
        .map_err(|error| sqlite_error(original, error))?;
    database.execute_batch("PRAGMA temp_store=FILE; CREATE TABLE csr(kind TEXT,source_label TEXT,target_label TEXT,source_row INTEGER,target_row INTEGER,uid TEXT UNIQUE);").map_err(|error| sqlite_error(original,error))?;
    let cancellation = Arc::clone(original);
    database
        .progress_handler(1024, Some(move || cancellation.check().is_err()))
        .map_err(|error| sqlite_error(original, error))?;
    for (source, target, uid) in rows {
        original.check()?;
        if *source >= source_rows || *target >= target_rows || uid.is_empty() || uid.contains('\0')
        {
            return Err(invalid("invalid FILE CSR endpoint"));
        }
        database
            .execute(
                "INSERT INTO csr VALUES ('','','',?1,?2,?3)",
                params![
                    i64::try_from(*source).map_err(|_| invalid("source row overflow"))?,
                    i64::try_from(*target).map_err(|_| invalid("target row overflow"))?,
                    uid
                ],
            )
            .map_err(|error| sqlite_error(original, error))?;
    }
    write_edges(
        &directory.join("edges.indices"),
        &database,
        ("", "", ""),
        original,
    )?;
    write_indptr(
        &directory.join("edges.indptr"),
        source_rows,
        &database,
        ("", "", ""),
        original,
    )?;
    OpenOptions::new()
        .write(true)
        .open(directory.join("edges.indices"))?
        .sync_all()?;
    OpenOptions::new()
        .write(true)
        .open(directory.join("edges.indptr"))?
        .sync_all()?;
    original.check()
}
