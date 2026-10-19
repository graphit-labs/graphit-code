use super::{Member, RemoteIcebugGraph, Result, column, integer, text, uint, wait};
use crate::{
    DataReadError, EdgeId, GraphAnchorKey, GraphDirection, GraphNeighborhoodLimits,
    GraphNeighborhoodPage, GraphNeighborhoodRequest, GraphProperty, GraphPropertyEdge,
    GraphPropertyNode, LocalGraphLimits, NodeId, PrimaryKey, bounded,
    neighborhood::{Candidate, PropertyValue, fingerprint, selected_rows},
    parquet::{Scalar, Work},
};
use arrow_array_parquet::{Array, BooleanArray, Int64Array, RecordBatch, StringArray};
use futures::StreamExt;
use graphit_core::file_io::SnapshotContext;
use parquet::arrow::arrow_reader::RowSelection;
use std::{
    collections::{BinaryHeap, HashSet},
    sync::Arc,
};

impl RemoteIcebugGraph {
    /// A UID-keyset page over the requested relationship and its selected properties.
    pub async fn neighborhood_graph(
        &self,
        request: GraphNeighborhoodRequest<'_>,
        limits: GraphNeighborhoodLimits,
        original: Arc<SnapshotContext>,
    ) -> Result<GraphNeighborhoodPage> {
        original.check()?;
        if !(1..=100).contains(&request.limit)
            || [
                request.anchor_label,
                request.relationship,
                request.other_label,
            ]
            .iter()
            .any(|name| name.is_empty() || name.len() > self.policy.0.max_name_bytes)
        {
            return Err(DataReadError::InvalidNeighborhoodRequest.into());
        }
        if let GraphAnchorKey::String(key) = request.anchor_key {
            bounded(key.len(), limits.max_string_bytes, "anchor key")?;
        }
        let find = |label| {
            self.manifest
                .node_tables
                .iter()
                .position(|node| node.label == label)
                .ok_or(DataReadError::UnknownNodeLabel)
        };
        let anchor_table = find(request.anchor_label)?;
        let other = find(request.other_label)?;
        if !self
            .manifest
            .relationships
            .iter()
            .any(|group| group.kind == request.relationship)
        {
            return Err(DataReadError::UnknownRelationship.into());
        }
        let matches = |member: &Member| {
            member.relationship == request.relationship
                && match request.direction {
                    GraphDirection::Outgoing => member.from == anchor_table && member.to == other,
                    GraphDirection::Incoming => member.to == anchor_table && member.from == other,
                }
        };
        if !self.members.iter().any(matches) {
            return Err(DataReadError::UnavailableRelationshipEndpoint.into());
        }
        let query = fingerprint(&request)?;
        if let Some(cursor) = request.cursor
            && (cursor.generation() != self.generation
                || cursor.query_fingerprint() != query
                || cursor.last_uid().is_empty()
                || cursor.last_uid().len() > limits.max_string_bytes)
        {
            return Err(DataReadError::InvalidCursor.into());
        }
        let work = Work::new(limits.decode);
        let anchor = self
            .find_anchor(anchor_table, request.anchor_key, limits, &work, &original)
            .await?;
        let top = request.limit + 1;
        let mut heap = BinaryHeap::<Candidate>::new();
        heap.try_reserve_exact(top)
            .map_err(|_| DataReadError::LimitExceeded("candidate allocation"))?;
        let mut matching = 0usize;
        for (index, member) in self
            .members
            .iter()
            .enumerate()
            .filter(|(_, member)| matches(member))
        {
            let (selection, mut row, expected_end) = match request.direction {
                GraphDirection::Outgoing => {
                    let (begin, end) =
                        pointer_range(member, anchor, limits, self.policy.0, &work, &original)
                            .await?;
                    (
                        Some(RowSelection::from_consecutive_ranges(
                            std::iter::once(
                                usize::try_from(begin)
                                    .map_err(|_| DataReadError::LimitExceeded("row selection"))?
                                    ..usize::try_from(end).map_err(|_| {
                                        DataReadError::LimitExceeded("row selection")
                                    })?,
                            ),
                            usize::try_from(member.indices.rows)
                                .map_err(|_| DataReadError::LimitExceeded("row selection"))?,
                        )),
                        begin,
                        end,
                    )
                }
                GraphDirection::Incoming => (None, 0, member.indices.rows),
            };
            let mut stream = member.indices.stream(
                &["target", "uid"],
                selection,
                self.policy.0,
                limits.decode.row_batch_size,
                Arc::clone(&work),
                Arc::clone(&original),
            )?;
            while let Some(batch) = wait(&original, async {
                stream.next().await.transpose().map_err(Into::into)
            })
            .await?
            {
                work.rows(batch.num_rows())?;
                for offset in 0..batch.num_rows() {
                    let local_target = uint(&batch, "target", offset)?;
                    if local_target >= member.target_rows {
                        return Err(DataReadError::InvalidCsr("target shard bounds").into());
                    }
                    let target = member.target_row_base + local_target;
                    if !self.nodes[member.to].contains(target) {
                        return Err(DataReadError::InvalidCsr("target bounds").into());
                    }
                    let uid = text(&batch, "uid", offset)?
                        .filter(|uid| !uid.is_empty())
                        .ok_or(DataReadError::InvalidCsr("edge UID"))?;
                    bounded(uid.len(), limits.max_string_bytes, "edge UID")?;
                    let physical = row;
                    row += 1;
                    if request.direction == GraphDirection::Incoming && target != anchor {
                        continue;
                    }
                    matching = matching
                        .checked_add(1)
                        .ok_or(DataReadError::LimitExceeded("matching edges"))?;
                    bounded(matching, limits.max_matching_edges, "matching edges")?;
                    if request
                        .cursor
                        .is_some_and(|cursor| uid <= cursor.last_uid())
                    {
                        continue;
                    }
                    if heap.iter().any(|candidate| candidate.uid == uid) {
                        return Err(DataReadError::DuplicateEdgeUid.into());
                    }
                    if heap.len() == top
                        && heap
                            .peek()
                            .is_some_and(|candidate| uid >= candidate.uid.as_str())
                    {
                        continue;
                    }
                    if heap.len() == top {
                        heap.pop();
                    }
                    heap.push(Candidate {
                        uid: limits.text(uid)?,
                        member: index,
                        row: physical,
                        target,
                        source: anchor,
                    });
                }
                original.check()?;
            }
            if row != expected_end {
                return Err(DataReadError::InvalidCsr("edge row count").into());
            }
        }
        let mut candidates = heap.into_sorted_vec();
        let has_more = candidates.len() > request.limit;
        if has_more {
            candidates.pop();
        }
        if request.direction == GraphDirection::Incoming {
            for (index, member) in self.members.iter().enumerate() {
                let rows = selected_rows(&candidates, index)?;
                if !rows.is_empty() {
                    map_sources(
                        member,
                        &rows,
                        &mut candidates,
                        limits,
                        self.policy.0,
                        &work,
                        &original,
                    )
                    .await?;
                }
            }
        }
        let anchor_id = self.node_id(anchor_table, anchor);
        let mut endpoints = vec![anchor_id];
        for candidate in &candidates {
            let member = &self.members[candidate.member];
            endpoints.push(self.node_id(member.from, candidate.source));
            endpoints.push(self.node_id(member.to, candidate.target));
        }
        endpoints.sort_unstable();
        endpoints.dedup();
        let edges = self
            .edge_properties(&candidates, limits, &work, &original)
            .await?;
        let nodes = self
            .node_properties(&endpoints, limits, &work, &original)
            .await?;
        original.check()?;
        Ok(GraphNeighborhoodPage {
            generation: self.generation,
            fingerprint: query,
            anchor: anchor_id,
            nodes,
            edges,
            has_more,
        })
    }

    async fn find_anchor(
        &self,
        table: usize,
        key: GraphAnchorKey<'_>,
        limits: GraphNeighborhoodLimits,
        work: &Arc<Work>,
        original: &Arc<SnapshotContext>,
    ) -> Result<u64> {
        let descriptor = &self.manifest.node_tables[table];
        let data = &self.nodes[table];
        if data.rows == 0 {
            return Err(DataReadError::AnchorNotFound.into());
        }
        let kind = data
            .fields
            .iter()
            .find(|(name, _)| name == &descriptor.primary_key)
            .map(|(_, kind)| kind);
        if !matches!(
            (kind, key),
            (Some(Scalar::String), GraphAnchorKey::String(_))
                | (Some(Scalar::Int64), GraphAnchorKey::Int64(_))
        ) {
            return Err(DataReadError::InvalidAnchorKey.into());
        }
        let mut stream = data.stream(
            &[&descriptor.primary_key],
            None,
            self.policy.0,
            limits.decode.row_batch_size,
            Arc::clone(work),
            Arc::clone(original),
        )?;
        let mut found = None;
        let mut row = 0;
        while let Some(batch) = wait(original, async {
            stream.next().await.transpose().map_err(Into::into)
        })
        .await?
        {
            work.rows(batch.num_rows())?;
            for offset in 0..batch.num_rows() {
                let matched = match key {
                    GraphAnchorKey::String(wanted) => {
                        let value = text(&batch, &descriptor.primary_key, offset)?
                            .ok_or(DataReadError::InvalidParquet("nonnull primary key"))?;
                        bounded(value.len(), limits.max_string_bytes, "primary key")?;
                        value == wanted
                    }
                    GraphAnchorKey::Int64(wanted) => {
                        integer(&batch, &descriptor.primary_key, offset)?
                            .ok_or(DataReadError::InvalidParquet("nonnull primary key"))?
                            == wanted
                    }
                };
                if matched && found.replace(data.offset(row)?).is_some() {
                    return Err(DataReadError::DuplicatePrimaryKey.into());
                }
                row += 1;
            }
            original.check()?;
        }
        found.ok_or_else(|| DataReadError::AnchorNotFound.into())
    }

    async fn edge_properties(
        &self,
        candidates: &[Candidate],
        limits: GraphNeighborhoodLimits,
        work: &Arc<Work>,
        original: &Arc<SnapshotContext>,
    ) -> Result<Vec<GraphPropertyEdge>> {
        let mut edges = Vec::new();
        for (index, member) in self.members.iter().enumerate() {
            let rows = selected_rows(candidates, index)?;
            if rows.is_empty() {
                continue;
            }
            let mut names = member
                .indices
                .fields
                .iter()
                .filter(|(name, _)| name != "target")
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>();
            names.sort_unstable();
            bounded(
                names.len(),
                limits.max_properties_per_record,
                "record properties",
            )?;
            let uid_index = names
                .binary_search(&"uid")
                .map_err(|_| DataReadError::InvalidCsr("UID property"))?;
            let mut projection = names.clone();
            projection.push("target");
            let physical = rows.iter().map(|(row, _)| *row).collect::<Vec<_>>();
            let mut stream = member.indices.stream(
                &projection,
                Some(member.indices.select_rows(&physical)?),
                self.policy.0,
                limits.decode.row_batch_size,
                Arc::clone(work),
                Arc::clone(original),
            )?;
            let mut next = 0;
            while let Some(batch) = wait(original, async {
                stream.next().await.transpose().map_err(Into::into)
            })
            .await?
            {
                work.rows(batch.num_rows())?;
                for offset in 0..batch.num_rows() {
                    let (_, candidate_index) = *rows
                        .get(next)
                        .ok_or(DataReadError::InvalidCsr("selected edge count"))?;
                    let candidate = &candidates[candidate_index];
                    if member.target_row_base + uint(&batch, "target", offset)? != candidate.target
                        || text(&batch, "uid", offset)? != Some(candidate.uid.as_str())
                    {
                        return Err(DataReadError::InvalidCsr("edge differential").into());
                    }
                    edges.push(GraphPropertyEdge {
                        id: EdgeId {
                            generation: self.generation,
                            member: index as u32,
                            row: candidate.row,
                        },
                        from: self.node_id(member.from, candidate.source),
                        to: self.node_id(member.to, candidate.target),
                        relationship: limits.text(&member.relationship)?,
                        properties: properties(&batch, &names, offset, limits)?,
                        uid_index,
                    });
                    next += 1;
                }
                original.check()?;
            }
            if next != rows.len() {
                return Err(DataReadError::InvalidCsr("selected edge count").into());
            }
        }
        edges.sort_unstable_by(|a, b| a.uid().cmp(b.uid()));
        Ok(edges)
    }

    async fn node_properties(
        &self,
        endpoints: &[NodeId],
        limits: GraphNeighborhoodLimits,
        work: &Arc<Work>,
        original: &Arc<SnapshotContext>,
    ) -> Result<Vec<GraphPropertyNode>> {
        let mut nodes = Vec::new();
        for (index, table) in self.nodes.iter().enumerate() {
            let rows = endpoints
                .iter()
                .filter(|id| id.table as usize == index)
                .map(|id| id.row)
                .collect::<Vec<_>>();
            if rows.is_empty() {
                continue;
            }
            let descriptor = &self.manifest.node_tables[index];
            let mut names = table
                .fields
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>();
            names.sort_unstable();
            bounded(
                names.len(),
                limits.max_properties_per_record,
                "record properties",
            )?;
            let mut stream = table.stream(
                &names,
                Some(table.select_rows(&rows)?),
                self.policy.0,
                limits.decode.row_batch_size,
                Arc::clone(work),
                Arc::clone(original),
            )?;
            let mut next = 0;
            let mut keys = HashSet::new();
            while let Some(batch) = wait(original, async {
                stream.next().await.transpose().map_err(Into::into)
            })
            .await?
            {
                work.rows(batch.num_rows())?;
                for offset in 0..batch.num_rows() {
                    let key = column(&batch, &descriptor.primary_key)?;
                    let key = if let Some(value) = key.as_any().downcast_ref::<StringArray>() {
                        if value.is_null(offset) {
                            return Err(DataReadError::InvalidParquet("nonnull primary key").into());
                        }
                        PrimaryKey::String(limits.text(value.value(offset))?)
                    } else if let Some(value) = key.as_any().downcast_ref::<Int64Array>() {
                        if value.is_null(offset) {
                            return Err(DataReadError::InvalidParquet("nonnull primary key").into());
                        }
                        PrimaryKey::Int64(value.value(offset))
                    } else {
                        return Err(DataReadError::InvalidParquet("primary key type").into());
                    };
                    if !keys.insert(key) {
                        return Err(DataReadError::DuplicatePrimaryKey.into());
                    }
                    let row = *rows
                        .get(next)
                        .ok_or(DataReadError::InvalidParquet("selected node count"))?;
                    nodes.push(GraphPropertyNode {
                        id: self.node_id(index, row),
                        label: limits.text(&descriptor.label)?,
                        properties: properties(&batch, &names, offset, limits)?,
                    });
                    next += 1;
                }
                original.check()?;
            }
            if next != rows.len() {
                return Err(DataReadError::InvalidParquet("selected node count").into());
            }
        }
        nodes.sort_unstable_by_key(|node| node.id);
        Ok(nodes)
    }
}

async fn pointer_range(
    member: &Member,
    anchor: u64,
    limits: GraphNeighborhoodLimits,
    policy: LocalGraphLimits,
    work: &Arc<Work>,
    original: &Arc<SnapshotContext>,
) -> Result<(u64, u64)> {
    if anchor < member.source_row_base || anchor - member.source_row_base >= member.source_rows {
        return Ok((0, 0));
    }
    let anchor = anchor - member.source_row_base;
    let rows = [anchor, anchor + 1];
    let mut stream = member.indptr.stream(
        &["ptr"],
        Some(member.indptr.select_rows(&rows)?),
        policy,
        limits.decode.row_batch_size,
        Arc::clone(work),
        Arc::clone(original),
    )?;
    let mut pointers = Vec::with_capacity(2);
    while let Some(batch) = wait(original, async {
        stream.next().await.transpose().map_err(Into::into)
    })
    .await?
    {
        work.rows(batch.num_rows())?;
        for offset in 0..batch.num_rows() {
            pointers.push(uint(&batch, "ptr", offset)?);
        }
        original.check()?;
    }
    if pointers.len() != 2
        || pointers[0] > pointers[1]
        || pointers[1] > member.indices.rows
        || anchor == 0 && pointers[0] != 0
        || anchor + 2 == member.indptr.rows && pointers[1] != member.indices.rows
    {
        return Err(DataReadError::InvalidCsr("row pointer").into());
    }
    Ok((pointers[0], pointers[1]))
}

async fn map_sources(
    member: &Member,
    rows: &[(u64, usize)],
    candidates: &mut [Candidate],
    limits: GraphNeighborhoodLimits,
    policy: LocalGraphLimits,
    work: &Arc<Work>,
    original: &Arc<SnapshotContext>,
) -> Result<()> {
    let mut stream = member.indptr.stream(
        &["ptr"],
        None,
        policy,
        limits.decode.row_batch_size,
        Arc::clone(work),
        Arc::clone(original),
    )?;
    let mut source = 0;
    let mut previous = 0;
    let mut next = 0;
    while let Some(batch) = wait(original, async {
        stream.next().await.transpose().map_err(Into::into)
    })
    .await?
    {
        work.rows(batch.num_rows())?;
        for offset in 0..batch.num_rows() {
            let pointer = uint(&batch, "ptr", offset)?;
            if source == 0 && pointer != 0 || pointer < previous || pointer > member.indices.rows {
                return Err(DataReadError::InvalidCsr("row pointer").into());
            }
            if source > 0 {
                while next < rows.len() && rows[next].0 < pointer {
                    if rows[next].0 < previous {
                        return Err(DataReadError::InvalidCsr("edge alignment").into());
                    }
                    candidates[rows[next].1].source = member.source_row_base + source - 1;
                    next += 1;
                }
            }
            previous = pointer;
            source += 1;
            if next == rows.len() {
                original.check()?;
                return Ok(());
            }
        }
        original.check()?;
    }
    Err(DataReadError::InvalidCsr("missing endpoint").into())
}

fn properties(
    batch: &RecordBatch,
    names: &[&str],
    row: usize,
    limits: GraphNeighborhoodLimits,
) -> Result<Vec<GraphProperty>> {
    bounded(
        names.len(),
        limits.max_properties_per_record,
        "record properties",
    )?;
    names
        .iter()
        .map(|name| {
            let value = column(batch, name)?;
            let value = if value.is_null(row) {
                PropertyValue::Null
            } else if let Some(value) = value.as_any().downcast_ref::<StringArray>() {
                PropertyValue::String(limits.text(value.value(row))?)
            } else if let Some(value) = value.as_any().downcast_ref::<Int64Array>() {
                PropertyValue::Int64(value.value(row))
            } else if let Some(value) = value.as_any().downcast_ref::<BooleanArray>() {
                PropertyValue::Bool(value.value(row))
            } else {
                return Err(DataReadError::InvalidParquet("property scalar").into());
            };
            Ok(GraphProperty {
                name: limits.text(name)?,
                value,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
