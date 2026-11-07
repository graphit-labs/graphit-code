//! Bounded forward-member traversal over one original captured graph.
use super::*;
use crate::policy::DecodeLimits;
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::cell::Cell;
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphDirection {
    Outgoing,
    Incoming,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GraphAnchorKey<'a> {
    String(&'a str),
    Int64(i64),
}
/// Logical occurrence position; it cannot restore a reader or grant access.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GraphNeighborhoodPosition<'a> {
    pub uid: &'a str,
    pub relationship: &'a str,
    pub from_label: &'a str,
    pub from_key: GraphAnchorKey<'a>,
    pub to_label: &'a str,
    pub to_key: GraphAnchorKey<'a>,
    pub occurrence: u64,
}
#[derive(Clone)]
pub(crate) struct OwnedNeighborhoodPosition {
    pub(crate) uid: String,
    pub(crate) relationship: String,
    pub(crate) from_label: String,
    pub(crate) from_key: PrimaryKey,
    pub(crate) to_label: String,
    pub(crate) to_key: PrimaryKey,
    pub(crate) occurrence: u64,
}
impl OwnedNeighborhoodPosition {
    pub(crate) fn borrowed(&self) -> GraphNeighborhoodPosition<'_> {
        GraphNeighborhoodPosition {
            uid: &self.uid,
            relationship: &self.relationship,
            from_label: &self.from_label,
            from_key: self.from_key.borrowed(),
            to_label: &self.to_label,
            to_key: self.to_key.borrowed(),
            occurrence: self.occurrence,
        }
    }
}
#[derive(Clone, Copy)]
pub struct GraphNeighborhoodCursor<'a> {
    fingerprint: [u8; 32],
    position: GraphNeighborhoodPosition<'a>,
}
impl<'a> GraphNeighborhoodCursor<'a> {
    pub fn from_parts(fingerprint: [u8; 32], position: GraphNeighborhoodPosition<'a>) -> Self {
        Self {
            fingerprint,
            position,
        }
    }
    pub fn query_fingerprint(self) -> [u8; 32] {
        self.fingerprint
    }
    pub fn last_uid(self) -> &'a str {
        self.position.uid
    }
    pub fn position(self) -> GraphNeighborhoodPosition<'a> {
        self.position
    }
}
pub struct GraphNeighborhoodRequest<'a> {
    pub anchor_label: &'a str,
    pub anchor_key: GraphAnchorKey<'a>,
    pub relationship: Option<&'a str>,
    pub other_label: Option<&'a str>,
    pub direction: Option<GraphDirection>,
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
pub(crate) fn query_decode_limits(
    input: LocalGraphLimits,
    row_batch_size: usize,
) -> Result<DecodeLimits> {
    if row_batch_size == 0 {
        return Err(DataReadError::InvalidPolicy("row_batch_size"));
    }
    Ok(DecodeLimits {
        max_decoded_rows: input.query_decoded_rows,
        max_page_work: input.max_pages,
        row_batch_size: row_batch_size.min(input.query_decoded_rows),
    })
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
    pub(crate) primary_key_index: usize,
}
impl GraphPropertyNode {
    pub fn primary_key(&self) -> &GraphProperty {
        &self.properties[self.primary_key_index]
    }
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
    pub(crate) position: OwnedNeighborhoodPosition,
}
impl GraphPropertyEdge {
    pub fn position(&self) -> GraphNeighborhoodPosition<'_> {
        self.position.borrowed()
    }
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
    pub(crate) fingerprint: [u8; 32],
    pub(crate) anchor: NodeId,
    pub(crate) nodes: Vec<GraphPropertyNode>,
    pub(crate) edges: Vec<GraphPropertyEdge>,
    pub(crate) has_more: bool,
}
impl GraphNeighborhoodPage {
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
        self.edges
            .last()
            .map(|e| GraphNeighborhoodCursor::from_parts(self.fingerprint, e.position()))
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
        property_text(text, self.max_string_bytes)
    }
    fn properties(&self, row: &Row, names: &[&str]) -> Result<Vec<GraphProperty>> {
        bounded(
            names.len(),
            self.max_properties_per_record,
            "record properties",
        )?;
        row_properties(row, names, self.max_string_bytes)
    }
}
pub(crate) fn property_text(text: &str, max_string_bytes: usize) -> Result<String> {
    bounded(text.len(), max_string_bytes, "output string")?;
    copy_text(text)
}
fn row_properties(
    row: &Row,
    names: &[&str],
    max_string_bytes: usize,
) -> Result<Vec<GraphProperty>> {
    let mut out = Vec::new();
    reserve(&mut out, names.len(), "properties allocation")?;
    for name in names {
        let value = match field(row, name)? {
            Field::Str(v) => PropertyValue::String(property_text(v, max_string_bytes)?),
            Field::Long(v) => PropertyValue::Int64(*v),
            Field::Bool(v) => PropertyValue::Bool(*v),
            Field::Null => PropertyValue::Null,
            _ => return Err(DataReadError::InvalidParquet("property scalar")),
        };
        out.push(GraphProperty {
            name: property_text(name, max_string_bytes)?,
            value,
        });
    }
    Ok(out)
}
pub(crate) fn query_projection<'a>(
    fields: &'a [(String, Scalar)],
    primary_key: &'a str,
    columns: &[&str],
    input: LocalGraphLimits,
) -> Result<Vec<&'a str>> {
    bounded(columns.len(), input.max_columns, "columns")?;
    let mut names = Vec::new();
    reserve(
        &mut names,
        add(columns.len(), 1, "columns")?,
        "projection allocation",
    )?;
    for name in columns.iter().copied().chain(std::iter::once(primary_key)) {
        let (name, _) = fields
            .iter()
            .find(|(field, _)| field == name)
            .ok_or(DataReadError::InvalidParquet("projection"))?;
        names.push(name.as_str());
    }
    names.sort_unstable();
    names.dedup();
    bounded(names.len(), input.max_columns, "columns")?;
    Ok(names)
}
pub(crate) struct RawCandidate {
    pub(crate) uid: String,
    pub(crate) member: usize,
    pub(crate) row: u64,
    pub(crate) target: u64,
    pub(crate) source: u64,
    pub(crate) reverse: bool,
    pub(crate) incoming: bool,
}
pub(crate) type ReverseRequests<'a> =
    BTreeMap<(&'a str, usize, usize, u64, u64), BTreeMap<String, Vec<usize>>>;
pub(crate) struct Candidate {
    pub(crate) uid: String,
    pub(crate) member: usize,
    pub(crate) row: u64,
    pub(crate) target: u64,
    pub(crate) source: u64,
    pub(crate) position: OwnedNeighborhoodPosition,
}
pub(crate) fn candidate_prefix(
    mut raw: Vec<RawCandidate>,
    request: &GraphNeighborhoodRequest<'_>,
) -> Result<Vec<RawCandidate>> {
    raw.retain(|candidate| {
        request
            .cursor
            .is_none_or(|cursor| candidate.uid.as_str() >= cursor.last_uid())
    });
    raw.sort_unstable_by(|a, b| a.uid.cmp(&b.uid));
    let mut end = 0;
    let mut after_cursor_uid = 0usize;
    while end < raw.len() {
        let start = end;
        while end < raw.len() && raw[end].uid == raw[start].uid {
            end += 1;
        }
        if request
            .cursor
            .is_none_or(|cursor| raw[start].uid.as_str() > cursor.last_uid())
        {
            after_cursor_uid = add(after_cursor_uid, end - start, "page candidates")?;
        }
        if after_cursor_uid > request.limit {
            break;
        }
    }
    raw.truncate(end);
    Ok(raw)
}
impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Candidate {}
impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.position.borrowed().cmp(&other.position.borrowed())
    }
}
pub(crate) fn append_group(
    page: &mut Vec<Candidate>,
    mut group: Vec<Candidate>,
    request: &GraphNeighborhoodRequest<'_>,
) -> Result<()> {
    group.sort_unstable_by(|a, b| {
        a.cmp(b)
            .then_with(|| (a.member, a.row).cmp(&(b.member, b.row)))
    });
    for i in 0..group.len() {
        let same = i > 0 && {
            let mut previous = group[i - 1].position.borrowed();
            previous.occurrence = 0;
            previous == group[i].position.borrowed()
        };
        group[i].position.occurrence = if same {
            group[i - 1]
                .position
                .occurrence
                .checked_add(1)
                .ok_or(DataReadError::LimitExceeded("edge occurrence"))?
        } else {
            0
        };
    }
    for candidate in group {
        if request
            .cursor
            .is_some_and(|cursor| candidate.position.borrowed() <= cursor.position())
        {
            continue;
        }
        reserve(page, 1, "page candidates")?;
        page.push(candidate);
        note_heap(page.len());
        if page.len() > request.limit {
            break;
        }
    }
    Ok(())
}
pub(crate) fn validate_cursor(
    request: &GraphNeighborhoodRequest<'_>,
    fingerprint: [u8; 32],
    names: usize,
    strings: usize,
) -> Result<()> {
    if let Some(cursor) = request.cursor {
        let p = cursor.position();
        if cursor.query_fingerprint() != fingerprint
            || p.uid.is_empty()
            || p.uid.len() > strings
            || [p.relationship, p.from_label, p.to_label]
                .iter()
                .any(|name| name.is_empty() || name.len() > names)
            || [p.from_key, p.to_key]
                .iter()
                .any(|key| matches!(key, GraphAnchorKey::String(key) if key.len() > strings))
        {
            return Err(DataReadError::InvalidCursor);
        }
    }
    Ok(())
}
pub(crate) fn primary_key(value: &Field, strings: usize) -> Result<PrimaryKey> {
    match value {
        Field::Str(value) => {
            bounded(value.len(), strings, "primary key")?;
            Ok(PrimaryKey::String(copy_text(value)?))
        }
        Field::Long(value) => Ok(PrimaryKey::Int64(*value)),
        _ => Err(DataReadError::InvalidParquet("nonnull primary key")),
    }
}
fn reserve<T>(out: &mut Vec<T>, count: usize, name: &'static str) -> Result<()> {
    out.try_reserve(count)
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
    match request.relationship {
        Some(kind) => frame(&mut hash, 4, kind.as_bytes())?,
        None => hash.update([8]),
    }
    match request.other_label {
        Some(label) => frame(&mut hash, 5, label.as_bytes())?,
        None => hash.update([9]),
    }
    hash.update([match request.direction {
        Some(GraphDirection::Outgoing) => 6,
        Some(GraphDirection::Incoming) => 7,
        None => 10,
    }]);
    Ok(hash.finalize().into())
}
#[derive(Clone, Copy)]
pub(crate) struct CsrMemberView<'a> {
    pub(crate) index: usize,
    pub(crate) kind: &'a str,
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) source_base: u64,
    pub(crate) source_rows: u64,
    pub(crate) target_base: u64,
    pub(crate) target_rows: u64,
    pub(crate) rows: u64,
}
pub(crate) struct Traversal {
    pub(crate) member: usize,
    pub(crate) incoming: bool,
    pub(crate) reverse: bool,
}
type CsrPair<'a> = (&'a str, usize, usize, u64, u64, u64, u64);
#[derive(Default)]
struct IncomingPair {
    rows: u64,
    reverse_rows: u64,
    forward: Vec<usize>,
    reverse: Vec<usize>,
}
/// Plan only metadata partitions adjacent to the selected label. Reverse storage
/// substitutes incoming scans; it never introduces another logical edge.
pub(crate) fn plan_traversals<'a>(
    request: &GraphNeighborhoodRequest<'_>,
    anchor: usize,
    other: Option<usize>,
    forward: impl IntoIterator<Item = CsrMemberView<'a>>,
    reverse: impl IntoIterator<Item = CsrMemberView<'a>>,
) -> Result<Vec<Traversal>> {
    let mut plan = Vec::new();
    let mut pairs: BTreeMap<CsrPair<'a>, IncomingPair> = BTreeMap::new();
    for member in forward {
        if request.relationship.is_some_and(|kind| kind != member.kind) {
            continue;
        }
        if request.direction != Some(GraphDirection::Incoming)
            && member.from == anchor
            && other.is_none_or(|other| member.to == other)
        {
            reserve(&mut plan, 1, "traversal plan")?;
            plan.push(Traversal {
                member: member.index,
                incoming: false,
                reverse: false,
            });
        }
        if request.direction != Some(GraphDirection::Outgoing)
            && member.to == anchor
            && other.is_none_or(|other| member.from == other)
        {
            let key = (
                member.kind,
                member.from,
                member.to,
                member.source_base,
                member.source_rows,
                member.target_base,
                member.target_rows,
            );
            let pair = pairs.entry(key).or_default();
            pair.rows = pair
                .rows
                .checked_add(member.rows)
                .ok_or(DataReadError::InvalidCsr("reverse coverage"))?;
            reserve(&mut pair.forward, 1, "traversal members")?;
            pair.forward.push(member.index);
        }
    }
    if request.direction != Some(GraphDirection::Outgoing) {
        for member in reverse {
            if request.relationship.is_some_and(|kind| kind != member.kind)
                || member.from != anchor
                || other.is_some_and(|other| member.to != other)
            {
                continue;
            }
            let key = (
                member.kind,
                member.to,
                member.from,
                member.target_base,
                member.target_rows,
                member.source_base,
                member.source_rows,
            );
            let Some(pair) = pairs.get_mut(&key) else {
                if member.rows != 0 {
                    return Err(DataReadError::InvalidCsr("reverse coverage"));
                }
                continue;
            };
            pair.reverse_rows = pair
                .reverse_rows
                .checked_add(member.rows)
                .ok_or(DataReadError::InvalidCsr("reverse coverage"))?;
            reserve(&mut pair.reverse, 1, "traversal members")?;
            pair.reverse.push(member.index);
        }
    }
    for pair in pairs.into_values() {
        let reverse = !pair.reverse.is_empty();
        if reverse && pair.rows != pair.reverse_rows {
            return Err(DataReadError::InvalidCsr("reverse coverage"));
        }
        let members = if reverse { pair.reverse } else { pair.forward };
        reserve(&mut plan, members.len(), "traversal plan")?;
        plan.extend(members.into_iter().map(|member| Traversal {
            member,
            incoming: true,
            reverse,
        }));
    }
    Ok(plan)
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
fn member_view(index: usize, member: &MemberTable) -> CsrMemberView<'_> {
    CsrMemberView {
        index,
        kind: &member.kind,
        from: member.from,
        to: member.to,
        source_base: member.source_row_base,
        source_rows: member.source_rows,
        target_base: member.target_row_base,
        target_rows: member.target_rows,
        rows: member.rows,
    }
}
fn source_for_row(member: &MemberTable, request: &DecodeRequest<'_>, row: u64) -> Result<u64> {
    if row >= member.rows {
        return Err(DataReadError::InvalidCsr("edge alignment"));
    }
    let mut low = 0;
    let mut high = member.source_rows;
    while low < high {
        let middle = low + (high - low) / 2;
        let mut end = None;
        note_scan();
        member
            .indptr
            .scan_selected(request, &["ptr"], &[middle + 1], |_, record| {
                end = Some(uint(field(record, "ptr")?)?);
                Ok(())
            })?;
        let end = end.ok_or(DataReadError::InvalidCsr("pointer count"))?;
        if end > member.rows {
            return Err(DataReadError::InvalidCsr("row pointer"));
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
    if !pointer_range(member, request, source)?.is_some_and(|range| range.contains(&row)) {
        return Err(DataReadError::InvalidCsr("edge alignment"));
    }
    Ok(source)
}
fn pointer_range(
    member: &MemberTable,
    request: &DecodeRequest<'_>,
    source: u64,
) -> Result<Option<std::ops::Range<u64>>> {
    let Some(local) = source
        .checked_sub(member.source_row_base)
        .filter(|row| *row < member.source_rows)
    else {
        return Ok(None);
    };
    let mut pointers = Vec::with_capacity(2);
    note_scan();
    member
        .indptr
        .scan_selected(request, &["ptr"], &[local, local + 1], |_, record| {
            pointers.push(uint(field(record, "ptr")?)?);
            Ok(())
        })?;
    let [begin, end] = pointers.as_slice() else {
        return Err(DataReadError::InvalidCsr("pointer count"));
    };
    if begin > end
        || *end > member.rows
        || local == 0 && *begin != 0
        || local + 1 == member.source_rows && *end != member.rows
    {
        return Err(DataReadError::InvalidCsr("row pointer"));
    }
    Ok(Some(*begin..*end))
}
impl CapturedLocalGraph {
    /// Return the exact input policy retained by this graph capture.
    pub fn input_limits(&self) -> crate::LocalGraphLimits {
        self.policy.limits()
    }

    /// Resolve one exact primary key within this captured graph. The returned
    /// ordinal cannot be reused with a different capture or authorize a read.
    pub fn resolve_node(
        &self,
        label: &str,
        key: GraphAnchorKey<'_>,
        limits: GraphNeighborhoodLimits,
    ) -> Result<ResolvedGraphNode<'_>> {
        self.resolve_node_with_decode(label, key, limits.max_string_bytes, limits.decode)
    }

    /// Resolve a query PK using this capture's input policy and an explicit SDK batch.
    /// An absent key returns None; other failures retain their original cause.
    pub fn resolve_query_node(
        &self,
        label: &str,
        key: GraphAnchorKey<'_>,
        row_batch_size: usize,
    ) -> Result<Option<ResolvedGraphNode<'_>>> {
        self.context.check()?;
        let input = self.input_limits();
        match self.resolve_node_with_decode(
            label,
            key,
            input.max_chunk_bytes,
            query_decode_limits(input, row_batch_size)?,
        ) {
            Ok(node) => Ok(Some(node)),
            Err(DataReadError::AnchorNotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Read selected properties and the declared PK from a node resolved by this capture.
    pub fn read_query_node(
        &self,
        anchor: &ResolvedGraphNode<'_>,
        columns: &[&str],
        row_batch_size: usize,
    ) -> Result<GraphPropertyNode> {
        self.context.check()?;
        if !matches!(anchor.graph, GraphSchemaSource::Local(graph) if std::ptr::eq(graph, self)) {
            return Err(DataReadError::InvalidAnchorKey);
        }
        let index = anchor.id.table as usize;
        let table = self
            .nodes
            .get(index)
            .ok_or(DataReadError::InvalidAnchorKey)?;
        let descriptor = &self.manifest.node_tables[index];
        let input = self.input_limits();
        let decode_limits = query_decode_limits(input, row_batch_size)?;
        let names = query_projection(&table.fields, &descriptor.primary_key, columns, input)?;
        let primary_key_index = names
            .binary_search(&descriptor.primary_key.as_str())
            .map_err(|_| DataReadError::InvalidParquet("primary key"))?;
        self.reader.revalidate_bindings()?;
        let work = Work::new(decode_limits);
        let decode = DecodeRequest {
            source: &self.reader,
            context: &self.context,
            limits: input,
            decode: decode_limits,
            work: &work,
        };
        let result = (|| {
            let mut found = None;
            table.scan_selected(&decode, &names, &[anchor.id.row], |_, record| {
                if !matches!(
                    field(record, &descriptor.primary_key)?,
                    Field::Str(_) | Field::Long(_)
                ) {
                    return Err(DataReadError::InvalidParquet("nonnull primary key"));
                }
                if found.is_some() {
                    return Err(DataReadError::InvalidParquet("selected node count"));
                }
                found = Some(GraphPropertyNode {
                    id: anchor.id,
                    label: property_text(&descriptor.label, input.max_name_bytes)?,
                    properties: row_properties(record, &names, input.max_chunk_bytes)?,
                    primary_key_index,
                });
                Ok(())
            })?;
            found.ok_or(DataReadError::InvalidParquet("selected node count"))
        })();
        self.reader.revalidate_bindings()?;
        self.context.check()?;
        result
    }

    fn resolve_node_with_decode(
        &self,
        label: &str,
        key: GraphAnchorKey<'_>,
        max_string_bytes: usize,
        decode_limits: DecodeLimits,
    ) -> Result<ResolvedGraphNode<'_>> {
        self.context.check()?;
        if label.is_empty() || label.len() > self.policy.0.max_name_bytes {
            return Err(DataReadError::InvalidNeighborhoodRequest);
        }
        if let GraphAnchorKey::String(key) = key {
            bounded(key.len(), max_string_bytes, "anchor key")?;
        }
        let index = self
            .manifest
            .node_tables
            .iter()
            .position(|table| table.label == label)
            .ok_or(DataReadError::UnknownNodeLabel)?;
        self.reader.revalidate_bindings()?;
        let work = Work::new(decode_limits);
        let decode = DecodeRequest {
            source: &self.reader,
            context: &self.context,
            limits: self.policy.0,
            decode: decode_limits,
            work: &work,
        };
        let result = self.resolve_node_inner(index, key, max_string_bytes, &decode);
        self.reader.revalidate_bindings()?;
        self.context.check()?;
        Ok(ResolvedGraphNode {
            graph: GraphSchemaSource::Local(self),
            id: result?,
        })
    }
    fn resolve_node_inner(
        &self,
        index: usize,
        key: GraphAnchorKey<'_>,
        strings: usize,
        decode: &DecodeRequest<'_>,
    ) -> Result<NodeId> {
        let pk = &self.manifest.node_tables[index].primary_key;
        let table = &self.nodes[index];
        if !(table.fields.is_empty() && table.rows == 0)
            && !matches!(
                (
                    table
                        .fields
                        .iter()
                        .find(|(name, _)| name == pk)
                        .map(|(_, kind)| kind),
                    key
                ),
                (Some(Scalar::String), GraphAnchorKey::String(_))
                    | (Some(Scalar::Int64), GraphAnchorKey::Int64(_))
            )
        {
            return Err(DataReadError::InvalidAnchorKey);
        }
        let lookup = match key {
            GraphAnchorKey::String(key) => crate::parquet::LookupKey::String(key),
            GraphAnchorKey::Int64(key) => crate::parquet::LookupKey::Int64(key),
        };
        let mut found = None;
        note_scan();
        table.scan_keys(decode, pk, &[lookup], strings, &[pk], |row, _| {
            if found.replace(row).is_some() {
                return Err(DataReadError::DuplicatePrimaryKey);
            }
            Ok(())
        })?;
        found
            .map(|row| self.node_id(index, row))
            .ok_or(DataReadError::AnchorNotFound)
    }
    fn position_group(
        &self,
        mut group: Vec<RawCandidate>,
        limits: GraphNeighborhoodLimits,
        decode: &DecodeRequest<'_>,
    ) -> Result<Vec<Candidate>> {
        let mut used = HashSet::new();
        used.try_reserve(group.len())
            .map_err(|_| DataReadError::LimitExceeded("forward occurrences"))?;
        for candidate in &group {
            if !candidate.reverse && !used.insert((candidate.member, candidate.row)) {
                return Err(DataReadError::InvalidCsr("duplicate traversal"));
            }
        }
        let mut requests: ReverseRequests<'_> = BTreeMap::new();
        for (index, candidate) in group
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate.reverse)
        {
            let member = &self.reverse_members[candidate.member];
            let pending = requests
                .entry((
                    &member.kind,
                    member.to,
                    member.from,
                    candidate.source,
                    candidate.target,
                ))
                .or_default()
                .entry(copy_text(&candidate.uid)?)
                .or_default();
            reserve(pending, 1, "reverse occurrences")?;
            pending.push(index);
        }
        if !requests.is_empty() {
            for (index, member) in self.members.iter().enumerate() {
                let mut sources = requests
                    .keys()
                    .filter(|(kind, from, to, source, target)| {
                        *kind == member.kind
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
                    let Some(range) = pointer_range(member, decode, source)? else {
                        continue;
                    };
                    note_scan();
                    member.indices.scan_range(
                        decode,
                        &["target", "uid"],
                        range,
                        |row, record| {
                            let uid = string(field(record, "uid")?)?
                                .filter(|uid| !uid.is_empty())
                                .ok_or(DataReadError::InvalidCsr("edge UID"))?;
                            bounded(uid.len(), limits.max_string_bytes, "edge UID")?;
                            let local = uint(field(record, "target")?)?;
                            if local >= member.target_rows {
                                return Err(DataReadError::InvalidCsr("target shard bounds"));
                            }
                            if used.contains(&(index, row)) {
                                return Ok(());
                            }
                            if let Some(queues) = requests.get_mut(&(
                                member.kind.as_str(),
                                member.from,
                                member.to,
                                source,
                                member.target_row_base + local,
                            )) && let Some(pending) = queues.get_mut(uid)
                                && let Some(selected) = pending.pop()
                            {
                                group[selected].member = index;
                                group[selected].row = row;
                                used.insert((index, row));
                            }
                            Ok(())
                        },
                    )?;
                }
            }
            if requests
                .values()
                .any(|queues| queues.values().any(|pending| !pending.is_empty()))
            {
                return Err(DataReadError::InvalidCsr("missing forward edge"));
            }
        }
        for candidate in group
            .iter_mut()
            .filter(|candidate| !candidate.reverse && candidate.incoming)
        {
            candidate.source =
                source_for_row(&self.members[candidate.member], decode, candidate.row)?;
        }
        let mut endpoints = Vec::new();
        reserve(
            &mut endpoints,
            add(group.len(), group.len(), "position endpoints")?,
            "position endpoints",
        )?;
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
            note_scan();
            table.scan_selected(decode, &[pk], &rows, |row, record| {
                let key = primary_key(field(record, pk)?, limits.max_string_bytes)?;
                if !seen.insert(key.clone()) {
                    return Err(DataReadError::DuplicatePrimaryKey);
                }
                keys.insert(self.node_id(index, row), key);
                Ok(())
            })?;
        }
        let mut positioned = Vec::new();
        reserve(&mut positioned, group.len(), "logical positions")?;
        for candidate in group {
            let member = &self.members[candidate.member];
            let position = OwnedNeighborhoodPosition {
                uid: copy_text(&candidate.uid)?,
                relationship: limits.text(&member.kind)?,
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
    pub fn neighborhood_graph(
        &self,
        request: GraphNeighborhoodRequest<'_>,
        limits: GraphNeighborhoodLimits,
    ) -> Result<GraphNeighborhoodPage> {
        self.neighborhood_graph_inner(None, request, limits)
    }
    /// Reuse an anchor resolved by this same capture, checking its selected key.
    pub fn neighborhood_graph_at(
        &self,
        anchor: &ResolvedGraphNode<'_>,
        request: GraphNeighborhoodRequest<'_>,
        limits: GraphNeighborhoodLimits,
    ) -> Result<GraphNeighborhoodPage> {
        self.context.check()?;
        if !matches!(anchor.graph, GraphSchemaSource::Local(graph) if std::ptr::eq(graph, self)) {
            return Err(DataReadError::InvalidAnchorKey);
        }
        self.neighborhood_graph_inner(Some(anchor.id), request, limits)
    }
    fn neighborhood_graph_inner(
        &self,
        resolved: Option<NodeId>,
        request: GraphNeighborhoodRequest<'_>,
        limits: GraphNeighborhoodLimits,
    ) -> Result<GraphNeighborhoodPage> {
        reset_metrics();
        self.context.check()?;
        if !(1..=1000).contains(&request.limit) {
            return Err(DataReadError::InvalidNeighborhoodRequest);
        }
        for name in std::iter::once(request.anchor_label)
            .chain(request.relationship)
            .chain(request.other_label)
        {
            if name.is_empty() || name.len() > self.policy.0.max_name_bytes {
                return Err(DataReadError::InvalidNeighborhoodRequest);
            }
        }
        if let GraphAnchorKey::String(key) = request.anchor_key {
            bounded(key.len(), limits.max_string_bytes, "anchor key")?;
        }
        let find = |label| {
            self.manifest
                .node_tables
                .iter()
                .position(|t| t.label == label)
                .ok_or(DataReadError::UnknownNodeLabel)
        };
        let anchor_table = find(request.anchor_label)?;
        let other = request.other_label.map(find).transpose()?;
        if resolved.is_some_and(|anchor| {
            anchor.table as usize != anchor_table || !self.nodes[anchor_table].contains(anchor.row)
        }) {
            return Err(DataReadError::InvalidAnchorKey);
        }
        if request
            .relationship
            .is_some_and(|kind| !self.manifest.relationships.iter().any(|r| r.kind == kind))
        {
            return Err(DataReadError::UnknownRelationship);
        }
        let relevant = |m: &MemberTable| {
            request.relationship.is_none_or(|kind| kind == m.kind)
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
            return Err(DataReadError::UnavailableRelationshipEndpoint);
        }
        let pk = &self.manifest.node_tables[anchor_table].primary_key;
        let table = &self.nodes[anchor_table];
        let kind = table
            .fields
            .iter()
            .find(|(name, _)| name == pk)
            .map(|(_, kind)| kind);
        if !(table.fields.is_empty() && table.rows == 0)
            && !matches!(
                (kind, request.anchor_key),
                (Some(Scalar::String), GraphAnchorKey::String(_))
                    | (Some(Scalar::Int64), GraphAnchorKey::Int64(_))
            )
        {
            return Err(DataReadError::InvalidAnchorKey);
        }
        let fingerprint = fingerprint(&request)?;
        validate_cursor(
            &request,
            fingerprint,
            self.policy.0.max_name_bytes,
            limits.max_string_bytes,
        )?;
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
            let anchor = if let Some(anchor) = resolved {
                note_scan();
                table
                    .scan_selected(&decode, &[pk], &[anchor.row], |_, record| {
                        let found = match (request.anchor_key, field(record, pk)?) {
                            (GraphAnchorKey::String(a), Field::Str(b)) => a == b,
                            (GraphAnchorKey::Int64(a), Field::Long(b)) => a == *b,
                            _ => false,
                        };
                        if !found {
                            return Err(DataReadError::InvalidAnchorKey);
                        }
                        Ok(())
                    })
                    .map(|()| anchor.row)
            } else {
                self.resolve_node_inner(
                    anchor_table,
                    request.anchor_key,
                    limits.max_string_bytes,
                    &decode,
                )
                .map(|anchor| anchor.row)
            };
            step("anchor");
            let anchor = anchor?;
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
                let range = if !traversal.incoming || traversal.reverse {
                    pointer_range(member, &decode, anchor)?
                } else {
                    None
                };
                if (!traversal.incoming || traversal.reverse) && range.is_none() {
                    continue;
                }
                let skip_loop = if traversal.incoming
                    && !traversal.reverse
                    && request.direction.is_none()
                    && member.from == anchor_table
                {
                    pointer_range(member, &decode, anchor)?
                } else {
                    None
                };
                note_scan();
                let mut consume = |row, record: &Row| {
                    let local = uint(field(record, "target")?)?;
                    if local >= member.target_rows {
                        return Err(DataReadError::InvalidCsr("target shard bounds"));
                    }
                    let target = member.target_row_base + local;
                    if !self.nodes[member.to].contains(target) {
                        return Err(DataReadError::InvalidCsr("target bounds"));
                    }
                    let uid = string(field(record, "uid")?)?
                        .filter(|uid| !uid.is_empty())
                        .ok_or(DataReadError::InvalidCsr("edge UID"))?;
                    bounded(uid.len(), limits.max_string_bytes, "edge UID")?;
                    if traversal.incoming && !traversal.reverse && target != anchor {
                        return Ok(());
                    }
                    if traversal.incoming
                        && request.direction.is_none()
                        && (skip_loop.as_ref().is_some_and(|range| range.contains(&row))
                            || traversal.reverse && member.to == anchor_table && target == anchor)
                    {
                        return Ok(());
                    }
                    bounded(
                        add(raw.len(), 1, "matching edges")?,
                        limits.max_matching_edges,
                        "matching edges",
                    )?;
                    reserve(&mut raw, 1, "matching edges")?;
                    raw.push(RawCandidate {
                        uid: copy_text(uid)?,
                        member: index,
                        row,
                        source: if traversal.reverse { target } else { anchor },
                        target: if traversal.reverse { anchor } else { target },
                        reverse: traversal.reverse,
                        incoming: traversal.incoming,
                    });
                    step("candidate");
                    Ok(())
                };
                if let Some(range) = range {
                    member
                        .indices
                        .scan_range(&decode, &["target", "uid"], range, &mut consume)?;
                } else {
                    let Some(target) = anchor
                        .checked_sub(member.target_row_base)
                        .filter(|row| *row < member.target_rows)
                    else {
                        continue;
                    };
                    member.indices.scan_keys(
                        &decode,
                        "target",
                        &[crate::parquet::LookupKey::UInt64(target)],
                        limits.max_string_bytes,
                        &["target", "uid"],
                        &mut consume,
                    )?;
                }
            }
            step("selection");
            let raw = candidate_prefix(raw, &request)?;
            let group = self.position_group(raw, limits, &decode)?;
            let mut candidates = Vec::new();
            append_group(&mut candidates, group, &request)?;
            let has_more = candidates.len() > request.limit;
            if has_more {
                candidates.pop();
            }
            let mut endpoints = Vec::new();

            reserve(
                &mut endpoints,
                add(candidates.len(), candidates.len(), "endpoints")?,
                "endpoint allocation",
            )?;
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
                let selected = rows.iter().map(|(row, _)| *row).collect::<Vec<_>>();
                member
                    .indices
                    .scan_selected(&decode, &all, &selected, |row, record| {
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
                                    member: u32::try_from(index).map_err(|_| {
                                        DataReadError::LimitExceeded("member identity")
                                    })?,
                                    row,
                                },
                                from: self.node_id(member.from, c.source),
                                to: self.node_id(member.to, c.target),
                                relationship: limits.text(&member.kind)?,
                                properties,
                                uid_index,
                                position: c.position.clone(),
                            });
                        }
                        Ok(())
                    })?;
            }
            edges.sort_unstable_by(|a, b| a.position().cmp(&b.position()));
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

                let selected = endpoints
                    .iter()
                    .filter(|id| id.table as usize == index)
                    .map(|id| id.row)
                    .collect::<Vec<_>>();
                note_scan();
                table.scan_selected(&decode, &names, &selected, |row, record| {
                    validate_properties(record, table, limits.max_string_bytes)?;
                    let id = self.node_id(index, row);
                    if endpoints.binary_search(&id).is_ok() {
                        nodes.push(GraphPropertyNode {
                            id,
                            label: limits.text(&self.manifest.node_tables[index].label)?,
                            properties: limits.properties(record, &names)?,
                            primary_key_index: names
                                .binary_search(
                                    &self.manifest.node_tables[index].primary_key.as_str(),
                                )
                                .map_err(|_| DataReadError::InvalidParquet("primary key"))?,
                        });
                    }
                    Ok(())
                })?;
                let pk = &self.manifest.node_tables[index].primary_key;
                let lookups = nodes
                    .iter()
                    .filter(|node| node.id.table as usize == index)
                    .map(|node| {
                        let value = node
                            .properties
                            .iter()
                            .find(|property| property.name == *pk)
                            .ok_or(DataReadError::InvalidParquet("primary key"))?;
                        match &value.value {
                            PropertyValue::String(value) => {
                                Ok(crate::parquet::LookupKey::String(value))
                            }
                            PropertyValue::Int64(value) => {
                                Ok(crate::parquet::LookupKey::Int64(*value))
                            }
                            _ => Err(DataReadError::InvalidParquet("nonnull primary key")),
                        }
                    })
                    .collect::<Result<Vec<_>>>()?;
                let mut keys = Keys {
                    values: HashSet::new(),
                    strings: limits.max_string_bytes,
                };
                note_scan();
                table.scan_keys(
                    &decode,
                    pk,
                    &lookups,
                    limits.max_string_bytes,
                    &[pk],
                    |_, record| keys.insert(field(record, pk)?),
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
            fingerprint,
            anchor: rows.anchor,
            nodes: rows.nodes,
            edges: rows.edges,
            has_more: rows.has_more,
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
pub(crate) use tests::{
    corrupt_unselected_property_fixture, parallel_fixture, pruned_corrupt_group_fixture,
    repeated_reverse_fixture, reverse_fixture, shared_reverse_fixture, shuffled_fixture,
    truncated_key_fixture,
};

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
