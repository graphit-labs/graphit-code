use crate::{
    error::{DataReadError, Result, add, bounded},
    policy::LocalGraphLimits,
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    #[serde(default, rename = "live")]
    pub(crate) live: Option<LiveMetadata>,
    pub(crate) manifest_version: u32,
    pub(crate) format: String,
    pub(crate) storage: String,
    pub(crate) finished: bool,
    pub(crate) add_reverse_edges: bool,
    pub(crate) schema_file: String,
    pub(crate) node_tables: Vec<NodeTable>,
    pub(crate) relationships: Vec<Relationship>,
    pub(crate) relation_uids: bool,
    pub(crate) n_edges: u64,
    pub(crate) repaired_strings: u64,
    pub(crate) invariants: Invariants,
}

/// Data consumes the publication namespace; the AST codec owns the FILE catalog.
/// Serde validates and skips the other JSON values without retaining their tree.
pub(crate) struct LiveMetadata {
    pub(crate) namespace: Option<[u8; 32]>,
}

impl<'de> Deserialize<'de> for LiveMetadata {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            namespace: Option<[u8; 32]>,
        }
        struct Object;
        impl<'de> serde::de::Visitor<'de> for Object {
            type Value = LiveMetadata;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a local graph metadata object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                map: M,
            ) -> std::result::Result<Self::Value, M::Error> {
                let fields =
                    Fields::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(LiveMetadata {
                    namespace: fields.namespace,
                })
            }
        }
        deserializer.deserialize_map(Object)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NodeTable {
    pub(crate) label: String,
    pub(crate) shards: Vec<NodeShard>,
    pub(crate) rows: u64,
    pub(crate) primary_key: String,
    pub(crate) columns: Vec<Column>,
    pub(crate) lang_counts: Vec<LangCount>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NodeShard {
    pub(crate) file: String,
    /// Physical Parquet row ordinal, independent of the public logical row base.
    #[serde(default)]
    pub(crate) row_offset: u64,
    pub(crate) row_base: u64,
    pub(crate) rows: u64,
}
impl NodeTable {
    pub(crate) fn row_extent(&self) -> u64 {
        self.shards
            .iter()
            .map(|part| part.row_base + part.rows)
            .max()
            .unwrap_or(0)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Column {
    pub(crate) name: String,
    #[serde(rename = "type")]
    pub(crate) kind: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LangCount {
    pub(crate) lang: String,
    pub(crate) rows: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Relationship {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) members: Vec<Member>,
    #[serde(default)]
    pub(crate) reverse_members: Vec<Member>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Member {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) table: String,
    pub(crate) indices: String,
    pub(crate) indptr: String,
    #[serde(default)]
    pub(crate) indices_row_offset: u64,
    #[serde(default)]
    pub(crate) indptr_row_offset: u64,
    pub(crate) source_row_base: u64,
    pub(crate) source_rows: u64,
    pub(crate) target_row_base: u64,
    pub(crate) target_rows: u64,
    pub(crate) rows: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Invariants {
    pub(crate) indptr_row_groups: u32,
    pub(crate) self_loops: String,
}

pub(crate) fn relative(value: &str) -> Result<&Path> {
    let path = Path::new(value);
    if value.is_empty()
        || value.contains(['\\', '\0'])
        || value.contains(':')
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(DataReadError::InvalidManifest("relative file"));
    }
    Ok(path)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StorageRole<'a> {
    Manifest,
    Schema,
    Nodes(&'a str),
    Indices(&'a str),
    Indptr,
}

#[derive(Default)]
struct StorageFiles<'a> {
    files: BTreeMap<&'a str, PhysicalFile<'a>>,
}

struct PhysicalFile<'a> {
    role: StorageRole<'a>,
    ranges: BTreeMap<u64, u64>,
}

impl<'a> StorageFiles<'a> {
    fn admit(
        &mut self,
        file: &'a str,
        role: StorageRole<'a>,
        offset: u64,
        rows: u64,
    ) -> Result<()> {
        let end = offset
            .checked_add(rows)
            .ok_or(DataReadError::InvalidManifest("physical row range"))?;
        let found = self.files.entry(file).or_insert_with(|| PhysicalFile {
            role,
            ranges: BTreeMap::new(),
        });
        if found.role != role {
            return Err(DataReadError::InvalidManifest("conflicting storage roles"));
        }
        if rows == 0 {
            return Ok(());
        }
        if found
            .ranges
            .range(..=offset)
            .next_back()
            .is_some_and(|(_, previous_end)| *previous_end > offset)
            || found
                .ranges
                .range(offset..)
                .next()
                .is_some_and(|(next_start, _)| *next_start < end)
        {
            return Err(DataReadError::InvalidManifest(
                "overlapping physical slices",
            ));
        }
        found.ranges.insert(offset, end);
        Ok(())
    }
}

impl Manifest {
    pub(crate) fn parse(bytes: &[u8], limits: LocalGraphLimits) -> Result<Self> {
        Self::parse_inner(bytes, limits, true)
    }
    pub(crate) fn parse_generated(bytes: &[u8], limits: LocalGraphLimits) -> Result<Self> {
        Self::parse_inner(bytes, limits, false)
    }
    fn parse_inner(bytes: &[u8], limits: LocalGraphLimits, imported_bounds: bool) -> Result<Self> {
        let admit = |value, maximum, name| {
            if imported_bounds {
                bounded(value, maximum, name)
            } else {
                Ok(())
            }
        };
        admit(bytes.len(), limits.manifest_bytes, "manifest bytes")?;
        let mut manifest: Self = serde_json::from_slice(bytes)
            .map_err(|_| DataReadError::InvalidManifest("JSON shape"))?;
        if manifest.manifest_version != 3
            || manifest.format != "icebug-canonical"
            || !manifest.finished
            || !manifest.relation_uids
            || manifest.invariants.indptr_row_groups != 1
            || manifest.invariants.self_loops != "forward-once"
        {
            return Err(DataReadError::UnsupportedFormat("Icebug manifest"));
        }
        admit(manifest.node_tables.len(), limits.max_tables, "tables")?;
        admit(
            manifest.relationships.len(),
            limits.max_members,
            "relationships",
        )?;
        let name = |value: &str| -> Result<()> {
            admit(value.len(), limits.max_name_bytes, "catalog name")
        };
        for value in [
            &manifest.format,
            &manifest.storage,
            &manifest.schema_file,
            &manifest.invariants.self_loops,
        ] {
            name(value)?;
        }
        relative(&manifest.schema_file)?;
        u32::try_from(manifest.node_tables.len())
            .map_err(|_| DataReadError::InvalidManifest("node identity width"))?;
        let mut labels = BTreeSet::new();
        let mut files = StorageFiles::default();
        files.admit("icebug.json", StorageRole::Manifest, 0, 0)?;
        files.admit(&manifest.schema_file, StorageRole::Schema, 0, 0)?;
        for table in &manifest.node_tables {
            if table.label.is_empty() || !labels.insert(table.label.as_str()) {
                return Err(DataReadError::InvalidManifest("duplicate label"));
            }
            admit(table.shards.len(), limits.max_members, "node shards")?;
            let mut rows = 0u64;
            let mut end = 0u64;
            for shard in &table.shards {
                relative(&shard.file)?;
                name(&shard.file)?;
                files.admit(
                    &shard.file,
                    StorageRole::Nodes(&table.label),
                    shard.row_offset,
                    shard.rows,
                )?;
                if shard.row_base < end {
                    return Err(DataReadError::InvalidManifest("overlapping node shards"));
                }
                end = shard
                    .row_base
                    .checked_add(shard.rows)
                    .ok_or(DataReadError::InvalidManifest("node shard rows"))?;
                rows = rows
                    .checked_add(shard.rows)
                    .ok_or(DataReadError::InvalidManifest("node shard rows"))?;
            }
            if rows != table.rows || table.shards.is_empty() {
                return Err(DataReadError::InvalidManifest("node shard rows"));
            }
            for value in [&table.label, &table.primary_key] {
                name(value)?;
            }

            admit(table.columns.len(), limits.max_columns, "columns")?;
            admit(
                table.lang_counts.len(),
                limits.max_metadata_entries,
                "language counts",
            )?;
            let mut columns = BTreeSet::new();
            for column in &table.columns {
                name(&column.name)?;
                name(&column.kind)?;
                if column.name.is_empty() || !columns.insert(column.name.as_str()) {
                    return Err(DataReadError::InvalidManifest("duplicate column"));
                }
                if !matches!(column.kind.as_str(), "STRING" | "INT64" | "BOOL") {
                    return Err(DataReadError::UnsupportedFormat("column type"));
                }
            }
            if table.rows > 0
                && !table.columns.iter().any(|column| {
                    column.name == table.primary_key
                        && matches!(column.kind.as_str(), "STRING" | "INT64")
                })
            {
                return Err(DataReadError::InvalidManifest("primary key"));
            }
            let mut language_rows = 0u64;
            for language in &table.lang_counts {
                name(&language.lang)?;
                language_rows = language_rows
                    .checked_add(language.rows)
                    .ok_or(DataReadError::InvalidManifest("language rows"))?;
            }
            if language_rows > table.rows {
                return Err(DataReadError::InvalidManifest("language rows"));
            }
        }
        let mut kinds = BTreeSet::new();
        let mut physical = BTreeSet::new();
        let mut members = 0usize;
        let mut edges = 0u64;
        for group in &manifest.relationships {
            name(&group.kind)?;
            if group.kind.is_empty() || !kinds.insert(group.kind.as_str()) {
                return Err(DataReadError::InvalidManifest("duplicate relationship"));
            }
            if !manifest.add_reverse_edges && !group.reverse_members.is_empty() {
                return Err(DataReadError::InvalidManifest("reverse declaration"));
            }
            for member in group.members.iter().chain(&group.reverse_members) {
                members = add(members, 1, "members")?;
                for value in [
                    &member.from,
                    &member.to,
                    &member.table,
                    &member.indices,
                    &member.indptr,
                ] {
                    name(value)?;
                }
                if !labels.contains(member.from.as_str())
                    || !labels.contains(member.to.as_str())
                    || member.table.is_empty()
                    || !physical.insert(member.table.as_str())
                {
                    return Err(DataReadError::InvalidManifest("member identity"));
                }
                for (label, base, rows) in [
                    (&member.from, member.source_row_base, member.source_rows),
                    (&member.to, member.target_row_base, member.target_rows),
                ] {
                    if !manifest
                        .node_tables
                        .iter()
                        .find(|node| &node.label == label)
                        .is_some_and(|node| {
                            node.shards
                                .iter()
                                .any(|shard| shard.row_base == base && shard.rows == rows)
                        })
                    {
                        return Err(DataReadError::InvalidManifest("CSR shard bounds"));
                    }
                }
                relative(&member.indices)?;
                relative(&member.indptr)?;
                files.admit(
                    &member.indices,
                    StorageRole::Indices(&group.kind),
                    member.indices_row_offset,
                    member.rows,
                )?;
                files.admit(
                    &member.indptr,
                    StorageRole::Indptr,
                    member.indptr_row_offset,
                    member
                        .source_rows
                        .checked_add(1)
                        .ok_or(DataReadError::InvalidManifest("pointer rows"))?,
                )?;
            }
            for member in &group.members {
                edges = edges
                    .checked_add(member.rows)
                    .ok_or(DataReadError::InvalidManifest("edge total"))?;
            }
        }
        admit(members, limits.max_members, "members")?;
        u32::try_from(members)
            .map_err(|_| DataReadError::InvalidManifest("relationship identity width"))?;
        if edges != manifest.n_edges {
            return Err(DataReadError::InvalidManifest("edge total"));
        }
        manifest.node_tables.sort_by(|a, b| a.label.cmp(&b.label));
        manifest.relationships.sort_by(|a, b| a.kind.cmp(&b.kind));
        let _ = manifest.repaired_strings; // Declared producer metric, not read authority.
        Ok(manifest)
    }
}
