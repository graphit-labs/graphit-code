//! Bounded physical consolidation. FILE and graph addresses remain logical.
use super::*;
use graphit_data::graph_writer::{concat_parquet_rows, parquet_rows};

const FILES_PER_BATCH: usize = 128;
type Slice = (String, u64, u64);
type Metadata = BTreeMap<String, u64>;

fn number(member: &Value, field: &str) -> io::Result<u64> {
    member[field]
        .as_u64()
        .ok_or_else(|| invalid(format!("invalid CSR {field}")))
}
fn offset(member: &Value, field: &str) -> io::Result<u64> {
    member.get(field).map_or(Ok(0), |value| {
        value
            .as_u64()
            .ok_or_else(|| invalid(format!("invalid CSR {field}")))
    })
}
fn filename(member: &Value, field: &str) -> io::Result<String> {
    member[field]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid(format!("invalid CSR {field}")))
}
fn groups(paths: &[String], catalog: &Catalog) -> io::Result<BTreeMap<String, Vec<Slice>>> {
    let mut groups = BTreeMap::<String, Vec<Slice>>::new();
    for path in paths {
        let file = &catalog.files[path];
        groups
            .entry("facts".into())
            .or_default()
            .push((file.facts.clone(), file.facts_row, 1));
        for part in &file.nodes {
            groups
                .entry(format!("nodes:{}", part.shard.label))
                .or_default()
                .push((part.shard.file.clone(), part.row_offset, part.shard.rows));
        }
        for member in &file.relationships {
            groups
                .entry(format!("indices:{}", filename(member, "kind")?))
                .or_default()
                .push((
                    filename(member, "indices")?,
                    offset(member, "indices_row_offset")?,
                    number(member, "rows")?,
                ));
            groups
                .entry(format!("indptr:{}", filename(member, "kind")?))
                .or_default()
                .push((
                    filename(member, "indptr")?,
                    offset(member, "indptr_row_offset")?,
                    number(member, "source_rows")?
                        .checked_add(1)
                        .ok_or_else(|| invalid("CSR pointer count overflow"))?,
                ));
        }
    }
    Ok(groups)
}
fn fragmented(
    root: &Path,
    paths: &[String],
    catalog: &Catalog,
    metadata: &mut Metadata,
    original: &SnapshotContext,
) -> io::Result<bool> {
    for slices in groups(paths, catalog)?.into_values() {
        original.check()?;
        let Some((file, _, _)) = slices.first() else {
            continue;
        };
        if slices.iter().any(|slice| &slice.0 != file) {
            return Ok(true);
        }
        let rows = match metadata.get(file) {
            Some(rows) => *rows,
            None => {
                let rows = parquet_rows(&portable::owned_path(root, file)?, original)?;
                metadata.insert(file.clone(), rows);
                rows
            }
        };
        let mut ranges = slices
            .iter()
            .map(|(_, offset, rows)| (*offset, *rows))
            .collect::<Vec<_>>();
        ranges.sort();
        let mut end = 0;
        let mut gap = false;
        for (offset, count) in ranges {
            original.check()?;
            if count == 0 {
                if offset > rows {
                    return Err(invalid("FILE physical slice exceeds Parquet"));
                }
                continue;
            }
            if offset < end {
                return Err(invalid("overlapping FILE physical slices"));
            }
            gap |= offset != end;
            end = offset
                .checked_add(count)
                .filter(|end| *end <= rows)
                .ok_or_else(|| invalid("FILE physical slice exceeds Parquet"))?;
        }
        if gap || end != rows {
            return Ok(true);
        }
    }
    Ok(false)
}
fn next_batch(
    root: &Path,
    catalog: &Catalog,
    metadata: &mut Metadata,
    original: &SnapshotContext,
) -> io::Result<Option<Vec<String>>> {
    // Membership comes from immutable physical facts, not lexical positions.
    // Inserting a path cannot split or shift an unrelated existing bundle.
    let mut bundles = BTreeMap::<String, Vec<String>>::new();
    for (path, file) in &catalog.files {
        original.check()?;
        bundles
            .entry(file.facts.clone())
            .or_default()
            .push(path.clone());
    }
    let mut partial = Vec::new();
    for paths in bundles.into_values() {
        original.check()?;
        if paths.len() >= FILES_PER_BATCH {
            // This writer produces bundles of at most FILES_PER_BATCH. A larger
            // physical input is drained in bounded slices of its own membership.
            for paths in paths.chunks(FILES_PER_BATCH) {
                if fragmented(root, paths, catalog, metadata, original)? {
                    return Ok(Some(paths.to_vec()));
                }
            }
        } else {
            partial.push(paths);
        }
    }
    partial.sort_by(|left, right| left[0].cmp(&right[0]));
    let mut pending = Vec::new();
    for paths in partial {
        if pending.len() + paths.len() > FILES_PER_BATCH {
            if fragmented(root, &pending, catalog, metadata, original)? {
                return Ok(Some(pending));
            }
            pending.clear();
        }
        pending.extend(paths);
    }
    if !pending.is_empty() && fragmented(root, &pending, catalog, metadata, original)? {
        return Ok(Some(pending));
    }
    Ok(None)
}
fn concatenate(
    root: &Path,
    output: &Path,
    slices: Vec<Slice>,
    original: &SnapshotContext,
) -> io::Result<Vec<(u64, u64)>> {
    concat_parquet_rows(
        output,
        slices
            .into_iter()
            .map(|(file, offset, rows)| Ok((portable::owned_path(root, &file)?, offset, rows))),
        original,
    )
}
fn compact(
    root: &Path,
    catalog: &mut Catalog,
    paths: &[String],
    original: &SnapshotContext,
) -> io::Result<()> {
    let (directory, relative) = portable::stage_directory(root)?;
    let facts = paths
        .iter()
        .map(|path| {
            let file = &catalog.files[path];
            (file.facts.clone(), file.facts_row, 1)
        })
        .collect();
    let positions = concatenate(root, &directory.join("facts.parquet"), facts, original)?;
    for (path, (offset, _)) in paths.iter().zip(positions) {
        let file = catalog
            .files
            .get_mut(path)
            .ok_or_else(|| invalid("FILE disappeared"))?;
        file.facts = format!("{relative}/facts.parquet");
        file.facts_row = offset;
    }
    let mut nodes = BTreeMap::<String, Vec<(String, usize)>>::new();
    for path in paths {
        for (index, part) in catalog.files[path].nodes.iter().enumerate() {
            nodes
                .entry(part.shard.label.clone())
                .or_default()
                .push((path.clone(), index));
        }
    }
    for (ordinal, parts) in nodes.into_values().enumerate() {
        original.check()?;
        let mut columns = BTreeMap::new();
        let slices = parts
            .iter()
            .map(|(path, index)| {
                let part = &catalog.files[path].nodes[*index];
                for (name, kind) in &part.shard.columns {
                    if columns
                        .insert(name.clone(), kind.clone())
                        .is_some_and(|old| old != *kind)
                    {
                        return Err(invalid("incompatible FILE node column"));
                    }
                }
                Ok((part.shard.file.clone(), part.row_offset, part.shard.rows))
            })
            .collect::<io::Result<Vec<_>>>()?;
        let name = format!("nodes-{ordinal}.parquet");
        let positions = concatenate(root, &directory.join(&name), slices, original)?;
        for ((path, index), (offset, rows)) in parts.into_iter().zip(positions) {
            let part = &mut catalog
                .files
                .get_mut(&path)
                .ok_or_else(|| invalid("FILE disappeared"))?
                .nodes[index];
            if part.shard.rows != rows {
                return Err(invalid("consolidated FILE node rows differ"));
            }
            part.shard.file = format!("{relative}/{name}");
            part.row_offset = offset;
            part.shard.columns = columns.clone();
        }
    }
    let mut kinds = BTreeMap::<String, Vec<(String, usize)>>::new();
    for path in paths {
        for (index, member) in catalog.files[path].relationships.iter().enumerate() {
            kinds
                .entry(filename(member, "kind")?)
                .or_default()
                .push((path.clone(), index));
        }
    }
    for (kind_index, members) in kinds.into_values().enumerate() {
        let mut indices = Vec::new();
        let mut indptr = Vec::new();
        for (path, index) in &members {
            let member = &catalog.files[path].relationships[*index];
            indices.push((
                filename(member, "indices")?,
                offset(member, "indices_row_offset")?,
                number(member, "rows")?,
            ));
            indptr.push((
                filename(member, "indptr")?,
                offset(member, "indptr_row_offset")?,
                number(member, "source_rows")?
                    .checked_add(1)
                    .ok_or_else(|| invalid("CSR pointer count overflow"))?,
            ));
        }
        let indices_name = format!("edges-{kind_index}.indices");
        let indptr_name = format!("edges-{kind_index}.indptr");
        let indices = concatenate(root, &directory.join(&indices_name), indices, original)?;
        let indptr = concatenate(root, &directory.join(&indptr_name), indptr, original)?;
        for (ordinal, ((path, index), ((indices_offset, _), (indptr_offset, _)))) in members
            .into_iter()
            .zip(indices.into_iter().zip(indptr))
            .enumerate()
        {
            let member = &mut catalog
                .files
                .get_mut(&path)
                .ok_or_else(|| invalid("FILE disappeared"))?
                .relationships[index];
            member["table"] = json!(format!("{relative}/member-{kind_index}-{ordinal}"));
            member["indices"] = json!(format!("{relative}/{indices_name}"));
            member["indptr"] = json!(format!("{relative}/{indptr_name}"));
            member["indices_row_offset"] = json!(indices_offset);
            member["indptr_row_offset"] = json!(indptr_offset);
        }
    }
    Ok(())
}

impl LiveGraph {
    /// Consolidate at most one batch, holding the existing publication fence.
    /// Call again only through the composing caller's existing maintenance cadence.
    pub fn maintain_shards(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<LiveGraphMaintenance> {
        original.check()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| invalid("graph writer poisoned"))?;
        original.check()?;
        let mut metadata = Metadata::new();
        let batch = next_batch(&self.directory, &state.catalog, &mut metadata, original)?;
        let consolidated_files = batch.as_ref().map_or(0, |paths| paths.len() as u64);
        if let Some(paths) = batch {
            let mut next = state.catalog.clone();
            compact(&self.directory, &mut next, &paths, original)?;
            next.schema.revision = next
                .schema
                .revision
                .checked_add(1)
                .ok_or_else(|| invalid("graph revision overflow"))?;
            original.check()?;
            portable::publish(&self.directory, &next, original)?;
            // Publish is the effect: cache must follow it even if cancelled now.
            state.catalog = next;
        }
        let committed = |error: io::Error| {
            io::Error::new(
                error.kind(),
                format!(
                    "maintenance {}graph revision {}: {error}",
                    if consolidated_files == 0 {
                        "at "
                    } else {
                        "committed at "
                    },
                    state.catalog.schema.revision
                ),
            )
        };
        original.check().map_err(&committed)?;
        let more = consolidated_files != 0
            && next_batch(&self.directory, &state.catalog, &mut metadata, original)
                .map_err(&committed)?
                .is_some();
        // Intermediate batches leave obsolete files intact. The final call does
        // one guarded collection, including staging left by interrupted writers.
        let reclaimed = if more {
            portable::Reclaimed::default()
        } else {
            portable::prune(&self.directory, &state.catalog, original).map_err(&committed)?
        };
        original.check().map_err(committed)?;
        Ok(LiveGraphMaintenance {
            revision: state.catalog.schema.revision,
            files: state.catalog.files.len() as u64,
            consolidated_files,
            more,
            reclaimed_files: reclaimed.files,
            reclaimed_directories: reclaimed.directories,
            reclaimed_bytes: reclaimed.bytes,
        })
    }
}
