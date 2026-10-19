//! SCIP projection of the existing syntax graph, without consuming its richer observations.
//!
//! Symbols identify a logical project/version, relative file and graph entity. Definitions use
//! genuine Original name ranges; references require a resolved local target and an exact Original
//! capture. Unresolved, compound and observed-only relationships remain in `AstGraphParts`.
//! Complexity, custom labels, values, ownership and embedded-language evidence stay in that graph;
//! they are not repackaged as SCIP diagnostics or documentation. The caller owns index streaming.

use crate::{
    RangeError, SourceRange, SourceText,
    semantic_graph::{
        AstGraphNode, AstGraphParts, AstGraphRelationOutcome, AstGraphRelationTarget, AstGraphValue,
    },
};
use graphit_core::file_io::SnapshotContext;
use protobuf::{Enum, MessageField};
use scip::{
    symbol,
    types::{self, descriptor, symbol_information},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, io,
    sync::Arc,
};

/// Borrowed inputs: no parsing, graph mutation, filesystem access or source reconstruction occurs.
pub struct AstScipSyntaxInput<'a> {
    pub graph: &'a AstGraphParts,
    pub source: &'a str,
    pub project_id: &'a str,
    pub project_version: &'a str,
}

#[derive(Debug)]
pub enum AstScipSyntaxError {
    Operation(io::Error),
    SourceMismatch,
    InvalidGraph(&'static str),
    Range(RangeError),
    Metadata(serde_json::Error),
    Symbol(symbol::SymbolError),
}

impl fmt::Display for AstScipSyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(e) => e.fmt(f),
            Self::SourceMismatch => f.write_str("SCIP syntax Source association mismatch"),
            Self::InvalidGraph(reason) => write!(f, "SCIP syntax graph: {reason}"),
            Self::Range(e) => e.fmt(f),
            Self::Metadata(e) => e.fmt(f),
            Self::Symbol(e) => write!(f, "SCIP syntax symbol: {e:?}"),
        }
    }
}
impl Error for AstScipSyntaxError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(e) => Some(e),
            Self::Range(e) => Some(e),
            Self::Metadata(e) => Some(e),
            _ => None,
        }
    }
}
impl From<io::Error> for AstScipSyntaxError {
    fn from(e: io::Error) -> Self {
        Self::Operation(e)
    }
}
impl From<RangeError> for AstScipSyntaxError {
    fn from(e: RangeError) -> Self {
        Self::Range(e)
    }
}
impl From<serde_json::Error> for AstScipSyntaxError {
    fn from(e: serde_json::Error) -> Self {
        Self::Metadata(e)
    }
}
type Result<T> = std::result::Result<T, AstScipSyntaxError>;

fn invalid(reason: &'static str) -> AstScipSyntaxError {
    AstScipSyntaxError::InvalidGraph(reason)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn string<'a>(node: &'a AstGraphNode, key: &str) -> Result<&'a str> {
    match node.properties().get(key) {
        Some(AstGraphValue::String(value)) => Ok(value),
        _ => Err(invalid("missing or non-string entity property")),
    }
}
fn number(node: &AstGraphNode, key: &str) -> Result<usize> {
    match node.properties().get(key) {
        Some(AstGraphValue::Int64(value)) => {
            usize::try_from(*value).map_err(|_| invalid("negative or unrepresentable range"))
        }
        _ => Err(invalid("missing or non-integer range property")),
    }
}
fn json_number(value: &Value, key: &str) -> Result<usize> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| invalid("missing or invalid Original range property"))
}

fn checked_range<'a>(
    source: &SourceText<'a>,
    get: impl Fn(&str) -> Result<usize>,
) -> Result<SourceRange<'a>> {
    let range = source.range(get("start_byte")?, get("end_byte")?)?;
    let a = range.start_point();
    let b = range.end_point();
    for (name, actual) in [
        ("start_row", a.row),
        ("start_column", a.column),
        ("end_row", b.row),
        ("end_column", b.column),
    ] {
        if get(name)? != actual {
            return Err(invalid("Original byte offsets and points disagree"));
        }
    }
    Ok(range)
}
fn node_range<'a>(
    node: &AstGraphNode,
    prefix: &str,
    source: &SourceText<'a>,
) -> Result<SourceRange<'a>> {
    let range = checked_range(source, |key| number(node, &format!("{prefix}{key}")))?;
    if number(node, &format!("{prefix}start_line"))? != range.start_point().row + 1
        || number(node, &format!("{prefix}end_line"))? != range.end_point().row + 1
    {
        return Err(invalid("Original line numbers disagree"));
    }
    Ok(range)
}
fn json_range<'a>(value: &Value, source: &SourceText<'a>) -> Result<SourceRange<'a>> {
    checked_range(source, |key| json_number(value, key))
}
fn exact_origin<'a>(
    value: Option<&Value>,
    source: &SourceText<'a>,
) -> Result<Option<SourceRange<'a>>> {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    match value.get("status").and_then(Value::as_str) {
        Some("Exact") => Ok(Some(json_range(
            value
                .get("range")
                .ok_or_else(|| invalid("exact Original lacks range"))?,
            source,
        )?)),
        Some("Compound" | "Synthetic" | "Unmappable" | "Ambiguous") => Ok(None),
        _ => Err(invalid("unknown Original projection")),
    }
}
fn packed(range: SourceRange<'_>) -> Result<Vec<i32>> {
    let a = range.start_point();
    let b = range.end_point();
    let values = if a.row == b.row {
        vec![a.row, a.column, b.column]
    } else {
        vec![a.row, a.column, b.row, b.column]
    };
    values
        .into_iter()
        .map(|n| i32::try_from(n).map_err(|_| invalid("range exceeds SCIP coordinates")))
        .collect()
}
fn contains(outer: SourceRange<'_>, inner: SourceRange<'_>) -> bool {
    outer.start() <= inner.start() && inner.end() <= outer.end()
}

fn validate_path(parts: &AstGraphParts) -> Result<&str> {
    let association = &parts.association;
    let components = association.relative_components();
    if components.is_empty()
        || components
            .iter()
            .any(|s| s.is_empty() || s == "." || s == ".." || s.contains(['/', '\\', '\0']))
        || components.join("/") != association.path()
    {
        return Err(invalid("invalid logical relative path"));
    }
    let path = association.path();
    if path.as_bytes().get(1) == Some(&b':') && path.as_bytes()[0].is_ascii_alphabetic() {
        return Err(invalid("drive-qualified path"));
    }
    Ok(path)
}
fn syntax_symbol(input: &AstScipSyntaxInput<'_>, node: &AstGraphNode) -> Result<String> {
    let descriptors = [
        input.project_id,
        input.project_version,
        input.graph.association.path(),
    ]
    .into_iter()
    .map(|name| types::Descriptor {
        name: name.to_owned(),
        suffix: descriptor::Suffix::Namespace.into(),
        ..Default::default()
    })
    .chain(std::iter::once(types::Descriptor {
        name: hex(&Sha256::digest(node.key().as_bytes())),
        suffix: descriptor::Suffix::Term.into(),
        ..Default::default()
    }))
    .collect();
    let value = symbol::format_symbol(types::Symbol {
        scheme: "graphit-syntax".into(),
        package: MessageField::some(types::Package {
            manager: "graphit".into(),
            name: "syntax".into(),
            ..Default::default()
        }),
        descriptors,
        ..Default::default()
    });
    symbol::parse_symbol(&value).map_err(AstScipSyntaxError::Symbol)?;
    Ok(value)
}

fn metadata(node: &AstGraphNode) -> Result<Value> {
    let value: Value = serde_json::from_str(string(node, "metadata_json")?)?;
    if !value.is_object() || !value.get("contributions").is_some_and(Value::is_array) {
        return Err(invalid("entity metadata lacks native contributions"));
    }
    Ok(value)
}
fn attachments(
    value: &Value,
    source: &SourceText<'_>,
    docs: &mut Vec<String>,
    original: &SnapshotContext,
) -> Result<()> {
    if let Some(items) = value.get("attachments").and_then(Value::as_array) {
        for item in items {
            original.check()?;
            let Some(range) = exact_origin(item.get("original"), source)? else {
                continue;
            };
            let text = match item.pointer("/content/kind").and_then(Value::as_str) {
                Some("Documentation") => item
                    .pointer("/content/text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("documentation lacks text"))?,
                Some("Comment") => {
                    let text = item
                        .get("literal_source")
                        .and_then(Value::as_str)
                        .ok_or_else(|| invalid("comment lacks Original text"))?;
                    if range.text() != text {
                        return Err(invalid("comment differs from Original"));
                    }
                    text
                }
                _ => continue,
            };
            if !docs.iter().any(|existing| existing == text) {
                docs.push(text.to_owned());
            }
        }
    }
    Ok(())
}
fn symbol_details(
    node: &AstGraphNode,
    source: &SourceText<'_>,
    original: &SnapshotContext,
) -> Result<(Vec<String>, MessageField<types::Signature>)> {
    let meta = metadata(node)?;
    let mut docs = Vec::new();
    let mut signatures = BTreeSet::new();
    if let Some(contributions) = meta.get("contributions").and_then(Value::as_array) {
        for contribution in contributions {
            original.check()?;
            for key in [
                "callable",
                "go_declaration",
                "ecma_declaration",
                "python_declaration",
            ] {
                let Some(value) = contribution.get(key).filter(|v| !v.is_null()) else {
                    continue;
                };
                attachments(value, source, &mut docs, original)?;
                if key == "callable"
                    && let Some(text) = value.get("signature_source").and_then(Value::as_str)
                    && let Some(range) = exact_origin(value.get("signature_origin"), source)?
                {
                    if range.text() != text {
                        return Err(invalid("signature differs from Original"));
                    }
                    signatures.insert(text.to_owned());
                }
                if key == "python_declaration"
                    && let Some(leading) = value.get("leading_string").filter(|v| !v.is_null())
                    && let Some(range) = exact_origin(leading.get("original"), source)?
                {
                    let text = range.text();
                    if !docs.iter().any(|existing| existing == text) {
                        docs.push(text.to_owned());
                    }
                }
            }
        }
    }
    // Conflicting observations remain in the graph; SCIP must not choose an invented signature.
    let signature = if signatures.len() == 1 {
        MessageField::some(types::Signature {
            language: string(node, "lang")?.into(),
            text: signatures.into_iter().next().expect("one signature"),
            ..Default::default()
        })
    } else {
        MessageField::none()
    };
    Ok((docs, signature))
}

fn reference_range<'a>(
    outcome: &AstGraphRelationOutcome,
    source: &SourceText<'a>,
) -> Result<Option<SourceRange<'a>>> {
    let observation: Value = serde_json::from_str(outcome.observation_json())?;
    if observation.get("original").is_some() {
        return exact_origin(observation.get("original"), source);
    }
    if let Some(captures) = observation.get("captures").and_then(Value::as_array) {
        let index = outcome
            .capture_index()
            .ok_or_else(|| invalid("resolved capture lacks index"))?;
        let capture = captures
            .get(index)
            .ok_or_else(|| invalid("resolved capture is foreign"))?;
        return exact_origin(capture.get("original"), source);
    }
    if let Some(name) = observation.pointer("/occurrence/name") {
        return exact_origin(name.pointer("/capture/original"), source);
    }
    Ok(None)
}

/// Project an already-associated graph into an official SCIP `Document`.
///
/// Coordinates are checked against the unchanged Original UTF-8 source, including advertised
/// byte points and line numbers. Exact extents become definition `enclosing_range`s. A reference
/// is emitted only when the graph resolves its target locally and retains an exact occurrence
/// anchor. No text matching or cross-file resolution is attempted. Unknown kinds remain SCIP's
/// `UnspecifiedKind`; nonrepresentable observations and conflicting signatures stay in the graph.
/// Returned occurrences and symbols are deterministically ordered; identical occurrences coalesce.
/// This function does not publish an artifact, and an error/cancellation returns no document.
pub fn produce_scip_syntax_document(
    input: AstScipSyntaxInput<'_>,
    original: &Arc<SnapshotContext>,
) -> Result<types::Document> {
    original.check()?;
    let result = (|| {
        if input.project_id.is_empty()
            || input.project_version.is_empty()
            || input.project_id.chars().any(char::is_control)
            || input.project_version.chars().any(char::is_control)
        {
            return Err(invalid("empty or invalid logical project identity"));
        }
        let graph = input.graph;
        let path = validate_path(graph)?;
        if Sha256::digest(input.source.as_bytes()).as_slice() != graph.association.source_sha256() {
            return Err(AstScipSyntaxError::SourceMismatch);
        }
        let source = SourceText::new(input.source.as_bytes(), input.source.len().max(1))?;
        let hash = hex(graph.association.source_sha256());
        let mut document = types::Document {
            relative_path: path.into(),
            language: string(&graph.file_observation, "lang")?.into(),
            position_encoding: types::PositionEncoding::UTF8CodeUnitOffsetFromLineStart.into(),
            ..Default::default()
        };
        let mut symbols = BTreeMap::new();
        let mut symbol_ids = BTreeSet::new();
        for node in &graph.nodes {
            original.check()?;
            if node.key().is_empty() || symbols.contains_key(node.key()) {
                return Err(invalid("duplicate or empty entity identity"));
            }
            if string(node, "path")? != path
                || string(node, "source_sha256")? != hash
                || string(node, "raw_version")? != graph.association.raw_version().to_string()
            {
                return Err(AstScipSyntaxError::SourceMismatch);
            }
            let name = node_range(node, "name_", &source)?;
            if name.text() != string(node, "raw_name")? {
                return Err(invalid("name differs from Original"));
            }
            let enclosing_range = match string(node, "extent_status")? {
                "Exact" => {
                    let extent = node_range(node, "", &source)?;
                    if !contains(extent, name) {
                        return Err(invalid("extent excludes its name"));
                    }
                    packed(extent)?
                }
                "Missing" | "NonExact" | "Conflict" | "Unavailable" => Vec::new(),
                _ => return Err(invalid("unknown entity extent")),
            };
            let symbol = syntax_symbol(&input, node)?;
            if !symbol_ids.insert(symbol.clone()) {
                return Err(invalid("SCIP symbol collision"));
            }
            let (documentation, signature_documentation) = symbol_details(node, &source, original)?;
            document.symbols.push(types::SymbolInformation {
                symbol: symbol.clone(),
                display_name: string(node, "name")?.into(),
                kind: symbol_information::Kind::from_str(node.label())
                    .unwrap_or(symbol_information::Kind::UnspecifiedKind)
                    .into(),
                documentation,
                signature_documentation,
                ..Default::default()
            });
            document.occurrences.push(types::Occurrence {
                range: packed(name)?,
                enclosing_range,
                symbol: symbol.clone(),
                symbol_roles: types::SymbolRole::Definition.value(),
                ..Default::default()
            });
            symbols.insert(node.key(), symbol);
        }
        for outcome in &graph.outcomes {
            original.check()?;
            let AstGraphRelationTarget::Resolved { key } = outcome.target() else {
                continue;
            };
            let Some(symbol) = symbols.get(key.as_str()) else {
                return Err(invalid("resolved syntax target is not local"));
            };
            if let Some(range) = reference_range(outcome, &source)? {
                document.occurrences.push(types::Occurrence {
                    range: packed(range)?,
                    symbol: symbol.clone(),
                    ..Default::default()
                });
            }
        }
        document.symbols.sort_by(|a, b| a.symbol.cmp(&b.symbol));
        document.occurrences.sort_by(|a, b| {
            (&a.range, &a.symbol, a.symbol_roles, &a.enclosing_range).cmp(&(
                &b.range,
                &b.symbol,
                b.symbol_roles,
                &b.enclosing_range,
            ))
        });
        document.occurrences.dedup();
        Ok(document)
    })();
    original.check()?;
    result
}
