//! Pure, bounded authored references borrowing an existing parsed document.
//!
//! No lookup or authority is inferred from the document or its namespace.
use crate::document::{
    DocumentParseError, DocumentParseLimits, MetadataKind, MetadataNode, NodeId, ParsedDocument,
    parse_document,
};
use graphit_core::file_io::SnapshotContext;
use graphit_records::reference_projection::{
    AuthoredReferenceEntry, AuthoredReferenceUpdate, AuthoredReferencesField, LogicalScope,
    ReferenceCandidate, ReferenceProjectionError, SourceNamespace, project_authored_references,
};
use serde::{
    Deserialize, Deserializer,
    de::{self, DeserializeSeed, IgnoredAny, MapAccess, Visitor},
};
use std::{collections::TryReserveError, error::Error, fmt, io, num::NonZeroUsize};

/// Limits on the returned tuples and on each identifier in one document.
#[derive(Clone, Copy, Debug)]
pub struct DocumentAuthoredReferenceLimits {
    pub max_results: NonZeroUsize,
    pub max_identifier_bytes: NonZeroUsize,
}

#[derive(Debug)]
pub enum DocumentAuthoredReferenceError {
    Request(io::Error),
    Syntax(DocumentParseError),
    SemanticYaml(serde_saphyr::Error),
    Kernel(ReferenceProjectionError),
    InvalidEnvelope,
    LimitExceeded(&'static str),
    Allocation(TryReserveError),
}
impl fmt::Display for DocumentAuthoredReferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(e) => write!(f, "authored references request: {e}"),
            Self::Syntax(e) => write!(f, "authored references syntax: {e}"),
            Self::SemanticYaml(e) => write!(f, "authored references YAML: {e}"),
            Self::Kernel(e) => write!(f, "authored references: {e}"),
            Self::InvalidEnvelope => f.write_str("document source and metadata are inconsistent"),
            Self::LimitExceeded(name) => write!(f, "authored references limit exceeded: {name}"),
            Self::Allocation(e) => write!(f, "authored references allocation: {e}"),
        }
    }
}
impl Error for DocumentAuthoredReferenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(e) => Some(e),
            Self::Syntax(e) => Some(e),
            Self::SemanticYaml(e) => Some(e),
            Self::Kernel(e) => Some(e),
            Self::Allocation(e) => Some(e),
            _ => None,
        }
    }
}
type Result<T> = std::result::Result<T, DocumentAuthoredReferenceError>;
fn check(original: &SnapshotContext) -> Result<()> {
    original
        .check()
        .map_err(DocumentAuthoredReferenceError::Request)
}
fn bound(n: usize, cap: NonZeroUsize, name: &'static str) -> Result<()> {
    if n > cap.get() {
        Err(DocumentAuthoredReferenceError::LimitExceeded(name))
    } else {
        Ok(())
    }
}
fn shape() -> DocumentAuthoredReferenceError {
    DocumentAuthoredReferenceError::Kernel(ReferenceProjectionError::InvalidShape)
}

/// Return a complete preserve/replace decision, with literal order and duplicates.
///
/// `ParsedDocument` retains no request identity or parser policy. The supplied
/// original is checked for liveness; actual source bytes and SDK work are bounded
/// using `parse_limits`, independently of the document's mutable statistics.
/// The result borrows the parsed owner (including decoded scalar allocations):
/// ```compile_fail
/// use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
/// use graphit_knowledge::{document::*, document_authored_references::*, document_authored_references::DocumentAuthoredReferenceLimits};
/// use graphit_records::reference_projection::*;
/// use std::{num::NonZeroUsize, time::{Duration, Instant}};
/// let n = NonZeroUsize::new(10000).unwrap();
/// let p = DocumentParseLimits { max_document_bytes:n, max_input_bytes:n,
/// max_frontmatter_bytes:n, max_yaml_events:n, max_markdown_events:n,
/// max_yaml_depth:n, max_scalar_bytes:n, max_links:n, max_projection_bytes:n, max_summary_bytes:n };
/// let original = SnapshotContext::new(Instant::now()+Duration::from_secs(30), Cancellation::default());
/// let source = "---\nreferences: [{target: {type: memory, id: 'a\\nb'}, relation: supports}]\n---\n";
/// let output = {
///     let parsed = parse_document(&original, source.as_bytes(), "fallback", p).unwrap();
///     project_document_authored_references(&original, &parsed,
///         SourceNamespace {scope:LogicalScope::User("owner"),context:None}, p,
///         DocumentAuthoredReferenceLimits {max_results:n,max_identifier_bytes:n}).unwrap()
/// };
/// println!("{output:?}");
/// ```
pub fn project_document_authored_references<'text>(
    original: &SnapshotContext,
    parsed: &'text ParsedDocument<'_>,
    source: SourceNamespace<'text>,
    parse_limits: DocumentParseLimits,
    limits: DocumentAuthoredReferenceLimits,
) -> Result<AuthoredReferenceUpdate<'text>> {
    check(original)?;
    bound(
        parsed.source.len(),
        parse_limits.max_document_bytes,
        "document bytes",
    )?;
    bound(
        parsed.source.len(),
        parse_limits.max_input_bytes,
        "input bytes",
    )?;
    for namespace in [
        match source.scope {
            LogicalScope::User(s) => Some(s),
            _ => None,
        },
        source.context,
    ]
    .into_iter()
    .flatten()
    {
        bound(
            namespace.len(),
            limits.max_identifier_bytes,
            "namespace identifier bytes",
        )?;
    }
    if let Some(range) = &parsed.envelope.yaml {
        let yaml = parsed
            .source
            .get(range.clone())
            .ok_or(DocumentAuthoredReferenceError::InvalidEnvelope)?;
        bound(
            yaml.len(),
            parse_limits.max_frontmatter_bytes,
            "frontmatter bytes",
        )?;
    }
    let actual = parse_document(original, parsed.source.as_bytes(), "", parse_limits)
        .map_err(DocumentAuthoredReferenceError::Syntax)?;
    if actual.envelope != parsed.envelope
        || actual.metadata.root != parsed.metadata.root
        || actual.metadata.nodes().len() != parsed.metadata.nodes().len()
        || !actual
            .metadata
            .nodes()
            .iter()
            .zip(parsed.metadata.nodes())
            .all(|(a, b)| same_node(a, b))
    {
        return Err(DocumentAuthoredReferenceError::InvalidEnvelope);
    }
    let Some(range) = &parsed.envelope.yaml else {
        if parsed.metadata.root.is_some() || !parsed.metadata.nodes().is_empty() {
            return Err(DocumentAuthoredReferenceError::InvalidEnvelope);
        }
        return project_authored_references(original, source, AuthoredReferencesField::Omitted)
            .map_err(DocumentAuthoredReferenceError::Kernel);
    };
    let yaml = parsed
        .source
        .get(range.clone())
        .ok_or(DocumentAuthoredReferenceError::InvalidEnvelope)?;
    bound(
        yaml.len(),
        parse_limits.max_frontmatter_bytes,
        "frontmatter bytes",
    )?;
    let view = DocumentView {
        original,
        parsed,
        parse_limits,
        limits,
    };
    let candidates = view.candidates()?;
    // Permissive nonfinite handling belongs only to unrelated metadata. Plain
    // reserved scalars already passed strict SDK admission; quoted and block
    // strings retain the original whole-document indentation context here.
    let mut state = RootState {
        original,
        local_failure: None,
    };
    let decoded = serde_saphyr::with_deserializer_from_str_with_options(
        yaml,
        options(parse_limits, false),
        |d| RootSeed(&mut state).deserialize(d),
    );
    if let Some(error) = state.local_failure {
        return Err(error);
    }
    let semantic = decoded.map_err(DocumentAuthoredReferenceError::SemanticYaml)?;
    check(original)?;
    match (&candidates, &semantic) {
        (None, None) => {}
        (Some(entries), Some(rows)) if entries.len() == rows.len() => {
            for (candidate, row) in entries.iter().zip(rows) {
                check(original)?;
                let AuthoredReferenceEntry::Tuple(c) = candidate else {
                    return Err(shape());
                };
                if !row.matches(*c) {
                    return Err(DocumentAuthoredReferenceError::InvalidEnvelope);
                }
            }
        }
        _ => return Err(DocumentAuthoredReferenceError::InvalidEnvelope),
    }
    let field = match &candidates {
        None => AuthoredReferencesField::Omitted,
        Some(entries) => AuthoredReferencesField::Sequence(entries),
    };
    project_authored_references(original, source, field)
        .map_err(DocumentAuthoredReferenceError::Kernel)
}
fn same_node(a: &MetadataNode<'_>, b: &MetadataNode<'_>) -> bool {
    a.span == b.span
        && match (&a.kind, &b.kind) {
            (
                MetadataKind::Scalar {
                    value: a,
                    style: sa,
                },
                MetadataKind::Scalar {
                    value: b,
                    style: sb,
                },
            ) => a == b && sa == sb,
            (MetadataKind::Sequence { items: a }, MetadataKind::Sequence { items: b }) => a == b,
            (MetadataKind::Mapping { entries: a }, MetadataKind::Mapping { entries: b }) => a == b,
            _ => false,
        }
}
fn options(limits: DocumentParseLimits, reject_nonfinite: bool) -> serde_saphyr::Options {
    let budget = serde_saphyr::budget! {
        max_reader_input_bytes: None, max_buffered_comment_events: 0,
        simple_key_max_lookahead: limits.max_frontmatter_bytes.get(),
        flow_nesting_limit: limits.max_yaml_depth.get(), max_events: limits.max_yaml_events.get(),
        max_aliases: 0, max_anchors: 0, max_recorded_anchor_events: 0, max_recorded_anchor_bytes: 0,
        max_property_expansion_depth: 0, max_total_property_interpolation_work: 0,
        max_depth: limits.max_yaml_depth.get(), max_inclusion_depth: 0, max_documents: 1,
        max_nodes: limits.max_yaml_events.get(), max_total_scalar_bytes: limits.max_projection_bytes.get(),
        max_total_comment_bytes: 0, max_merge_keys: 0,
    };
    serde_saphyr::options! {
        budget: budget, no_schema: false, strict_booleans: false, legacy_octal_numbers: false,
        merge_keys: serde_saphyr::options::MergeKeyPolicy::Error,
        duplicate_keys: serde_saphyr::options::DuplicateKeyPolicy::FirstWins,
        emit_comments: false, with_snippet: false, reject_unsupported_tags: true,
        reject_non_finite_typeless_float: reject_nonfinite,
    }
}
struct DocumentView<'request, 'text, 'input> {
    original: &'request SnapshotContext,
    parsed: &'text ParsedDocument<'input>,
    parse_limits: DocumentParseLimits,
    limits: DocumentAuthoredReferenceLimits,
}
impl<'text, 'input> DocumentView<'_, 'text, 'input> {
    fn node(&self, id: NodeId) -> Result<&'text MetadataNode<'input>> {
        self.parsed
            .metadata
            .node(id)
            .ok_or(DocumentAuthoredReferenceError::InvalidEnvelope)
    }
    fn scalar(&self, id: NodeId) -> Result<&'text str> {
        check(self.original)?;
        let node = self.node(id)?;
        let MetadataKind::Scalar { value, style } = &node.kind else {
            return Err(shape());
        };
        let range = self
            .parsed
            .envelope
            .yaml
            .as_ref()
            .ok_or(DocumentAuthoredReferenceError::InvalidEnvelope)?;
        if node.span.start < range.start || node.span.end > range.end {
            return Err(DocumentAuthoredReferenceError::InvalidEnvelope);
        }
        let raw = self
            .parsed
            .source
            .get(node.span.clone())
            .ok_or(DocumentAuthoredReferenceError::InvalidEnvelope)?;
        if *style == serde_saphyr::granit_parser::ScalarStyle::Plain {
            let typed: StringScalar =
                serde_saphyr::from_str_with_options(raw, options(self.parse_limits, true))
                    .map_err(DocumentAuthoredReferenceError::SemanticYaml)?;
            check(self.original)?;
            if typed.0 != value.as_ref() {
                return Err(DocumentAuthoredReferenceError::InvalidEnvelope);
            }
        }
        bound(
            value.len(),
            self.parse_limits.max_scalar_bytes,
            "scalar bytes",
        )?;
        Ok(value.as_ref())
    }
    fn fields<const N: usize>(&self, id: NodeId, names: [&str; N]) -> Result<[Option<NodeId>; N]> {
        let MetadataKind::Mapping { entries } = &self.node(id)?.kind else {
            return Err(shape());
        };
        let mut fields = [None; N];
        for (key, value) in entries {
            check(self.original)?;
            let key = self.scalar(*key)?;
            let Some(index) = names.iter().position(|name| *name == key) else {
                return Err(shape());
            };
            if fields[index].replace(*value).is_some() {
                return Err(shape());
            }
        }
        Ok(fields)
    }
    fn candidates(&self) -> Result<Option<Vec<AuthoredReferenceEntry<'text>>>> {
        let Some(root) = self.parsed.metadata.root else {
            return Ok(None);
        };
        let root = self.node(root)?;
        let MetadataKind::Mapping { entries } = &root.kind else {
            // Empty/null frontmatter is checked by the full SDK root visitor.
            if matches!(root.kind, MetadataKind::Scalar { .. }) {
                return Ok(None);
            }
            return Err(shape());
        };
        let mut references = None;
        for (key, value) in entries {
            check(self.original)?;
            if matches!(&self.node(*key)?.kind, MetadataKind::Scalar {value,..} if value == "references")
                && references.replace(*value).is_some()
            {
                return Err(shape());
            }
        }
        let Some(id) = references else {
            return Ok(None);
        };
        let MetadataKind::Sequence { items } = &self.node(id)?.kind else {
            return Err(shape());
        };
        bound(items.len(), self.limits.max_results, "reference results")?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(items.len())
            .map_err(DocumentAuthoredReferenceError::Allocation)?;
        for id in items {
            check(self.original)?;
            let [target, relation, field] = self.fields(*id, ["target", "relation", "field"])?;
            let [kind, target_id, scope, scope_id, context] = self.fields(
                target.ok_or_else(shape)?,
                ["type", "id", "scope", "scope_id", "context"],
            )?;
            let c = ReferenceCandidate {
                target_type: self.scalar(kind.ok_or_else(shape)?)?,
                id: self.scalar(target_id.ok_or_else(shape)?)?,
                scope: scope.map(|v| self.scalar(v)).transpose()?,
                scope_id: scope_id.map(|v| self.scalar(v)).transpose()?,
                context: context.map(|v| self.scalar(v)).transpose()?,
                relation: self.scalar(relation.ok_or_else(shape)?)?,
                field: field.map(|v| self.scalar(v)).transpose()?,
            };
            for text in [Some(c.id), c.scope_id, c.context].into_iter().flatten() {
                bound(
                    text.len(),
                    self.limits.max_identifier_bytes,
                    "identifier bytes",
                )?;
            }
            output.push(AuthoredReferenceEntry::Tuple(c));
        }
        Ok(Some(output))
    }
}
#[derive(Debug)]
struct StringScalar(String);
impl<'de> Deserialize<'de> for StringScalar {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct StringVisitor;
        impl<'de> Visitor<'de> for StringVisitor {
            type Value = StringScalar;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a YAML string")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> std::result::Result<Self::Value, E> {
                Ok(StringScalar(s.into()))
            }
            fn visit_string<E: de::Error>(self, s: String) -> std::result::Result<Self::Value, E> {
                Ok(StringScalar(s))
            }
        }
        d.deserialize_any(StringVisitor)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SemanticTarget {
    #[serde(rename = "type")]
    kind: StringScalar,
    id: StringScalar,
    scope: Option<StringScalar>,
    scope_id: Option<StringScalar>,
    context: Option<StringScalar>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SemanticEntry {
    target: SemanticTarget,
    relation: StringScalar,
    field: Option<StringScalar>,
}
impl SemanticEntry {
    fn matches(&self, c: ReferenceCandidate<'_>) -> bool {
        self.target.kind.0 == c.target_type
            && self.target.id.0 == c.id
            && self.relation.0 == c.relation
            && self.target.scope.as_ref().map(|s| s.0.as_str()) == c.scope
            && self.target.scope_id.as_ref().map(|s| s.0.as_str()) == c.scope_id
            && self.target.context.as_ref().map(|s| s.0.as_str()) == c.context
            && self.field.as_ref().map(|s| s.0.as_str()) == c.field
    }
}
struct RootState<'a> {
    original: &'a SnapshotContext,
    local_failure: Option<DocumentAuthoredReferenceError>,
}
struct RootSeed<'a, 'b>(&'a mut RootState<'b>);
impl<'de> DeserializeSeed<'de> for RootSeed<'_, '_> {
    type Value = Option<Vec<SemanticEntry>>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for RootSeed<'_, '_> {
    type Value = Option<Vec<SemanticEntry>>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a frontmatter mapping or null")
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_map<M: MapAccess<'de>>(
        self,
        mut map: M,
    ) -> std::result::Result<Self::Value, M::Error> {
        let mut references = None;
        while let Some(key) = map.next_key_seed(RootKey)? {
            if let Err(error) = check(self.0.original) {
                self.0.local_failure = Some(error);
                return Err(de::Error::custom("original request failed"));
            }
            if key {
                references = Some(map.next_value()?);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(references)
    }
}
struct RootKey;
impl<'de> DeserializeSeed<'de> for RootKey {
    type Value = bool;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<bool, D::Error> {
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for RootKey {
    type Value = bool;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a primitive frontmatter key")
    }
    fn visit_str<E: de::Error>(self, s: &str) -> std::result::Result<bool, E> {
        Ok(s == "references")
    }
    fn visit_string<E: de::Error>(self, s: String) -> std::result::Result<bool, E> {
        self.visit_str(&s)
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<bool, E> {
        Ok(false)
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> std::result::Result<bool, E> {
        Ok(false)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> std::result::Result<bool, E> {
        Ok(false)
    }
    fn visit_i128<E: de::Error>(self, _: i128) -> std::result::Result<bool, E> {
        Ok(false)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> std::result::Result<bool, E> {
        Ok(false)
    }
    fn visit_u128<E: de::Error>(self, _: u128) -> std::result::Result<bool, E> {
        Ok(false)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> std::result::Result<bool, E> {
        Ok(false)
    }
}
