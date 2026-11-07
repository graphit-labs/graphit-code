use super::{Member, RemoteIcebugGraph, Result, column, integer, text, uint, wait};
use crate::policy::DecodeLimits;
use crate::{
    DataReadError, EdgeId, GraphAnchorKey, GraphDirection, GraphNeighborhoodLimits,
    GraphNeighborhoodPage, GraphNeighborhoodRequest, GraphProperty, GraphPropertyEdge,
    GraphPropertyNode, GraphSchemaSource, LocalGraphLimits, NodeId, PrimaryKey, ResolvedGraphNode,
    bounded,
    neighborhood::{
        Candidate, CsrMemberView, OwnedNeighborhoodPosition, PropertyValue, RawCandidate,
        ReverseRequests, append_group, candidate_prefix, fingerprint, plan_traversals,
        property_text, query_decode_limits, query_projection, selected_rows, validate_cursor,
    },
    parquet::{LookupKey, Scalar, Work},
};
use arrow_array_parquet::{Array, BooleanArray, Int64Array, RecordBatch, StringArray};
use futures::StreamExt;
use graphit_core::file_io::SnapshotContext;
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};

impl RemoteIcebugGraph {
    /// Return the exact input policy retained by this immutable graph pin.
    pub fn input_limits(&self) -> LocalGraphLimits {
        self.policy.limits()
    }

    /// Resolve a key against this immutable graph pin with a fresh operation.
    pub async fn resolve_node(
        &self,
        label: &str,
        key: GraphAnchorKey<'_>,
        limits: GraphNeighborhoodLimits,
        original: Arc<SnapshotContext>,
    ) -> Result<ResolvedGraphNode<'_>> {
        self.resolve_node_with_decode(label, key, limits.max_string_bytes, limits.decode, original)
            .await
    }
    /// Resolve a query PK using this pin's policy and an explicit SDK batch.
    /// An absent key returns None; other failures retain their original cause.
    pub async fn resolve_query_node(
        &self,
        label: &str,
        key: GraphAnchorKey<'_>,
        row_batch_size: usize,
        original: Arc<SnapshotContext>,
    ) -> Result<Option<ResolvedGraphNode<'_>>> {
        original.check()?;
        let input = self.input_limits();
        match self
            .resolve_node_with_decode(
                label,
                key,
                input.max_chunk_bytes,
                query_decode_limits(input, row_batch_size)?,
                original,
            )
            .await
        {
            Ok(node) => Ok(Some(node)),
            Err(super::RemoteGraphError::Data(DataReadError::AnchorNotFound)) => Ok(None),
            Err(error) => Err(error),
        }
    }
    /// Read selected properties and the declared PK from this pin's resolved node.
    pub async fn read_query_node(
        &self,
        anchor: &ResolvedGraphNode<'_>,
        columns: &[&str],
        row_batch_size: usize,
        original: Arc<SnapshotContext>,
    ) -> Result<GraphPropertyNode> {
        original.check()?;
        if !matches!(anchor.graph, GraphSchemaSource::Remote(graph) if std::ptr::eq(graph, self)) {
            return Err(DataReadError::InvalidAnchorKey.into());
        }
        let index = anchor.id.table as usize;
        let table = self
            .nodes
            .get(index)
            .ok_or(DataReadError::InvalidAnchorKey)?;
        let descriptor = &self.manifest.node_tables[index];
        let input = self.input_limits();
        let decode = query_decode_limits(input, row_batch_size)?;
        let names = query_projection(&table.fields, &descriptor.primary_key, columns, input)?;
        let primary_key_index = names
            .binary_search(&descriptor.primary_key.as_str())
            .map_err(|_| DataReadError::InvalidParquet("primary key"))?;
        let work = Work::new(decode);
        let result = async {
            let mut stream = table.stream(
                &names,
                Some(table.select_rows(&[anchor.id.row])?),
                input,
                decode.row_batch_size,
                Arc::clone(&work),
                Arc::clone(&original),
            )?;
            let mut found = None;
            while let Some(batch) = wait(&original, async {
                stream.next().await.transpose().map_err(Into::into)
            })
            .await?
            {
                work.rows(batch.num_rows())?;
                for row in 0..batch.num_rows() {
                    let properties = query_properties(&batch, &names, row, input.max_chunk_bytes)?;
                    if !matches!(
                        properties[primary_key_index].value,
                        PropertyValue::String(_) | PropertyValue::Int64(_)
                    ) {
                        return Err(DataReadError::InvalidParquet("nonnull primary key").into());
                    }
                    if found.is_some() {
                        return Err(DataReadError::InvalidParquet("selected node count").into());
                    }
                    found = Some(GraphPropertyNode {
                        id: anchor.id,
                        label: property_text(&descriptor.label, input.max_name_bytes)?,
                        properties,
                        primary_key_index,
                    });
                }
                original.check()?;
            }
            found.ok_or_else(|| DataReadError::InvalidParquet("selected node count").into())
        }
        .await;
        original.check()?;
        result
    }

    async fn resolve_node_with_decode(
        &self,
        label: &str,
        key: GraphAnchorKey<'_>,
        max_string_bytes: usize,
        decode: DecodeLimits,
        original: Arc<SnapshotContext>,
    ) -> Result<ResolvedGraphNode<'_>> {
        original.check()?;
        if label.is_empty() || label.len() > self.policy.0.max_name_bytes {
            return Err(DataReadError::InvalidNeighborhoodRequest.into());
        }
        if let GraphAnchorKey::String(key) = key {
            bounded(key.len(), max_string_bytes, "anchor key")?;
        }
        let table = self
            .manifest
            .node_tables
            .iter()
            .position(|node| node.label == label)
            .ok_or(DataReadError::UnknownNodeLabel)?;
        let work = Work::new(decode);
        let row = self
            .find_anchor(
                table,
                key,
                max_string_bytes,
                decode.row_batch_size,
                &work,
                &original,
            )
            .await;
        original.check()?;
        Ok(ResolvedGraphNode {
            graph: GraphSchemaSource::Remote(self),
            id: self.node_id(table, row?),
        })
    }
    /// A UID-keyset page over the requested relationship and its selected properties.
    pub async fn neighborhood_graph(
        &self,
        request: GraphNeighborhoodRequest<'_>,
        limits: GraphNeighborhoodLimits,
        original: Arc<SnapshotContext>,
    ) -> Result<GraphNeighborhoodPage> {
        let result = self
            .neighborhood_graph_inner(None, request, limits, Arc::clone(&original))
            .await;
        original.check()?;
        result
    }
    /// Reuse an anchor from this graph pin without another primary-key lookup.
    pub async fn neighborhood_graph_at(
        &self,
        anchor: &ResolvedGraphNode<'_>,
        request: GraphNeighborhoodRequest<'_>,
        limits: GraphNeighborhoodLimits,
        original: Arc<SnapshotContext>,
    ) -> Result<GraphNeighborhoodPage> {
        original.check()?;
        if !matches!(anchor.graph, GraphSchemaSource::Remote(graph) if std::ptr::eq(graph, self)) {
            return Err(DataReadError::InvalidAnchorKey.into());
        }
        let result = self
            .neighborhood_graph_inner(Some(anchor.id), request, limits, Arc::clone(&original))
            .await;
        original.check()?;
        result
    }
    async fn neighborhood_graph_inner(
        &self,
        resolved: Option<NodeId>,
        request: GraphNeighborhoodRequest<'_>,
        limits: GraphNeighborhoodLimits,
        original: Arc<SnapshotContext>,
    ) -> Result<GraphNeighborhoodPage> {
        original.check()?;
        if !(1..=1000).contains(&request.limit) {
            return Err(DataReadError::InvalidNeighborhoodRequest.into());
        }
        for name in std::iter::once(request.anchor_label)
            .chain(request.relationship)
            .chain(request.other_label)
        {
            if name.is_empty() || name.len() > self.policy.0.max_name_bytes {
                return Err(DataReadError::InvalidNeighborhoodRequest.into());
            }
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
        let other = request.other_label.map(find).transpose()?;
        if resolved.is_some_and(|anchor| {
            anchor.table as usize != anchor_table || !self.nodes[anchor_table].contains(anchor.row)
        }) {
            return Err(DataReadError::InvalidAnchorKey.into());
        }
        if request.relationship.is_some_and(|kind| {
            !self
                .manifest
                .relationships
                .iter()
                .any(|group| group.kind == kind)
        }) {
            return Err(DataReadError::UnknownRelationship.into());
        }
        let relevant = |m: &Member| {
            request
                .relationship
                .is_none_or(|kind| kind == m.relationship)
                && ((request.direction != Some(GraphDirection::Incoming)
                    && m.from == anchor_table
                    && other.is_none_or(|other| m.to == other))
                    || (request.direction != Some(GraphDirection::Outgoing)
                        && m.to == anchor_table
                        && other.is_none_or(|other| m.from == other)))
        };
        if (request.relationship.is_some() || request.other_label.is_some())
            && !self.members.iter().any(relevant)
        {
            return Err(DataReadError::UnavailableRelationshipEndpoint.into());
        }
        let data = &self.nodes[anchor_table];
        let pk = &self.manifest.node_tables[anchor_table].primary_key;
        let kind = data
            .fields
            .iter()
            .find(|(name, _)| name == pk)
            .map(|(_, kind)| kind);
        if !(data.fields.is_empty() && data.rows == 0)
            && !matches!(
                (kind, request.anchor_key),
                (Some(Scalar::String), GraphAnchorKey::String(_))
                    | (Some(Scalar::Int64), GraphAnchorKey::Int64(_))
            )
        {
            return Err(DataReadError::InvalidAnchorKey.into());
        }
        let query = fingerprint(&request)?;
        validate_cursor(
            &request,
            query,
            self.policy.0.max_name_bytes,
            limits.max_string_bytes,
        )?;
        let work = Work::new(limits.decode);
        let anchor = if let Some(anchor) = resolved {
            let mut stream = data.stream(
                &[pk],
                Some(data.select_rows(&[anchor.row])?),
                self.policy.0,
                limits.decode.row_batch_size,
                Arc::clone(&work),
                Arc::clone(&original),
            )?;
            let mut count = 0;
            while let Some(batch) = wait(&original, async {
                stream.next().await.transpose().map_err(Into::into)
            })
            .await?
            {
                work.rows(batch.num_rows())?;
                for row in 0..batch.num_rows() {
                    let matches = match request.anchor_key {
                        GraphAnchorKey::String(key) => text(&batch, pk, row)? == Some(key),
                        GraphAnchorKey::Int64(key) => integer(&batch, pk, row)? == Some(key),
                    };
                    if !matches {
                        return Err(DataReadError::InvalidAnchorKey.into());
                    }
                    count += 1;
                }
            }
            if count != 1 {
                return Err(DataReadError::InvalidAnchorKey.into());
            }
            anchor.row
        } else {
            self.find_anchor(
                anchor_table,
                request.anchor_key,
                limits.max_string_bytes,
                limits.decode.row_batch_size,
                &work,
                &original,
            )
            .await?
        };
        let plan = plan_traversals(
            &request,
            anchor_table,
            other,
            self.members
                .iter()
                .enumerate()
                .map(|(index, member)| member_view(index, member)),
            self.reverse_members
                .iter()
                .enumerate()
                .map(|(index, member)| member_view(index, member)),
        )?;
        let mut raw = Vec::new();
        for traversal in plan {
            let index = traversal.member;
            let member = if traversal.reverse {
                &self.reverse_members[index]
            } else {
                &self.members[index]
            };
            let ranges = if !traversal.incoming || traversal.reverse {
                let (begin, end) =
                    pointer_range(member, anchor, limits, self.policy.0, &work, &original).await?;
                std::iter::once(begin..end).collect::<Vec<_>>()
            } else {
                let Some(target) = anchor
                    .checked_sub(member.target_row_base)
                    .filter(|row| *row < member.target_rows)
                else {
                    continue;
                };
                member
                    .indices
                    .key_ranges("target", &[LookupKey::UInt64(target)])?
            };
            let skip_loop = if traversal.incoming
                && !traversal.reverse
                && request.direction.is_none()
                && member.from == anchor_table
            {
                Some(pointer_range(member, anchor, limits, self.policy.0, &work, &original).await?)
            } else {
                None
            };
            let mut ordinals = ranges.iter().flat_map(|range| range.clone());
            let mut stream = member.indices.stream(
                &["target", "uid"],
                Some(member.indices.select_ranges(&ranges)?),
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
                    let local = uint(&batch, "target", offset)?;
                    if local >= member.target_rows {
                        return Err(DataReadError::InvalidCsr("target shard bounds").into());
                    }
                    let target = member.target_row_base + local;
                    if !self.nodes[member.to].contains(target) {
                        return Err(DataReadError::InvalidCsr("target bounds").into());
                    }
                    let uid = text(&batch, "uid", offset)?
                        .filter(|uid| !uid.is_empty())
                        .ok_or(DataReadError::InvalidCsr("edge UID"))?;
                    bounded(uid.len(), limits.max_string_bytes, "edge UID")?;
                    let row = ordinals
                        .next()
                        .ok_or(DataReadError::InvalidCsr("edge row count"))?;
                    if traversal.incoming && !traversal.reverse && target != anchor {
                        continue;
                    }
                    if traversal.incoming
                        && request.direction.is_none()
                        && (skip_loop.is_some_and(|(begin, end)| row >= begin && row < end)
                            || traversal.reverse && member.to == anchor_table && target == anchor)
                    {
                        continue;
                    }
                    bounded(
                        raw.len()
                            .checked_add(1)
                            .ok_or(DataReadError::LimitExceeded("matching edges"))?,
                        limits.max_matching_edges,
                        "matching edges",
                    )?;
                    raw.try_reserve(1)
                        .map_err(|_| DataReadError::LimitExceeded("matching edges"))?;
                    raw.push(RawCandidate {
                        uid: limits.text(uid)?,
                        member: index,
                        row,
                        source: if traversal.reverse { target } else { anchor },
                        target: if traversal.reverse { anchor } else { target },
                        reverse: traversal.reverse,
                        incoming: traversal.incoming,
                    });
                }
                original.check()?;
            }
            if ordinals.next().is_some() {
                return Err(DataReadError::InvalidCsr("edge row count").into());
            }
        }
        let raw = candidate_prefix(raw, &request)?;
        let group = self.position_group(raw, limits, &work, &original).await?;
        let mut candidates = Vec::new();
        append_group(&mut candidates, group, &request)?;
        let has_more = candidates.len() > request.limit;
        if has_more {
            candidates.pop();
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
            fingerprint: query,
            anchor: anchor_id,
            nodes,
            edges,
            has_more,
        })
    }

    async fn position_group(
        &self,
        mut group: Vec<RawCandidate>,
        limits: GraphNeighborhoodLimits,
        work: &Arc<Work>,
        original: &Arc<SnapshotContext>,
    ) -> Result<Vec<Candidate>> {
        let mut used = HashSet::new();
        used.try_reserve(group.len())
            .map_err(|_| DataReadError::LimitExceeded("forward occurrences"))?;
        for candidate in &group {
            if !candidate.reverse && !used.insert((candidate.member, candidate.row)) {
                return Err(DataReadError::InvalidCsr("duplicate traversal").into());
            }
        }
        self.resolve_forward_group(&mut group, &mut used, limits, work, original)
            .await?;
        for candidate in group
            .iter_mut()
            .filter(|candidate| !candidate.reverse && candidate.incoming)
        {
            candidate.source = source_for_row(
                &self.members[candidate.member],
                candidate.row,
                limits,
                self.policy.0,
                work,
                original,
            )
            .await?;
        }
        let mut endpoints = Vec::new();
        endpoints
            .try_reserve(
                group
                    .len()
                    .checked_mul(2)
                    .ok_or(DataReadError::LimitExceeded("position endpoints"))?,
            )
            .map_err(|_| DataReadError::LimitExceeded("position endpoints"))?;
        for candidate in &group {
            let member = &self.members[candidate.member];
            endpoints.extend([
                self.node_id(member.from, candidate.source),
                self.node_id(member.to, candidate.target),
            ]);
        }
        endpoints.sort_unstable();
        endpoints.dedup();
        let mut keys = BTreeMap::new();
        for (index, table) in self.nodes.iter().enumerate() {
            let rows = endpoints
                .iter()
                .filter(|id| id.table as usize == index)
                .map(|id| id.row)
                .collect::<Vec<_>>();
            if rows.is_empty() {
                continue;
            }
            let pk = &self.manifest.node_tables[index].primary_key;
            let mut seen = HashSet::new();
            let mut ordinals = rows.iter();
            let mut stream = table.stream(
                &[pk],
                Some(table.select_rows(&rows)?),
                self.policy.0,
                limits.decode.row_batch_size,
                Arc::clone(work),
                Arc::clone(original),
            )?;
            while let Some(batch) = wait(original, async {
                stream.next().await.transpose().map_err(Into::into)
            })
            .await?
            {
                work.rows(batch.num_rows())?;
                for offset in 0..batch.num_rows() {
                    let key = read_primary_key(&batch, pk, offset, limits)?;
                    if !seen.insert(key.clone()) {
                        return Err(DataReadError::DuplicatePrimaryKey.into());
                    }
                    let row = ordinals
                        .next()
                        .ok_or(DataReadError::InvalidParquet("selected key count"))?;
                    keys.insert(self.node_id(index, *row), key);
                }
            }
            if ordinals.next().is_some() {
                return Err(DataReadError::InvalidParquet("selected key count").into());
            }
        }
        let mut positioned = Vec::new();
        positioned
            .try_reserve(group.len())
            .map_err(|_| DataReadError::LimitExceeded("logical positions"))?;
        for candidate in group {
            let member = &self.members[candidate.member];
            let position = OwnedNeighborhoodPosition {
                uid: limits.text(&candidate.uid)?,
                relationship: limits.text(&member.relationship)?,
                from_label: limits.text(&self.manifest.node_tables[member.from].label)?,
                from_key: keys
                    .get(&self.node_id(member.from, candidate.source))
                    .cloned()
                    .ok_or(DataReadError::InvalidCsr("missing endpoint"))?,
                to_label: limits.text(&self.manifest.node_tables[member.to].label)?,
                to_key: keys
                    .get(&self.node_id(member.to, candidate.target))
                    .cloned()
                    .ok_or(DataReadError::InvalidCsr("missing endpoint"))?,
                occurrence: 0,
            };
            positioned.push(Candidate {
                uid: candidate.uid,
                member: candidate.member,
                row: candidate.row,
                source: candidate.source,
                target: candidate.target,
                position,
            });
        }
        Ok(positioned)
    }
    async fn resolve_forward_group(
        &self,
        group: &mut [RawCandidate],
        used: &mut HashSet<(usize, u64)>,
        limits: GraphNeighborhoodLimits,
        work: &Arc<Work>,
        original: &Arc<SnapshotContext>,
    ) -> Result<()> {
        let mut requests: ReverseRequests<'_> = BTreeMap::new();
        for (index, candidate) in group
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate.reverse)
        {
            let member = &self.reverse_members[candidate.member];
            let pending = requests
                .entry((
                    &member.relationship,
                    member.to,
                    member.from,
                    candidate.source,
                    candidate.target,
                ))
                .or_default()
                .entry(limits.text(&candidate.uid)?)
                .or_default();
            pending
                .try_reserve(1)
                .map_err(|_| DataReadError::LimitExceeded("reverse occurrences"))?;
            pending.push(index);
        }
        if requests.is_empty() {
            return Ok(());
        }
        for (index, member) in self.members.iter().enumerate() {
            let mut sources = requests
                .keys()
                .filter(|(kind, from, to, source, target)| {
                    *kind == member.relationship
                        && *from == member.from
                        && *to == member.to
                        && source
                            .checked_sub(member.source_row_base)
                            .is_some_and(|row| row < member.source_rows)
                        && target
                            .checked_sub(member.target_row_base)
                            .is_some_and(|row| row < member.target_rows)
                })
                .map(|(_, _, _, source, _)| *source)
                .collect::<Vec<_>>();
            sources.sort_unstable();
            sources.dedup();
            for source in sources {
                let (begin, end) =
                    pointer_range(member, source, limits, self.policy.0, work, original).await?;
                let range = begin..end;
                let mut row = begin;
                let mut stream = member.indices.stream(
                    &["target", "uid"],
                    Some(member.indices.select_ranges(std::slice::from_ref(&range))?),
                    self.policy.0,
                    limits.decode.row_batch_size,
                    Arc::clone(work),
                    Arc::clone(original),
                )?;
                while let Some(batch) = wait(original, async {
                    stream.next().await.transpose().map_err(Into::into)
                })
                .await?
                {
                    work.rows(batch.num_rows())?;
                    for offset in 0..batch.num_rows() {
                        let uid = text(&batch, "uid", offset)?
                            .filter(|uid| !uid.is_empty())
                            .ok_or(DataReadError::InvalidCsr("edge UID"))?;
                        bounded(uid.len(), limits.max_string_bytes, "edge UID")?;
                        let local = uint(&batch, "target", offset)?;
                        if local >= member.target_rows {
                            return Err(DataReadError::InvalidCsr("target shard bounds").into());
                        }
                        if !used.contains(&(index, row))
                            && let Some(queues) = requests.get_mut(&(
                                member.relationship.as_str(),
                                member.from,
                                member.to,
                                source,
                                member.target_row_base + local,
                            ))
                            && let Some(pending) = queues.get_mut(uid)
                            && let Some(selected) = pending.pop()
                        {
                            group[selected].member = index;
                            group[selected].row = row;
                            used.insert((index, row));
                        }
                        row += 1;
                    }
                }
                if row != end {
                    return Err(DataReadError::InvalidCsr("edge row count").into());
                }
            }
        }
        if requests
            .values()
            .any(|queues| queues.values().any(|pending| !pending.is_empty()))
        {
            return Err(DataReadError::InvalidCsr("missing forward edge").into());
        }
        original.check()?;
        Ok(())
    }
    async fn find_anchor(
        &self,
        table: usize,
        key: GraphAnchorKey<'_>,
        max_string_bytes: usize,
        row_batch_size: usize,
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
        let lookup = match key {
            GraphAnchorKey::String(key) => LookupKey::String(key),
            GraphAnchorKey::Int64(key) => LookupKey::Int64(key),
        };
        let ranges = data.key_ranges(&descriptor.primary_key, &[lookup])?;
        let mut ordinals = ranges.iter().flat_map(|range| range.clone());
        let mut stream = data.stream(
            &[&descriptor.primary_key],
            Some(data.select_ranges(&ranges)?),
            self.policy.0,
            row_batch_size,
            Arc::clone(work),
            Arc::clone(original),
        )?;
        let mut found = None;
        while let Some(batch) = wait(original, async {
            stream.next().await.transpose().map_err(Into::into)
        })
        .await?
        {
            work.rows(batch.num_rows())?;
            for offset in 0..batch.num_rows() {
                let row = ordinals
                    .next()
                    .ok_or(DataReadError::InvalidParquet("lookup row count"))?;
                let matched = match key {
                    GraphAnchorKey::String(wanted) => {
                        let value = text(&batch, &descriptor.primary_key, offset)?
                            .ok_or(DataReadError::InvalidParquet("nonnull primary key"))?;
                        bounded(value.len(), max_string_bytes, "primary key")?;
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
            }
            original.check()?;
        }
        if ordinals.next().is_some() {
            return Err(DataReadError::InvalidParquet("lookup row count").into());
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
                            member: index as u32,
                            row: candidate.row,
                        },
                        from: self.node_id(member.from, candidate.source),
                        to: self.node_id(member.to, candidate.target),
                        relationship: limits.text(&member.relationship)?,
                        properties: properties(&batch, &names, offset, limits)?,
                        uid_index,
                        position: candidate.position.clone(),
                    });
                    next += 1;
                }
                original.check()?;
            }
            if next != rows.len() {
                return Err(DataReadError::InvalidCsr("selected edge count").into());
            }
        }
        edges.sort_unstable_by(|a, b| a.position().cmp(&b.position()));
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
                        primary_key_index: names
                            .binary_search(&descriptor.primary_key.as_str())
                            .map_err(|_| DataReadError::InvalidParquet("primary key"))?,
                    });
                    next += 1;
                }
                original.check()?;
            }
            if next != rows.len() {
                return Err(DataReadError::InvalidParquet("selected node count").into());
            }
            let lookups = keys
                .iter()
                .map(|key| match key {
                    PrimaryKey::String(key) => LookupKey::String(key),
                    PrimaryKey::Int64(key) => LookupKey::Int64(*key),
                })
                .collect::<Vec<_>>();
            let ranges = table.key_ranges(&descriptor.primary_key, &lookups)?;
            let mut stream = table.stream(
                &[&descriptor.primary_key],
                Some(table.select_ranges(&ranges)?),
                self.policy.0,
                limits.decode.row_batch_size,
                Arc::clone(work),
                Arc::clone(original),
            )?;
            let mut seen = HashSet::new();
            while let Some(batch) = wait(original, async {
                stream.next().await.transpose().map_err(Into::into)
            })
            .await?
            {
                work.rows(batch.num_rows())?;
                for offset in 0..batch.num_rows() {
                    let value = column(&batch, &descriptor.primary_key)?;
                    let key = if let Some(value) = value.as_any().downcast_ref::<StringArray>() {
                        if value.is_null(offset) {
                            return Err(DataReadError::InvalidParquet("nonnull primary key").into());
                        }
                        PrimaryKey::String(limits.text(value.value(offset))?)
                    } else if let Some(value) = value.as_any().downcast_ref::<Int64Array>() {
                        if value.is_null(offset) {
                            return Err(DataReadError::InvalidParquet("nonnull primary key").into());
                        }
                        PrimaryKey::Int64(value.value(offset))
                    } else {
                        return Err(DataReadError::InvalidParquet("primary key type").into());
                    };
                    if keys.contains(&key) && !seen.insert(key) {
                        return Err(DataReadError::DuplicatePrimaryKey.into());
                    }
                }
            }
            if seen.len() != keys.len() {
                return Err(DataReadError::InvalidParquet("selected key count").into());
            }
        }
        nodes.sort_unstable_by_key(|node| node.id);
        Ok(nodes)
    }
}

fn member_view(index: usize, member: &Member) -> CsrMemberView<'_> {
    CsrMemberView {
        index,
        kind: &member.relationship,
        from: member.from,
        to: member.to,
        source_base: member.source_row_base,
        source_rows: member.source_rows,
        target_base: member.target_row_base,
        target_rows: member.target_rows,
        rows: member.indices.rows,
    }
}
fn read_primary_key(
    batch: &RecordBatch,
    name: &str,
    row: usize,
    limits: GraphNeighborhoodLimits,
) -> Result<PrimaryKey> {
    let value = column(batch, name)?;
    if value.is_null(row) {
        return Err(DataReadError::InvalidParquet("nonnull primary key").into());
    }
    if let Some(value) = value.as_any().downcast_ref::<StringArray>() {
        Ok(PrimaryKey::String(limits.text(value.value(row))?))
    } else if let Some(value) = value.as_any().downcast_ref::<Int64Array>() {
        Ok(PrimaryKey::Int64(value.value(row)))
    } else {
        Err(DataReadError::InvalidParquet("primary key type").into())
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

async fn source_for_row(
    member: &Member,
    row: u64,
    limits: GraphNeighborhoodLimits,
    policy: LocalGraphLimits,
    work: &Arc<Work>,
    original: &Arc<SnapshotContext>,
) -> Result<u64> {
    if row >= member.indices.rows {
        return Err(DataReadError::InvalidCsr("edge alignment").into());
    }
    let mut low = 0;
    let mut high = member.source_rows;
    while low < high {
        let middle = low + (high - low) / 2;
        let mut stream = member.indptr.stream(
            &["ptr"],
            Some(member.indptr.select_rows(&[middle + 1])?),
            policy,
            limits.decode.row_batch_size,
            Arc::clone(work),
            Arc::clone(original),
        )?;
        let mut end = None;
        while let Some(batch) = wait(original, async {
            stream.next().await.transpose().map_err(Into::into)
        })
        .await?
        {
            work.rows(batch.num_rows())?;
            for offset in 0..batch.num_rows() {
                if end.replace(uint(&batch, "ptr", offset)?).is_some() {
                    return Err(DataReadError::InvalidCsr("pointer count").into());
                }
            }
        }
        let end = end.ok_or(DataReadError::InvalidCsr("pointer count"))?;
        if end > member.indices.rows {
            return Err(DataReadError::InvalidCsr("row pointer").into());
        }
        if row < end {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    let source = member
        .source_row_base
        .checked_add(low)
        .ok_or(DataReadError::InvalidCsr("source bounds"))?;
    let (begin, end) = pointer_range(member, source, limits, policy, work, original).await?;
    if row < begin || row >= end {
        return Err(DataReadError::InvalidCsr("edge alignment").into());
    }
    original.check()?;
    Ok(source)
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
    query_properties(batch, names, row, limits.max_string_bytes)
}
fn query_properties(
    batch: &RecordBatch,
    names: &[&str],
    row: usize,
    max_string_bytes: usize,
) -> Result<Vec<GraphProperty>> {
    names
        .iter()
        .map(|name| {
            let value = column(batch, name)?;
            let value = if value.is_null(row) {
                PropertyValue::Null
            } else if let Some(value) = value.as_any().downcast_ref::<StringArray>() {
                PropertyValue::String(property_text(value.value(row), max_string_bytes)?)
            } else if let Some(value) = value.as_any().downcast_ref::<Int64Array>() {
                PropertyValue::Int64(value.value(row))
            } else if let Some(value) = value.as_any().downcast_ref::<BooleanArray>() {
                PropertyValue::Bool(value.value(row))
            } else {
                return Err(DataReadError::InvalidParquet("property scalar").into());
            };
            Ok(GraphProperty {
                name: property_text(name, max_string_bytes)?,
                value,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
