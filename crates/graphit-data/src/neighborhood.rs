//! Bounded forward-member traversal over one original captured graph.
use super::*;
use crate::policy::DecodeLimits;
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::cell::Cell;
use std::{cmp::Ordering, collections::BinaryHeap};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphDirection {
    Outgoing,
    Incoming,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphAnchorKey<'a> {
    String(&'a str),
    Int64(i64),
}
#[derive(Clone, Copy)]
pub struct GraphNeighborhoodCursor<'a> {
    generation: GraphGeneration,
    fingerprint: [u8; 32],
    last_uid: &'a str,
}
impl<'a> GraphNeighborhoodCursor<'a> {
    pub fn from_parts(
        generation: GraphGeneration,
        fingerprint: [u8; 32],
        last_uid: &'a str,
    ) -> Self {
        Self {
            generation,
            fingerprint,
            last_uid,
        }
    }
    pub fn generation(self) -> GraphGeneration {
        self.generation
    }
    pub fn query_fingerprint(self) -> [u8; 32] {
        self.fingerprint
    }
    pub fn last_uid(self) -> &'a str {
        self.last_uid
    }
}
pub struct GraphNeighborhoodRequest<'a> {
    pub anchor_label: &'a str,
    pub anchor_key: GraphAnchorKey<'a>,
    pub relationship: &'a str,
    pub other_label: &'a str,
    pub direction: GraphDirection,
    pub limit: usize,
    pub cursor: Option<GraphNeighborhoodCursor<'a>>,
}
#[derive(Clone, Copy, Debug)]
pub struct GraphNeighborhoodLimits {
    pub(crate) max_string_bytes: usize,
    pub(crate) max_properties_per_record: usize,
    pub(crate) max_matching_edges: usize,
    pub(crate) decode: DecodeLimits,
}
impl GraphNeighborhoodLimits {
    pub fn new(
        max_string_bytes: usize,
        max_properties_per_record: usize,
        max_matching_edges: usize,
        max_decoded_rows: usize,
        max_page_work: usize,
        row_batch_size: usize,
    ) -> Result<Self> {
        if [
            max_string_bytes,
            max_properties_per_record,
            max_matching_edges,
            max_decoded_rows,
            max_page_work,
            row_batch_size,
        ]
        .contains(&0)
        {
            return Err(DataReadError::InvalidPolicy("neighborhood limits"));
        }
        if row_batch_size > max_decoded_rows {
            return Err(DataReadError::InvalidPolicy("row_batch_size"));
        }
        Ok(Self {
            max_string_bytes,
            max_properties_per_record,
            max_matching_edges,
            decode: DecodeLimits {
                max_decoded_rows,
                max_page_work,
                row_batch_size,
            },
        })
    }
}
pub(crate) enum PropertyValue {
    String(String),
    Int64(i64),
    Bool(bool),
    Null,
}
#[derive(Debug, PartialEq, Eq)]
pub enum GraphPropertyValueRef<'a> {
    String(&'a str),
    Int64(i64),
    Bool(bool),
    Null,
}
/// Owned property data cannot be extracted from its retained page.
/// ```compile_fail
/// fn detach(p: graphit_data::GraphProperty) { let _ = p.value; }
/// ```
/// ```compile_fail
/// fn copy(p: &graphit_data::GraphProperty) { let _: graphit_data::GraphProperty = p.clone(); }
/// ```
pub struct GraphProperty {
    pub(crate) name: String,
    pub(crate) value: PropertyValue,
}
impl GraphProperty {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn value(&self) -> GraphPropertyValueRef<'_> {
        match &self.value {
            PropertyValue::String(v) => GraphPropertyValueRef::String(v),
            PropertyValue::Int64(v) => GraphPropertyValueRef::Int64(*v),
            PropertyValue::Bool(v) => GraphPropertyValueRef::Bool(*v),
            PropertyValue::Null => GraphPropertyValueRef::Null,
        }
    }
}
pub struct GraphPropertyNode {
    pub(crate) id: NodeId,
    pub(crate) label: String,
    pub(crate) properties: Vec<GraphProperty>,
}
impl GraphPropertyNode {
    pub fn id(&self) -> NodeId {
        self.id
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn properties(&self) -> &[GraphProperty] {
        &self.properties
    }
}
pub struct GraphPropertyEdge {
    pub(crate) id: EdgeId,
    pub(crate) from: NodeId,
    pub(crate) to: NodeId,
    pub(crate) relationship: String,
    pub(crate) properties: Vec<GraphProperty>,
    pub(crate) uid_index: usize,
}
impl GraphPropertyEdge {
    pub fn id(&self) -> EdgeId {
        self.id
    }
    pub fn from(&self) -> NodeId {
        self.from
    }
    pub fn to(&self) -> NodeId {
        self.to
    }
    pub fn relationship(&self) -> &str {
        &self.relationship
    }
    pub fn uid(&self) -> &str {
        match &self.properties[self.uid_index].value {
            PropertyValue::String(v) => v,
            _ => unreachable!("validated UID"),
        }
    }
    pub fn properties(&self) -> &[GraphProperty] {
        &self.properties
    }
}
/// The page owns its vectors and strings and exposes borrowed views.
/// ```compile_fail
/// fn copy(p: &graphit_data::GraphNeighborhoodPage) { let _: graphit_data::GraphNeighborhoodPage = p.clone(); }
/// ```
/// ```compile_fail
/// fn detach(p: graphit_data::GraphNeighborhoodPage) { let _ = p.nodes; }
/// ```
/// ```compile_fail
/// fn escaped(p: graphit_data::GraphNeighborhoodPage) -> graphit_data::GraphNeighborhoodCursor<'static> { p.next_cursor().unwrap() }
/// ```
/// ```compile_fail
/// fn restore(c: graphit_data::GraphNeighborhoodCursor<'_>) { let _ = graphit_data::CapturedLocalGraph::from_cursor(c); }
/// ```
pub struct GraphNeighborhoodPage {
    pub(crate) generation: GraphGeneration,
    pub(crate) fingerprint: [u8; 32],
    pub(crate) anchor: NodeId,
    pub(crate) nodes: Vec<GraphPropertyNode>,
    pub(crate) edges: Vec<GraphPropertyEdge>,
    pub(crate) has_more: bool,
}
impl GraphNeighborhoodPage {
    pub fn generation(&self) -> GraphGeneration {
        self.generation
    }
    pub fn anchor(&self) -> NodeId {
        self.anchor
    }
    pub fn nodes(&self) -> &[GraphPropertyNode] {
        &self.nodes
    }
    pub fn edges(&self) -> &[GraphPropertyEdge] {
        &self.edges
    }
    pub fn has_more(&self) -> bool {
        self.has_more
    }
    pub fn node(&self, id: NodeId) -> Option<&GraphPropertyNode> {
        self.nodes
            .binary_search_by_key(&id, |n| n.id)
            .ok()
            .map(|i| &self.nodes[i])
    }
    pub fn next_cursor(&self) -> Option<GraphNeighborhoodCursor<'_>> {
        if !self.has_more {
            return None;
        }
        self.edges.last().map(|e| {
            GraphNeighborhoodCursor::from_parts(self.generation, self.fingerprint, e.uid())
        })
    }
}

struct PageRows {
    anchor: NodeId,
    nodes: Vec<GraphPropertyNode>,
    edges: Vec<GraphPropertyEdge>,
    has_more: bool,
}
impl GraphNeighborhoodLimits {
    pub(crate) fn text(&self, text: &str) -> Result<String> {
        bounded(text.len(), self.max_string_bytes, "output string")?;
        copy_text(text)
    }
    fn properties(&self, row: &Row, names: &[&str]) -> Result<Vec<GraphProperty>> {
        bounded(
            names.len(),
            self.max_properties_per_record,
            "record properties",
        )?;
        let mut out = Vec::new();
        reserve(&mut out, names.len(), "properties allocation")?;
        for name in names {
            let value = match field(row, name)? {
                Field::Str(v) => PropertyValue::String(self.text(v)?),
                Field::Long(v) => PropertyValue::Int64(*v),
                Field::Bool(v) => PropertyValue::Bool(*v),
                Field::Null => PropertyValue::Null,
                _ => return Err(DataReadError::InvalidParquet("property scalar")),
            };
            out.push(GraphProperty {
                name: self.text(name)?,
                value,
            });
        }
        Ok(out)
    }
}
pub(crate) struct Candidate {
    pub(crate) uid: String,
    pub(crate) member: usize,
    pub(crate) row: u64,
    pub(crate) target: u64,
    pub(crate) source: u64,
}
impl PartialEq for Candidate {
    fn eq(&self, b: &Self) -> bool {
        self.uid == b.uid
    }
}
impl Eq for Candidate {}
impl PartialOrd for Candidate {
    fn partial_cmp(&self, b: &Self) -> Option<Ordering> {
        Some(self.cmp(b))
    }
}
impl Ord for Candidate {
    fn cmp(&self, b: &Self) -> Ordering {
        self.uid.cmp(&b.uid)
    }
}
fn reserve<T>(out: &mut Vec<T>, count: usize, name: &'static str) -> Result<()> {
    out.try_reserve_exact(count)
        .map_err(|_| DataReadError::LimitExceeded(name))
}
pub(crate) fn selected_rows(candidates: &[Candidate], member: usize) -> Result<Vec<(u64, usize)>> {
    let count = candidates.iter().filter(|c| c.member == member).count();
    let mut rows = Vec::new();
    reserve(&mut rows, count, "selected ordinals")?;
    rows.extend(
        candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| c.member == member)
            .map(|(index, c)| (c.row, index)),
    );
    rows.sort_unstable();
    Ok(rows)
}
fn frame(hash: &mut Sha256, tag: u8, bytes: &[u8]) -> Result<()> {
    let length =
        u64::try_from(bytes.len()).map_err(|_| DataReadError::InvalidNeighborhoodRequest)?;
    hash.update([tag]);
    hash.update(length.to_be_bytes());
    hash.update(bytes);
    Ok(())
}
pub fn fingerprint(request: &GraphNeighborhoodRequest<'_>) -> Result<[u8; 32]> {
    let mut hash = Sha256::new();
    hash.update(b"graphit/icebug-neighborhood/v1\0");
    frame(&mut hash, 1, request.anchor_label.as_bytes())?;
    match request.anchor_key {
        GraphAnchorKey::String(v) => frame(&mut hash, 2, v.as_bytes())?,
        GraphAnchorKey::Int64(v) => frame(&mut hash, 3, &v.to_be_bytes())?,
    }
    frame(&mut hash, 4, request.relationship.as_bytes())?;
    frame(&mut hash, 5, request.other_label.as_bytes())?;
    hash.update([match request.direction {
        GraphDirection::Outgoing => 6,
        GraphDirection::Incoming => 7,
    }]);
    Ok(hash.finalize().into())
}
struct Keys {
    values: HashSet<PrimaryKey>,
    strings: usize,
}
impl Keys {
    fn insert(&mut self, value: &Field) -> Result<()> {
        let key = match value {
            Field::Str(v) => {
                bounded(v.len(), self.strings, "primary key")?;
                PrimaryKey::String(copy_text(v)?)
            }
            Field::Long(v) => PrimaryKey::Int64(*v),
            _ => return Err(DataReadError::InvalidParquet("nonnull primary key")),
        };
        self.values
            .try_reserve(1)
            .map_err(|_| DataReadError::LimitExceeded("primary keys"))?;
        if !self.values.insert(key) {
            return Err(DataReadError::DuplicatePrimaryKey);
        }
        Ok(())
    }
}
fn validate_properties(row: &Row, table: &Table, limit: usize) -> Result<()> {
    for (name, scalar) in &table.fields {
        match (scalar, field(row, name)?) {
            (_, Field::Null) => (),
            (Scalar::String, Field::Str(v)) => bounded(v.len(), limit, "property string")?,
            (Scalar::Int64, Field::Long(_))
            | (Scalar::Bool, Field::Bool(_))
            | (Scalar::UInt64, Field::ULong(_)) => (),
            _ => return Err(DataReadError::InvalidParquet("property scalar")),
        }
    }
    Ok(())
}
fn pointer_ranges(
    member: &MemberTable,
    request: &DecodeRequest<'_>,
    mut consume: impl FnMut(u64, u64, u64) -> Result<()>,
) -> Result<()> {
    let mut previous = 0;
    let mut count = 0;
    note_scan();
    member.indptr.scan(request, &["ptr"], |row, record| {
        let pointer = uint(field(record, "ptr")?)?;
        if row == 0 && pointer != 0 || pointer < previous || pointer > member.rows {
            return Err(DataReadError::InvalidCsr("row pointer"));
        }
        if row > 0 {
            consume(member.source_row_base + row - 1, previous, pointer)?;
        }
        previous = pointer;
        count += 1;
        Ok(())
    })?;
    if previous != member.rows || count != member.indptr.rows {
        return Err(DataReadError::InvalidCsr("final pointer"));
    }
    Ok(())
}
impl CapturedLocalGraph {
    pub fn neighborhood_graph(
        &self,
        request: GraphNeighborhoodRequest<'_>,
        limits: GraphNeighborhoodLimits,
    ) -> Result<GraphNeighborhoodPage> {
        reset_metrics();
        self.context.check()?;
        if !(1..=100).contains(&request.limit) {
            return Err(DataReadError::InvalidNeighborhoodRequest);
        }
        for name in [
            request.anchor_label,
            request.relationship,
            request.other_label,
        ] {
            if name.is_empty() || name.len() > self.policy.0.max_name_bytes {
                return Err(DataReadError::InvalidNeighborhoodRequest);
            }
        }
        if let GraphAnchorKey::String(key) = request.anchor_key {
            bounded(key.len(), limits.max_string_bytes, "anchor key")?;
        }
        let top = add(request.limit, 1, "candidate count")?;
        let find = |label| {
            self.manifest
                .node_tables
                .iter()
                .position(|t| t.label == label)
                .ok_or(DataReadError::UnknownNodeLabel)
        };
        let anchor_table = find(request.anchor_label)?;
        let other = find(request.other_label)?;
        if !self
            .manifest
            .relationships
            .iter()
            .any(|r| r.kind == request.relationship)
        {
            return Err(DataReadError::UnknownRelationship);
        }
        let matches = |member: &MemberTable| {
            member.kind == request.relationship
                && match request.direction {
                    GraphDirection::Outgoing => member.from == anchor_table && member.to == other,
                    GraphDirection::Incoming => member.to == anchor_table && member.from == other,
                }
        };
        if !self.members.iter().any(matches) {
            return Err(DataReadError::UnavailableRelationshipEndpoint);
        }
        let pk = &self.manifest.node_tables[anchor_table].primary_key;
        let kind = self.nodes[anchor_table]
            .fields
            .iter()
            .find(|(name, _)| name == pk)
            .map(|(_, kind)| kind);
        if !(self.nodes[anchor_table].fields.is_empty() && self.nodes[anchor_table].rows == 0)
            && !matches!(
                (kind, request.anchor_key),
                (Some(Scalar::String), GraphAnchorKey::String(_))
                    | (Some(Scalar::Int64), GraphAnchorKey::Int64(_))
            )
        {
            return Err(DataReadError::InvalidAnchorKey);
        }
        let fingerprint = fingerprint(&request)?;
        if let Some(cursor) = request.cursor
            && (cursor.generation != self.generation
                || cursor.fingerprint != fingerprint
                || cursor.last_uid.is_empty()
                || cursor.last_uid.len() > limits.max_string_bytes)
        {
            return Err(DataReadError::InvalidCursor);
        }

        self.reader.revalidate_bindings()?;
        let work = Work::new(limits.decode);
        let decode = DecodeRequest {
            source: &self.reader,
            context: &self.context,

            limits: self.policy.0,
            decode: limits.decode,
            work: &work,
        };
        let result = (|| {
            let mut anchor = None;
            if self.nodes[anchor_table].rows > 0 {
                self.scan_primary_keys(
                    anchor_table,
                    &decode,
                    limits.max_string_bytes,
                    &[pk],
                    |row, record| {
                        let found = match (request.anchor_key, field(record, pk)?) {
                            (GraphAnchorKey::String(a), Field::Str(b)) => a == b,
                            (GraphAnchorKey::Int64(a), Field::Long(b)) => a == *b,
                            _ => false,
                        };
                        if found {
                            anchor = Some(row);
                        }
                        Ok(())
                    },
                )?;
            }
            step("anchor");
            let anchor = anchor.ok_or(DataReadError::AnchorNotFound)?;
            let mut heap = BinaryHeap::new();
            heap.try_reserve_exact(top)
                .map_err(|_| DataReadError::LimitExceeded("candidate allocation"))?;
            let mut uids = HashSet::new();
            for (index, member) in self.members.iter().enumerate().filter(|(_, m)| matches(m)) {
                let mut range = (0, 0);
                pointer_ranges(member, &decode, |source, begin, end| {
                    if source == anchor {
                        range = (begin, end);
                    }
                    Ok(())
                })?;
                note_scan();
                member
                    .indices
                    .scan(&decode, &["target", "uid"], |row, record| {
                        let local_target = uint(field(record, "target")?)?;
                        if local_target >= member.target_rows {
                            return Err(DataReadError::InvalidCsr("target shard bounds"));
                        }
                        let target = member.target_row_base + local_target;
                        if !self.nodes[member.to].contains(target) {
                            return Err(DataReadError::InvalidCsr("target bounds"));
                        }
                        let uid = string(field(record, "uid")?)?
                            .ok_or(DataReadError::InvalidCsr("NULL edge UID"))?;
                        if uid.is_empty() {
                            return Err(DataReadError::InvalidCsr("empty edge UID"));
                        }
                        bounded(uid.len(), limits.max_string_bytes, "edge UID")?;
                        let matched = match request.direction {
                            GraphDirection::Outgoing => row >= range.0 && row < range.1,
                            GraphDirection::Incoming => target == anchor,
                        };
                        if !matched {
                            return Ok(());
                        }
                        if uids.contains(uid) {
                            return Err(DataReadError::DuplicateEdgeUid);
                        }
                        bounded(
                            add(uids.len(), 1, "matching edges")?,
                            limits.max_matching_edges,
                            "matching edges",
                        )?;
                        uids.try_reserve(1)
                            .map_err(|_| DataReadError::LimitExceeded("UID set"))?;
                        uids.insert(copy_text(uid)?);
                        if request.cursor.is_some_and(|c| uid <= c.last_uid) {
                            return Ok(());
                        }
                        if heap.len() == top
                            && heap
                                .peek()
                                .is_some_and(|c: &Candidate| uid >= c.uid.as_str())
                        {
                            return Ok(());
                        }
                        if heap.len() == top {
                            heap.pop();
                        }
                        heap.push(Candidate {
                            uid: copy_text(uid)?,
                            member: index,
                            row,
                            target,
                            source: anchor,
                        });
                        note_heap(heap.len());
                        step("candidate");
                        Ok(())
                    })?;
            }
            step("selection");
            drop(uids);
            let mut candidates = heap.into_sorted_vec();
            let has_more = candidates.len() > request.limit;
            if has_more {
                candidates.pop();
            }
            if request.direction == GraphDirection::Incoming {
                for (index, member) in self.members.iter().enumerate() {
                    if !candidates.iter().any(|c| c.member == index) {
                        continue;
                    }
                    let rows = selected_rows(&candidates, index)?;
                    let mut next = 0;
                    pointer_ranges(member, &decode, |source, begin, end| {
                        while next < rows.len() && rows[next].0 < end {
                            if rows[next].0 < begin {
                                return Err(DataReadError::InvalidCsr("edge alignment"));
                            }
                            candidates[rows[next].1].source = source;
                            next += 1;
                        }
                        Ok(())
                    })?;
                    if next != rows.len() {
                        return Err(DataReadError::InvalidCsr("edge alignment"));
                    }
                }
            }
            let mut endpoints = Vec::new();

            reserve(&mut endpoints, top, "endpoint allocation")?;
            let anchor_id = self.node_id(anchor_table, anchor);
            endpoints.push(anchor_id);
            for c in &candidates {
                let member = &self.members[c.member];
                for id in [
                    self.node_id(member.from, c.source),
                    self.node_id(member.to, c.target),
                ] {
                    if !endpoints.contains(&id) {
                        endpoints.push(id);
                    }
                }
            }
            endpoints.sort_unstable();
            let mut edges = Vec::new();
            reserve(&mut edges, candidates.len(), "edge allocation")?;
            for (index, member) in self.members.iter().enumerate() {
                if !candidates.iter().any(|c| c.member == index) {
                    continue;
                }
                let rows = selected_rows(&candidates, index)?;

                let property_count = member
                    .indices
                    .fields
                    .iter()
                    .filter(|(n, _)| n != "target")
                    .count();
                bounded(
                    property_count,
                    limits.max_properties_per_record,
                    "record properties",
                )?;
                let mut names = Vec::new();
                reserve(&mut names, property_count, "projection allocation")?;
                names.extend(
                    member
                        .indices
                        .fields
                        .iter()
                        .filter(|(n, _)| n != "target")
                        .map(|(n, _)| n.as_str()),
                );
                names.sort_unstable();

                // Include target in decoder projection, excluding it only from ordinary output properties.

                bounded(
                    member.indices.fields.len(),
                    self.policy.0.max_columns,
                    "columns",
                )?;
                let mut all = Vec::new();
                reserve(
                    &mut all,
                    member.indices.fields.len(),
                    "projection allocation",
                )?;
                all.extend(member.indices.fields.iter().map(|(n, _)| n.as_str()));
                note_scan();
                member.indices.scan(&decode, &all, |row, record| {
                    validate_properties(record, &member.indices, limits.max_string_bytes)?;
                    let local_target = uint(field(record, "target")?)?;
                    if local_target >= member.target_rows {
                        return Err(DataReadError::InvalidCsr("target shard bounds"));
                    }
                    let target = member.target_row_base + local_target;
                    let uid = string(field(record, "uid")?)?
                        .ok_or(DataReadError::InvalidCsr("NULL edge UID"))?;
                    if !self.nodes[member.to].contains(target) || uid.is_empty() {
                        return Err(DataReadError::InvalidCsr("edge properties"));
                    }
                    if let Ok(selected) = rows.binary_search_by_key(&row, |r| r.0) {
                        let c = &candidates[rows[selected].1];
                        if target != c.target || uid != c.uid {
                            return Err(DataReadError::InvalidCsr("edge differential"));
                        }
                        let properties = limits.properties(record, &names)?;
                        let uid_index = names
                            .binary_search(&"uid")
                            .map_err(|_| DataReadError::InvalidCsr("UID property"))?;
                        edges.push(GraphPropertyEdge {
                            id: EdgeId {
                                generation: self.generation,
                                member: u32::try_from(index)
                                    .map_err(|_| DataReadError::LimitExceeded("member identity"))?,
                                row,
                            },
                            from: self.node_id(member.from, c.source),
                            to: self.node_id(member.to, c.target),
                            relationship: limits.text(&member.kind)?,
                            properties,
                            uid_index,
                        });
                    }
                    Ok(())
                })?;
            }
            edges.sort_unstable_by(|a, b| a.uid().cmp(b.uid()));
            let mut nodes = Vec::new();
            reserve(&mut nodes, endpoints.len(), "nodes allocation")?;
            for (index, table) in self.nodes.iter().enumerate() {
                if !endpoints.iter().any(|id| id.table as usize == index) {
                    continue;
                }

                bounded(
                    table.fields.len(),
                    limits.max_properties_per_record,
                    "record properties",
                )?;
                bounded(table.fields.len(), self.policy.0.max_columns, "columns")?;
                let mut names = Vec::new();
                reserve(&mut names, table.fields.len(), "projection allocation")?;
                names.extend(table.fields.iter().map(|(n, _)| n.as_str()));
                names.sort_unstable();

                self.scan_primary_keys(
                    index,
                    &decode,
                    limits.max_string_bytes,
                    &names,
                    |row, record| {
                        validate_properties(record, table, limits.max_string_bytes)?;
                        let id = self.node_id(index, row);
                        if endpoints.binary_search(&id).is_ok() {
                            nodes.push(GraphPropertyNode {
                                id,
                                label: limits.text(&self.manifest.node_tables[index].label)?,
                                properties: limits.properties(record, &names)?,
                            });
                        }
                        Ok(())
                    },
                )?;
            }
            step("properties");
            nodes.sort_unstable_by_key(|n| n.id);
            if nodes.len() != endpoints.len() || edges.len() != candidates.len() {
                return Err(DataReadError::InvalidCsr("missing endpoint"));
            }
            Ok(PageRows {
                anchor: anchor_id,
                nodes,
                edges,
                has_more,
            })
        })();
        // The original physical/context guard takes precedence over semantic or decoder errors.
        finish_metrics(&work);
        step("before_final");
        self.reader.revalidate_bindings()?;
        self.context.check()?;
        let rows = result?;
        Ok(GraphNeighborhoodPage {
            generation: self.generation,
            fingerprint,
            anchor: rows.anchor,
            nodes: rows.nodes,
            edges: rows.edges,
            has_more: rows.has_more,
        })
    }
    fn scan_primary_keys(
        &self,
        index: usize,
        request: &DecodeRequest<'_>,
        strings: usize,
        names: &[&str],
        mut consume: impl FnMut(u64, &Row) -> Result<()>,
    ) -> Result<()> {
        let mut keys = Keys {
            values: HashSet::new(),
            strings,
        };
        let pk = &self.manifest.node_tables[index].primary_key;
        note_scan();
        self.nodes[index].scan(request, names, |row, record| {
            keys.insert(field(record, pk)?)?;
            consume(row, record)
        })
    }
}
#[cfg(test)]
type StepHook = Option<Box<dyn FnMut(&str)>>;
#[cfg(test)]
thread_local! { static STEP: std::cell::RefCell<StepHook> = const { std::cell::RefCell::new(None) }; }
fn step(name: &str) {
    #[cfg(test)]
    STEP.with(|hook| {
        if let Some(hook) = hook.borrow_mut().as_mut() {
            hook(name);
        }
    });
    #[cfg(not(test))]
    let _ = name;
}
#[cfg(test)]
mod tests;

#[cfg(test)]
#[derive(Clone, Copy, Default)]
struct Metrics {
    heap: usize,
    scans: usize,
    rows: usize,
    pages: usize,
}
#[cfg(test)]
thread_local! { static METRICS: Cell<Metrics> = const { Cell::new(Metrics { heap: 0, scans: 0, rows: 0, pages: 0 }) }; }
fn reset_metrics() {
    #[cfg(test)]
    METRICS.with(|m| m.set(Metrics::default()));
}
fn note_scan() {
    #[cfg(test)]
    METRICS.with(|m| {
        let mut v = m.get();
        v.scans += 1;
        m.set(v);
    });
}
fn note_heap(count: usize) {
    #[cfg(test)]
    METRICS.with(|m| {
        let mut v = m.get();
        v.heap = v.heap.max(count);
        m.set(v);
    });
    #[cfg(not(test))]
    let _ = count;
}
fn finish_metrics(work: &Work) {
    #[cfg(test)]
    METRICS.with(|m| {
        let mut v = m.get();
        (v.rows, v.pages) = work.counts();
        m.set(v);
    });
    #[cfg(not(test))]
    let _ = work;
}
