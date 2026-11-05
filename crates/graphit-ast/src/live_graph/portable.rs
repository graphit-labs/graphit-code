//! FILE catalogue and rebuildable disk indexes. Only Parquet and its manifest persist.
use super::*;
use graphit_data::graph_writer::{
    FileNodeShard, visit_fact_rows, visit_node_shard, write_csr_shard, write_empty_node_shard,
    write_fact_rows, write_file_nodes,
};
use rusqlite::{Connection, params};
use std::io::Write;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct NodePart {
    pub shard: FileNodeShard,
    pub row_base: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct FileRecord {
    pub observation: LiveFileObservation,
    pub facts: String,
    pub nodes: Vec<NodePart>,
    pub relationships: Vec<Value>,
    pub pending_compiler: bool,
    pub scip_document: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Catalog {
    pub namespace: [u8; 32],
    pub schema: LiveGraphSchema,
    pub next_rows: BTreeMap<String, u64>,
    pub files: BTreeMap<String, FileRecord>,
    pub externals: BTreeMap<String, String>,
}
impl Catalog {
    pub fn new(namespace: [u8; 32]) -> Self {
        Self {
            namespace,
            schema: LiveGraphSchema::default(),
            next_rows: BTreeMap::new(),
            files: BTreeMap::new(),
            externals: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct FileFacts {
    pub parser_edges: Vec<GraphEdge>,
    pub edges: Vec<GraphEdge>,
    pub scip_edges: BTreeMap<String, Vec<GraphEdge>>,
    pub oracle_edges: BTreeMap<String, Vec<GraphEdge>>,
    pub definitions: Vec<crate::semantic_graph::scip_document::AstScipDocumentDefinition>,
    pub references: Vec<crate::semantic_graph::scip_document::AstScipDocumentReference>,
    pub oracle: Vec<crate::semantic_graph::oracle::AstOracleGraphRows>,
    pub oracle_file: Option<crate::semantic_graph::AstGraphNode>,
    pub pending_compiler: Option<String>,
    pub scip_document: Option<String>,
}
pub(super) struct State {
    pub catalog: Catalog,
    pub index: Option<Index>,
}
impl State {
    pub fn ensure_index(&mut self, root: &Path, original: &Arc<SnapshotContext>) -> io::Result<()> {
        if self.index.is_none() {
            self.index = Some(Index::rebuild(root, &self.catalog, original)?);
        }
        Ok(())
    }
    pub fn index(&self) -> io::Result<&Index> {
        self.index
            .as_ref()
            .ok_or_else(|| invalid("FILE lookup index is not initialized"))
    }
}
pub(super) struct Index {
    pub database: Connection,
    _temporary: tempfile::TempDir,
}
fn sql(error: rusqlite::Error) -> io::Error {
    io::Error::other(error)
}
impl Index {
    pub fn rebuild(
        root: &Path,
        catalog: &Catalog,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        let temporary = tempfile::tempdir()?;
        let database = Connection::open(temporary.path().join("locators.sqlite")).map_err(sql)?;
        database.execute_batch("PRAGMA temp_store=FILE;
            CREATE TABLE entities(key TEXT PRIMARY KEY,label TEXT NOT NULL,path TEXT NOT NULL,row_base INTEGER NOT NULL,local_row INTEGER NOT NULL,shard_rows INTEGER NOT NULL);
            CREATE INDEX entity_owner ON entities(path);
            CREATE TABLE scip_def(symbol TEXT NOT NULL,path TEXT NOT NULL,entity TEXT NOT NULL,payload TEXT NOT NULL,UNIQUE(symbol,path,entity,payload));
            CREATE INDEX scip_definition_symbol ON scip_def(symbol);
            CREATE TABLE scip_ref(symbol TEXT NOT NULL,path TEXT NOT NULL,UNIQUE(symbol,path));
            CREATE INDEX scip_reference_symbol ON scip_ref(symbol);
            CREATE TABLE oracle_def(lookup TEXT NOT NULL,path TEXT NOT NULL,entity TEXT NOT NULL,payload TEXT NOT NULL,UNIQUE(lookup,path,entity,payload));
            CREATE INDEX oracle_definition_lookup ON oracle_def(lookup);
            CREATE TABLE oracle_ref(lookup TEXT NOT NULL,path TEXT NOT NULL,UNIQUE(lookup,path));
            CREATE INDEX oracle_reference_lookup ON oracle_ref(lookup);
            CREATE TABLE edge_ref(key TEXT NOT NULL,path TEXT NOT NULL);
            CREATE INDEX edge_reference_key ON edge_ref(key);").map_err(sql)?;
        let cancellation = Arc::clone(original);
        database
            .progress_handler(1024, Some(move || cancellation.check().is_err()))
            .map_err(sql)?;
        let result = Self {
            database,
            _temporary: temporary,
        };
        result.database.execute_batch("BEGIN").map_err(sql)?;
        let mut ranges = BTreeMap::<String, Vec<(u64, u64)>>::new();
        for (path, file) in &catalog.files {
            original.check()?;
            if path.is_empty() || path.contains('\0') || file.observation.path != *path {
                return Err(invalid("invalid FILE catalogue ownership"));
            }
            for part in &file.nodes {
                let end = part
                    .row_base
                    .checked_add(part.shard.rows)
                    .ok_or_else(|| invalid("FILE range overflow"))?;
                if end
                    > catalog
                        .next_rows
                        .get(&part.shard.label)
                        .copied()
                        .unwrap_or(0)
                {
                    return Err(invalid("FILE range exceeds allocated rows"));
                }
                ranges
                    .entry(part.shard.label.clone())
                    .or_default()
                    .push((part.row_base, end));
            }
            let nodes = result.index_nodes(root, path, &file.nodes, original)?;
            let facts = read_facts(root, &file.facts, original)?;
            if file.pending_compiler != facts.pending_compiler.is_some()
                || file.scip_document != facts.scip_document.is_some()
            {
                return Err(invalid("FILE payload membership differs from catalogue"));
            }
            let mut actual_schema = LiveGraphSchema::default();
            validate_nodes(&mut actual_schema, &nodes)?;
            for (label, columns) in actual_schema.nodes {
                for (name, kind) in columns {
                    if catalog
                        .schema
                        .nodes
                        .get(&label)
                        .and_then(|columns| columns.get(&name))
                        != Some(&kind)
                    {
                        return Err(invalid("FILE schema differs from catalogue"));
                    }
                }
            }
            scip::validate(
                &LiveFileInput {
                    path,
                    source_sha256: file.observation.source_sha256,
                    raw_version: file.observation.raw_version,
                    nodes: &nodes,
                    edges: &facts.parser_edges,
                    definitions: &facts.definitions,
                    references: &facts.references,
                    scip_document: None,
                    oracle: &facts.oracle,
                    oracle_file: facts.oracle_file.as_ref(),
                    pending_compiler: facts.pending_compiler.as_deref(),
                    scip_compiler: file.observation.scip_compiler.as_deref(),
                },
                original,
            )?;
            result.index_facts(path, &facts)?;
        }
        for ranges in ranges.values_mut() {
            ranges.sort();
            for pair in ranges.windows(2) {
                if pair[0].1 > pair[1].0 {
                    return Err(invalid("overlapping FILE node ranges"));
                }
            }
        }
        result.database.execute_batch("COMMIT").map_err(sql)?;
        Ok(result)
    }
    fn index_nodes(
        &self,
        root: &Path,
        path: &str,
        parts: &[NodePart],
        original: &Arc<SnapshotContext>,
    ) -> io::Result<Vec<GraphNode>> {
        let mut nodes = Vec::new();
        for part in parts {
            let mut row = 0u64;
            visit_node_shard(
                &owned_path(root, &part.shard.file)?,
                &part.shard.label,
                original,
                |node| {
                    self.insert_node(path, &node, part.row_base, row, part.shard.rows)?;
                    nodes.push(node);
                    row = row
                        .checked_add(1)
                        .ok_or_else(|| invalid("FILE row overflow"))?;
                    Ok(())
                },
            )?;
            if row != part.shard.rows {
                return Err(invalid("FILE node rows differ from manifest"));
            }
        }
        Ok(nodes)
    }
    fn insert_node(
        &self,
        path: &str,
        node: &GraphNode,
        base: u64,
        row: u64,
        rows: u64,
    ) -> io::Result<()> {
        self.database
            .execute(
                "INSERT INTO entities VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    node.key,
                    node.label,
                    path,
                    i64::try_from(base).map_err(|_| invalid("node row base overflow"))?,
                    i64::try_from(row).map_err(|_| invalid("node row offset overflow"))?,
                    i64::try_from(rows).map_err(|_| invalid("node row count overflow"))?
                ],
            )
            .map_err(sql)?;
        Ok(())
    }
    pub fn index_facts(&self, path: &str, facts: &FileFacts) -> io::Result<()> {
        for definition in &facts.definitions {
            self.database
                .execute(
                    "INSERT OR IGNORE INTO scip_def VALUES (?1,?2,?3,?4)",
                    params![
                        scip::symbol_key(path, &definition.symbol)?,
                        path,
                        definition.entity_key,
                        encoded(definition)?
                    ],
                )
                .map_err(sql)?;
        }
        for reference in &facts.references {
            if reference.kind.is_some() && reference.source_key.is_some() {
                self.database
                    .execute(
                        "INSERT OR IGNORE INTO scip_ref VALUES (?1,?2)",
                        params![scip::symbol_key(path, &reference.symbol)?, path],
                    )
                    .map_err(sql)?;
            }
        }
        for rows in &facts.oracle {
            for endpoint in rows.endpoints() {
                self.database
                    .execute(
                        "INSERT OR IGNORE INTO oracle_def VALUES (?1,?2,?3,?4)",
                        params![
                            endpoint.lookup_key(),
                            path,
                            endpoint.key(),
                            encoded(endpoint)?
                        ],
                    )
                    .map_err(sql)?;
            }
            for reference in rows.references() {
                if let Some(key) = reference.lookup_key() {
                    self.database
                        .execute(
                            "INSERT OR IGNORE INTO oracle_ref VALUES (?1,?2)",
                            params![key, path],
                        )
                        .map_err(sql)?;
                }
            }
        }
        for edge in &facts.edges {
            for key in [&edge.source_key, &edge.target_key] {
                self.database
                    .execute("INSERT INTO edge_ref VALUES (?1,?2)", params![key, path])
                    .map_err(sql)?;
            }
        }
        Ok(())
    }
    pub(super) fn set_context(&self, original: &Arc<SnapshotContext>) -> io::Result<()> {
        let cancellation = Arc::clone(original);
        self.database
            .progress_handler(1024, Some(move || cancellation.check().is_err()))
            .map_err(sql)
    }
    fn remove_file(&self, path: &str) -> io::Result<()> {
        for table in [
            "entities",
            "scip_def",
            "scip_ref",
            "oracle_def",
            "oracle_ref",
            "edge_ref",
        ] {
            self.database
                .execute(&format!("DELETE FROM {table} WHERE path=?1"), [path])
                .map_err(sql)?;
        }
        Ok(())
    }
    fn replace_edges(&self, path: &str, edges: &[GraphEdge]) -> io::Result<()> {
        self.database
            .execute("DELETE FROM edge_ref WHERE path=?1", [path])
            .map_err(sql)?;
        for edge in edges {
            for key in [&edge.source_key, &edge.target_key] {
                self.database
                    .execute("INSERT INTO edge_ref VALUES (?1,?2)", params![key, path])
                    .map_err(sql)?;
            }
        }
        Ok(())
    }
    pub fn candidates<T: serde::de::DeserializeOwned>(
        &self,
        table: &str,
        column: &str,
        key: &str,
    ) -> io::Result<Vec<T>> {
        let query = format!(
            "SELECT d.payload FROM {table} d JOIN entities e ON e.key=d.entity WHERE d.{column}=?1 ORDER BY d.entity"
        );
        let mut statement = self.database.prepare(&query).map_err(sql)?;
        let rows = statement
            .query_map([key], |row| row.get::<_, String>(0))
            .map_err(sql)?;
        rows.map(|row| decoded(&row.map_err(sql)?)).collect()
    }
    fn affected(
        &self,
        table: &str,
        column: &str,
        keys: &BTreeSet<String>,
    ) -> io::Result<BTreeSet<String>> {
        let mut paths = BTreeSet::new();
        let mut statement = self
            .database
            .prepare(&format!(
                "SELECT DISTINCT path FROM {table} WHERE {column}=?1"
            ))
            .map_err(sql)?;
        for key in keys {
            for row in statement
                .query_map([key], |row| row.get::<_, String>(0))
                .map_err(sql)?
            {
                paths.insert(row.map_err(sql)?);
            }
        }
        Ok(paths)
    }
    pub fn address(&self, key: &str) -> io::Result<Option<(String, u64, u64, u64)>> {
        use rusqlite::OptionalExtension;
        let address = self
            .database
            .query_row(
                "SELECT label,row_base,local_row,shard_rows FROM entities WHERE key=?1",
                [key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(sql)?;
        address
            .map(|(label, base, row, rows)| {
                Ok((
                    label,
                    u64::try_from(base).map_err(|_| invalid("negative node row base"))?,
                    u64::try_from(row).map_err(|_| invalid("negative node row offset"))?,
                    u64::try_from(rows).map_err(|_| invalid("negative node row count"))?,
                ))
            })
            .transpose()
    }
}
pub(super) fn read_facts(
    root: &Path,
    file: &str,
    original: &SnapshotContext,
) -> io::Result<FileFacts> {
    let mut result = None;
    visit_fact_rows(&owned_path(root, file)?, original, |payload| {
        if result.is_some() {
            return Err(invalid("duplicate FILE fact row"));
        }
        result = Some(decoded(payload)?);
        Ok(())
    })?;
    result.ok_or_else(|| invalid("missing FILE fact row"))
}
pub(super) fn read_nodes(
    root: &Path,
    file: &FileRecord,
    original: &SnapshotContext,
) -> io::Result<Vec<GraphNode>> {
    let mut result = Vec::new();
    for part in &file.nodes {
        visit_node_shard(
            &owned_path(root, &part.shard.file)?,
            &part.shard.label,
            original,
            |node| {
                result.push(node);
                Ok(())
            },
        )?;
    }
    Ok(result)
}
fn changed_keys(nodes: &[GraphNode]) -> BTreeSet<String> {
    nodes.iter().map(|node| node.key.clone()).collect()
}
fn symbols(facts: &FileFacts, path: &str) -> io::Result<BTreeSet<String>> {
    facts
        .definitions
        .iter()
        .map(|definition| scip::symbol_key(path, &definition.symbol))
        .chain(
            facts
                .references
                .iter()
                .map(|reference| scip::symbol_key(path, &reference.symbol)),
        )
        .collect()
}
fn lookups(facts: &FileFacts) -> BTreeSet<String> {
    facts
        .oracle
        .iter()
        .flat_map(|rows| {
            rows.endpoints()
                .iter()
                .map(|endpoint| endpoint.lookup_key().to_owned())
                .chain(
                    rows.references()
                        .iter()
                        .filter_map(|reference| reference.lookup_key().map(str::to_owned)),
                )
        })
        .collect()
}
fn validate_nodes(schema: &mut LiveGraphSchema, nodes: &[GraphNode]) -> io::Result<()> {
    let mut seen = BTreeSet::new();
    for node in nodes {
        ident(&node.label)?;
        if node.key.is_empty() || node.key.contains('\0') || !seen.insert(&node.key) {
            return Err(invalid("invalid or duplicate FILE entity key"));
        }
        let columns = schema.nodes.entry(node.label.clone()).or_default();
        for (name, value) in &node.properties {
            ident(name)?;
            if name == "entity_key" {
                return Err(invalid("graph primary key is supplied separately"));
            }
            let kind = match value {
                GraphValue::String(_) => "STRING",
                GraphValue::Int64(_) => "INT64",
                GraphValue::Bool(_) => "BOOL",
            };
            if columns
                .insert(name.clone(), kind.into())
                .is_some_and(|previous| previous != kind)
            {
                return Err(invalid("inconsistent graph column type"));
            }
        }
    }
    Ok(())
}
fn allocate(
    index: &Index,
    catalog: &mut Catalog,
    path: &str,
    nodes: &[GraphNode],
) -> io::Result<()> {
    index
        .database
        .execute("DELETE FROM entities WHERE path=?1", [path])
        .map_err(sql)?;
    let mut counts = BTreeMap::<String, u64>::new();
    for node in nodes {
        *counts.entry(node.label.clone()).or_default() += 1;
    }
    let mut bases = BTreeMap::new();
    for (label, count) in &counts {
        let base = catalog.next_rows.entry(label.clone()).or_default();
        bases.insert(label.clone(), *base);
        *base = base
            .checked_add(*count)
            .filter(|end| *end <= i64::MAX as u64)
            .ok_or_else(|| invalid("graph row allocation overflow"))?;
    }
    let mut rows = BTreeMap::<String, u64>::new();
    for node in nodes {
        let row = rows.entry(node.label.clone()).or_default();
        index.insert_node(path, node, bases[&node.label], *row, counts[&node.label])?;
        *row += 1;
    }
    Ok(())
}
fn stage_directory(root: &Path) -> io::Result<(PathBuf, String)> {
    let shards = root.join("shards");
    std::fs::create_dir_all(&shards)?;
    let directory = tempfile::Builder::new()
        .prefix("file-")
        .tempdir_in(&shards)?
        .keep();
    let relative = format!(
        "shards/{}",
        directory
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("invalid shard directory"))?
    );
    Ok((directory, relative))
}
fn stage_nodes(
    root: &Path,
    index: &Index,
    nodes: &[GraphNode],
    original: &Arc<SnapshotContext>,
) -> io::Result<Vec<NodePart>> {
    let (directory, relative) = stage_directory(root)?;
    let mut parts = Vec::new();
    for mut shard in write_file_nodes(&directory, nodes, original)? {
        let key = nodes
            .iter()
            .find(|node| node.label == shard.label)
            .ok_or_else(|| invalid("missing allocated node shard"))?
            .key
            .as_str();
        let (_, row_base, _, rows) = index
            .address(key)?
            .ok_or_else(|| invalid("allocated node disappeared"))?;
        if rows != shard.rows {
            return Err(invalid("allocated FILE row count differs"));
        }
        shard.file = format!("{relative}/{}", shard.file);
        parts.push(NodePart { shard, row_base });
    }
    Ok(parts)
}
fn stage_facts(
    root: &Path,
    facts: &FileFacts,
    original: &Arc<SnapshotContext>,
) -> io::Result<String> {
    let (directory, relative) = stage_directory(root)?;
    write_fact_rows(&directory.join("facts.parquet"), [encoded(facts)], original)?;
    Ok(format!("{relative}/facts.parquet"))
}
fn stage_relationships(
    root: &Path,
    index: &Index,
    catalog: &mut Catalog,
    edges: &[GraphEdge],
    original: &Arc<SnapshotContext>,
) -> io::Result<Vec<Value>> {
    let mut groups =
        BTreeMap::<(String, String, u64, u64, String, u64, u64), Vec<(u64, u64, String)>>::new();
    let mut uids = BTreeSet::new();
    for edge in edges {
        original.check()?;
        ident(&edge.kind)?;
        if edge.uid.is_empty() || edge.uid.contains('\0') || !uids.insert(&edge.uid) {
            return Err(invalid("invalid or duplicate FILE edge identity"));
        }
        let (
            Some((from, source_base, source, source_rows)),
            Some((to, target_base, target, target_rows)),
        ) = (
            index.address(&edge.source_key)?,
            index.address(&edge.target_key)?,
        )
        else {
            continue;
        };
        groups
            .entry((
                edge.kind.clone(),
                from,
                source_base,
                source_rows,
                to,
                target_base,
                target_rows,
            ))
            .or_default()
            .push((source, target, edge.uid.clone()));
    }
    let mut result = Vec::new();
    for ((kind, from, source_row_base, source_rows, to, target_row_base, target_rows), rows) in
        groups
    {
        let (directory, relative) = stage_directory(root)?;
        write_csr_shard(&directory, source_rows, target_rows, &rows, original)?;
        catalog
            .schema
            .relationships
            .entry(kind.clone())
            .or_default()
            .insert((from.clone(), to.clone()));
        result.push(json!({"kind":kind,"from":from,"to":to,"table":relative,"indices":format!("{relative}/edges.indices"),"indptr":format!("{relative}/edges.indptr"),"rows":rows.len(),"source_row_base":source_row_base,"source_rows":source_rows,"target_row_base":target_row_base,"target_rows":target_rows}));
    }
    Ok(result)
}

pub(super) fn mutate(
    graph: &LiveGraph,
    path: &str,
    replacement: Option<LiveFileInput<'_>>,
    expected: Option<u64>,
    original: &Arc<SnapshotContext>,
) -> io::Result<u64> {
    original.check()?;
    if path.is_empty() || path.contains('\0') {
        return Err(invalid("invalid FILE path"));
    }
    let mut state = graph
        .state
        .lock()
        .map_err(|_| invalid("graph writer poisoned"))?;
    let old = state.catalog.files.get(path).cloned();
    if replacement.is_none() && old.is_none() {
        return Ok(state.catalog.schema.revision);
    }
    if let Some(expected) = expected {
        let current = old.as_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::WouldBlock,
                "FILE disappeared before compiler enrichment",
            )
        })?;
        let input = replacement
            .as_ref()
            .ok_or_else(|| invalid("enrichment lacks replacement"))?;
        if current.observation.file_revision != expected
            || current.observation.source_sha256 != input.source_sha256
            || current.observation.scip_compiler.as_deref() != input.scip_compiler
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "FILE changed before compiler enrichment",
            ));
        }
    }
    state.ensure_index(&graph.directory, original)?;
    state.index()?.set_context(original)?;
    state
        .index()?
        .database
        .execute_batch("BEGIN")
        .map_err(sql)?;
    let mut committed = false;
    let result = (|| {
        let mut next = state.catalog.clone();
        next.schema.revision = next
            .schema
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("graph revision overflow"))?;
        let mut keys = BTreeSet::new();
        let mut changed_symbols = BTreeSet::new();
        let mut changed_lookups = BTreeSet::new();
        if let Some(file) = &old {
            let nodes = read_nodes(&graph.directory, file, original)?;
            keys.extend(changed_keys(&nodes));
            let facts = read_facts(&graph.directory, &file.facts, original)?;
            changed_symbols.extend(symbols(&facts, path)?);
            changed_lookups.extend(lookups(&facts));
        }
        let mut edge_owners = state.index()?.affected("edge_ref", "key", &keys)?;
        let mut semantic_owners =
            state
                .index()?
                .affected("scip_ref", "symbol", &changed_symbols)?;
        semantic_owners.extend(state.index()?.affected(
            "oracle_ref",
            "lookup",
            &changed_lookups,
        )?);
        state.index()?.remove_file(path)?;
        next.files.remove(path);
        let mut current = None;
        if let Some(input) = replacement {
            validate_nodes(&mut next.schema, input.nodes)?;
            let facts = FileFacts::from_input(&input, original)?;
            keys.extend(changed_keys(input.nodes));
            changed_symbols.extend(symbols(&facts, path)?);
            changed_lookups.extend(lookups(&facts));
            allocate(state.index()?, &mut next, path, input.nodes)?;
            state.index()?.index_facts(path, &facts)?;
            let record = FileRecord {
                observation: LiveFileObservation {
                    path: path.into(),
                    source_sha256: input.source_sha256,
                    file_revision: expected.unwrap_or(next.schema.revision),
                    raw_version: input.raw_version,
                    scip_compiler: input.scip_compiler.map(str::to_owned),
                },
                facts: String::new(),
                nodes: Vec::new(),
                relationships: Vec::new(),
                pending_compiler: input.pending_compiler.is_some(),
                scip_document: input.scip_document.is_some(),
            };
            next.files.insert(path.into(), record);
            current = Some((input.nodes.to_vec(), facts));
            semantic_owners.insert(path.into());
        }
        semantic_owners.extend(
            state
                .index()?
                .affected("scip_ref", "symbol", &changed_symbols)?,
        );
        semantic_owners.extend(state.index()?.affected(
            "oracle_ref",
            "lookup",
            &changed_lookups,
        )?);
        edge_owners.extend(state.index()?.affected("edge_ref", "key", &keys)?);
        for owner in semantic_owners {
            original.check()?;
            let Some(record) = next.files.get(&owner).cloned() else {
                continue;
            };
            let (mut nodes, mut facts) = if owner == path {
                current
                    .take()
                    .ok_or_else(|| invalid("replacement FILE facts missing"))?
            } else {
                (
                    read_nodes(&graph.directory, &record, original)?,
                    read_facts(&graph.directory, &record.facts, original)?,
                )
            };
            let before = nodes.clone();
            let before_facts = facts.clone();
            oracle::resolve(
                state.index()?,
                &mut nodes,
                &mut facts,
                if owner == path {
                    None
                } else {
                    Some(&changed_lookups)
                },
                original,
            )?;
            scip::resolve(
                state.index()?,
                &mut nodes,
                &mut facts,
                if owner == path {
                    None
                } else {
                    Some(&changed_symbols)
                },
                original,
            )?;
            facts.rebuild_edges()?;
            let nodes_changed = owner == path || nodes != before;
            let node_parts = if nodes_changed {
                if owner != path {
                    allocate(state.index()?, &mut next, &owner, &nodes)?;
                }
                keys.extend(changed_keys(&nodes));
                stage_nodes(&graph.directory, state.index()?, &nodes, original)?
            } else {
                record.nodes
            };
            let edges_changed = owner == path || facts.edges != before_facts.edges;
            if edges_changed {
                state.index()?.replace_edges(&owner, &facts.edges)?;
            }
            let facts_path = if owner == path || facts != before_facts {
                stage_facts(&graph.directory, &facts, original)?
            } else {
                record.facts
            };
            let selected = next
                .files
                .get_mut(&owner)
                .ok_or_else(|| invalid("affected FILE disappeared"))?;
            selected.nodes = node_parts;
            selected.facts = facts_path;
            if nodes_changed || edges_changed {
                edge_owners.insert(owner);
            }
        }
        edge_owners.extend(state.index()?.affected("edge_ref", "key", &keys)?);
        for owner in edge_owners {
            original.check()?;
            let Some(record) = next.files.get(&owner) else {
                continue;
            };
            let facts = read_facts(&graph.directory, &record.facts, original)?;
            let relationships = stage_relationships(
                &graph.directory,
                state.index()?,
                &mut next,
                &facts.edges,
                original,
            )?;
            next.files
                .get_mut(&owner)
                .ok_or_else(|| invalid("edge owner disappeared"))?
                .relationships = relationships;
        }
        if let Some(previous) = old
            .as_ref()
            .and_then(|file| file.observation.scip_compiler.as_ref())
        {
            // An external-symbol group belongs to its exact compiler image.
            // Keep it while another FILE still uses that producer. New groups
            // staged before their first FILE are untouched by this retirement.
            if !next
                .files
                .values()
                .any(|file| file.observation.scip_compiler.as_ref() == Some(previous))
            {
                next.externals.remove(previous);
            }
        }
        original.check()?;
        publish(&graph.directory, &next, original)?;
        // The atomic manifest replacement is the effect; acknowledge before post-commit cancellation.
        state.catalog = next;
        committed = true;
        state
            .index()?
            .database
            .progress_handler(0, None::<fn() -> bool>)
            .map_err(sql)?;
        if let Err(error) = state.index()?.database.execute_batch("COMMIT") {
            state.index = None;
            return Err(io::Error::other(format!(
                "FILE committed at graph revision {}; lookup checkpoint failed: {error}",
                state.catalog.schema.revision
            )));
        }
        prune(&graph.directory, &state.catalog).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "FILE committed at graph revision {}; cleanup failed: {error}",
                    state.catalog.schema.revision
                ),
            )
        })?;
        original.check().map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "FILE committed at graph revision {}; {error}",
                    state.catalog.schema.revision
                ),
            )
        })?;
        Ok(state.catalog.schema.revision)
    })();
    if result.is_err()
        && let Some(index) = &state.index
    {
        let _ = index.database.execute_batch("ROLLBACK");
    }
    result.map_err(|cause| {
        let cause = if committed {
            cause
        } else {
            original.check().err().unwrap_or(cause)
        };
        io::Error::new(
            cause.kind(),
            FileMutationError {
                path: path.to_owned(),
                cause,
            },
        )
    })
}

impl FileFacts {
    fn rebuild_edges(&mut self) -> io::Result<()> {
        let mut edges = BTreeMap::new();
        for edge in self
            .parser_edges
            .iter()
            .chain(self.scip_edges.values().flatten())
            .chain(self.oracle_edges.values().flatten())
        {
            if let Some(previous) = edges.insert(edge.uid.clone(), edge.clone())
                && previous != *edge
            {
                return Err(invalid("derived edge UID conflicts"));
            }
        }
        self.edges = edges.into_values().collect();
        Ok(())
    }
    fn from_input(input: &LiveFileInput<'_>, original: &Arc<SnapshotContext>) -> io::Result<Self> {
        scip::validate(input, original)?;
        oracle::validate(input, original)?;
        scip_export::validate_document(input, original)?;
        let mut seen = BTreeSet::new();
        for edge in input.edges {
            ident(&edge.kind)?;
            if edge.uid.is_empty()
                || edge.uid.contains('\0')
                || edge.source_key.is_empty()
                || edge.target_key.is_empty()
                || edge.source_key.contains('\0')
                || edge.target_key.contains('\0')
                || !seen.insert(&edge.uid)
            {
                return Err(invalid("invalid graph edge identity"));
            }
        }
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        Ok(Self {
            parser_edges: input.edges.to_vec(),
            edges: input.edges.to_vec(),
            scip_edges: BTreeMap::new(),
            oracle_edges: BTreeMap::new(),
            definitions: input.definitions.to_vec(),
            references: input.references.to_vec(),
            oracle: input.oracle.to_vec(),
            oracle_file: input.oracle_file.cloned(),
            pending_compiler: input.pending_compiler.map(str::to_owned),
            scip_document: input.scip_document.map(|bytes| STANDARD.encode(bytes)),
        })
    }
}

pub(super) fn publish(
    root: &Path,
    catalog: &Catalog,
    original: &Arc<SnapshotContext>,
) -> io::Result<()> {
    original.check()?;
    let (directory, relative) = stage_directory(root)?;
    let mut node_tables = Vec::new();
    let mut schema = String::new();
    for (label, columns) in &catalog.schema.nodes {
        let mut parts = catalog
            .files
            .values()
            .flat_map(|file| file.nodes.iter())
            .filter(|part| part.shard.label == *label)
            .collect::<Vec<_>>();
        parts.sort_by_key(|part| part.row_base);
        let mut rows = 0u64;
        let mut languages = BTreeMap::<String, u64>::new();
        let mut shards = Vec::new();
        for part in parts {
            rows = rows
                .checked_add(part.shard.rows)
                .ok_or_else(|| invalid("graph rows overflow"))?;
            shards.push(
                json!({"file":part.shard.file,"row_base":part.row_base,"rows":part.shard.rows}),
            );
            for (language, count) in &part.shard.languages {
                let total = languages.entry(language.clone()).or_default();
                *total = total
                    .checked_add(*count)
                    .ok_or_else(|| invalid("language rows overflow"))?;
            }
        }
        if shards.is_empty() {
            let filename = format!("empty-{}.parquet", node_tables.len());
            write_empty_node_shard(&directory.join(&filename), columns, original)?;
            shards.push(json!({"file":format!("{relative}/{filename}"),"row_base":0,"rows":0}));
        }
        let mut all_columns = columns.clone();
        all_columns.insert("entity_key".into(), "STRING".into());
        node_tables.push(json!({"label":label,"shards":shards,"rows":rows,"primary_key":"entity_key","columns":all_columns.iter().map(|(name,kind)|json!({"name":name,"type":kind})).collect::<Vec<_>>(),"lang_counts":languages.into_iter().map(|(lang,rows)|json!({"lang":lang,"rows":rows})).collect::<Vec<_>>() }));
        schema.push_str(&format!(
            "CREATE NODE TABLE {} ({}, PRIMARY KEY (entity_key));\n",
            ident(label)?,
            all_columns
                .iter()
                .map(|(name, kind)| Ok(format!("{} {kind}", ident(name)?)))
                .collect::<io::Result<Vec<_>>>()?
                .join(", ")
        ));
    }
    let mut relationships = BTreeMap::<String, Vec<Value>>::new();
    let mut n_edges = 0u64;
    for file in catalog.files.values() {
        for member in &file.relationships {
            let mut member = member.clone();
            let kind = member
                .as_object_mut()
                .and_then(|fields| fields.remove("kind"))
                .and_then(|kind| kind.as_str().map(str::to_owned))
                .ok_or_else(|| invalid("invalid CSR kind"))?;
            n_edges = n_edges
                .checked_add(
                    member["rows"]
                        .as_u64()
                        .ok_or_else(|| invalid("invalid CSR row count"))?,
                )
                .ok_or_else(|| invalid("graph edges overflow"))?;
            relationships.entry(kind).or_default().push(member);
        }
    }
    for (kind, pairs) in &catalog.schema.relationships {
        for (from, to) in pairs {
            let present = relationships.get(kind).is_some_and(|members| {
                members.iter().any(|member| {
                    member["from"].as_str() == Some(from) && member["to"].as_str() == Some(to)
                })
            });
            if present {
                continue;
            }
            let source = node_tables
                .iter()
                .find(|table| table["label"].as_str() == Some(from))
                .and_then(|table| table["shards"].as_array())
                .and_then(|shards| shards.first())
                .ok_or_else(|| invalid("empty relationship source missing"))?;
            let target = node_tables
                .iter()
                .find(|table| table["label"].as_str() == Some(to))
                .and_then(|table| table["shards"].as_array())
                .and_then(|shards| shards.first())
                .ok_or_else(|| invalid("empty relationship target missing"))?;
            let source_rows = source["rows"]
                .as_u64()
                .ok_or_else(|| invalid("empty relationship source rows"))?;
            let target_rows = target["rows"]
                .as_u64()
                .ok_or_else(|| invalid("empty relationship target rows"))?;
            let name = format!(
                "empty-edge-{}",
                relationships.values().map(Vec::len).sum::<usize>()
            );
            std::fs::create_dir(directory.join(&name))?;
            write_csr_shard(
                &directory.join(&name),
                source_rows,
                target_rows,
                &[],
                original,
            )?;
            relationships.entry(kind.clone()).or_default().push(json!({"from":from,"to":to,"table":format!("{relative}/{name}"),"indices":format!("{relative}/{name}/edges.indices"),"indptr":format!("{relative}/{name}/edges.indptr"),"rows":0,"source_row_base":source["row_base"],"source_rows":source_rows,"target_row_base":target["row_base"],"target_rows":target_rows}));
        }
    }
    for (kind, pairs) in &catalog.schema.relationships {
        let endpoints = pairs
            .iter()
            .map(|(from, to)| Ok(format!("FROM {} TO {}", ident(from)?, ident(to)?)))
            .collect::<io::Result<Vec<_>>>()?
            .join(", ");
        schema.push_str(&format!(
            "CREATE REL TABLE GROUP {} ({endpoints}, uid STRING);\n",
            ident(kind)?
        ));
    }
    std::fs::write(directory.join("schema.cypher"), schema)?;
    let manifest = json!({"manifest_version":3,"format":"icebug-canonical","storage":"direct-parquet","finished":true,"add_reverse_edges":false,"schema_file":format!("{relative}/schema.cypher"),"node_tables":node_tables,"relationships":relationships.into_iter().map(|(kind,members)|json!({"type":kind,"members":members})).collect::<Vec<_>>(),"relation_uids":true,"generation":format!("{}:{}",catalog.namespace.iter().map(|byte|format!("{byte:02x}")).collect::<String>(),catalog.schema.revision),"n_edges":n_edges,"repaired_strings":0,"invariants":{"indptr_row_groups":1,"self_loops":"forward-once"},"live":catalog});
    let mut temporary = tempfile::NamedTempFile::new_in(root)?;
    serde_json::to_writer(&mut temporary, &manifest).map_err(io::Error::other)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    original.check()?;
    temporary
        .persist(root.join("icebug.json"))
        .map_err(|error| error.error)?;
    #[cfg(test)]
    AFTER_MANIFEST_REPLACE.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
    Ok(())
}

#[cfg(test)]
thread_local! {
    pub(super) static AFTER_MANIFEST_REPLACE: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

pub(super) fn owned_path(root: &Path, relative: &str) -> io::Result<PathBuf> {
    if relative.is_empty() {
        return Err(invalid("empty FILE shard path"));
    }
    let mut path = root.to_owned();
    for component in Path::new(relative).components() {
        let std::path::Component::Normal(component) = component else {
            return Err(invalid("FILE shard path escapes its graph"));
        };
        path.push(component);
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(invalid("FILE shard is a symbolic link"));
        }
    }
    Ok(path)
}

pub(super) fn prune(root: &Path, catalog: &Catalog) -> io::Result<()> {
    let mut retained = BTreeSet::new();
    let mut retain = |file: &str| -> io::Result<()> {
        let mut components = Path::new(file).components();
        if components.next() != Some(std::path::Component::Normal(std::ffi::OsStr::new("shards"))) {
            return Err(invalid("FILE shard outside canonical directory"));
        }
        let Some(std::path::Component::Normal(directory)) = components.next() else {
            return Err(invalid("FILE shard has no directory"));
        };
        retained.insert(directory.to_owned());
        Ok(())
    };
    for file in catalog.files.values() {
        retain(&file.facts)?;
        for part in &file.nodes {
            retain(&part.shard.file)?;
        }
        for member in &file.relationships {
            for field in ["indices", "indptr"] {
                retain(
                    member[field]
                        .as_str()
                        .ok_or_else(|| invalid("invalid CSR file"))?,
                )?;
            }
        }
    }
    for file in catalog.externals.values() {
        retain(file)?;
    }
    let manifest: Value = serde_json::from_reader(std::fs::File::open(root.join("icebug.json"))?)
        .map_err(io::Error::other)?;
    retain(
        manifest["schema_file"]
            .as_str()
            .ok_or_else(|| invalid("missing graph schema file"))?,
    )?;
    for entry in std::fs::read_dir(root.join("shards"))? {
        let entry = entry?;
        let metadata = entry.file_type()?;
        if metadata.is_symlink() || !metadata.is_dir() {
            return Err(invalid("invalid FILE shard directory"));
        }
        if !retained.contains(&entry.file_name()) {
            std::fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}
