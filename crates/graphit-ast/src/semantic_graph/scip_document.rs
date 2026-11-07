//! Apply official SCIP observations to the richer, Source-associated parser graph.
//!
//! Existing entities keep their identities and parser metadata. Additional compiler definitions
//! have their own source-bound identities. References become edges only in the caller's same-job
//! symbol join, and only when an exact parser observation establishes the relationship kind.
use super::*;
use crate::{SourceText, scip::coordinates};
use ::scip::{
    symbol,
    types::{self, Document, Occurrence, SymbolInformation, occurrence, symbol_information::Kind},
};
use graphit_data::{CodeDeclaration, CodeFileId, CodePrimaryKey, CodeScope, CodeUid};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The trusted producer identifies compiler observations versus our syntax projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AstScipDocumentOrigin {
    Compiler,
    Syntax,
}

pub struct AstScipGraphDocument {
    pub parts: AstGraphParts,
    pub definitions: Vec<AstScipDocumentDefinition>,
    pub references: Vec<AstScipDocumentReference>,
    pub identity_changes: Vec<AstGraphIdentityChange>,
    pub origin: AstScipDocumentOrigin,
}

/// A projection rekey from the same captured Source, suitable for preserving its vector row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AstGraphIdentityChange {
    pub label: String,
    pub data_key: String,
    pub start_byte: u64,
    pub end_byte: u64,
    pub previous_uid: String,
    pub uid: String,
    pub previous_key: String,
    pub key: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AstScipDocumentDefinition {
    pub association: AstGraphAssociation,
    pub path: String,
    pub symbol: String,
    pub start_byte: u64,
    pub end_byte: u64,
    pub symbol_roles: i32,
    pub entity_key: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AstScipDocumentReference {
    pub association: AstGraphAssociation,
    pub path: String,
    pub symbol: String,
    pub start_byte: u64,
    pub end_byte: u64,
    pub symbol_roles: i32,
    pub kind: Option<String>,
    pub source_key: Option<String>,
    pub query_data_key: Option<String>,
    pub reference_target: String,
    pub occurrence_ordinal: Option<u64>,
    pub relation_index: Option<usize>,
    pub source_index: Option<usize>,
    pub capture_index: Option<usize>,
    pub observation_json: Option<String>,
    pub target_labels: Vec<String>,
    pub graph_edge_uids: Vec<String>,
    pub parser_target: Option<String>,
}

/// Stable identity shared by immutable exports and the per-FILE native publisher.
pub fn scip_edge_uid(
    reference: &AstScipDocumentReference,
    target: &AstScipDocumentDefinition,
) -> Result<String, AstGraphError> {
    let kind = reference
        .kind
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid("SCIP edge lacks relation kind"))?;
    let source = reference
        .source_key
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid("SCIP edge lacks source identity"))?;
    let file = logical_file(&reference.association)?;
    let source = graphit_data::GraphAnchorKey::String(source);
    let occurrence = graphit_data::CodeRelationOccurrence::derive(
        file,
        source,
        kind,
        reference.query_data_key.as_deref(),
        &reference.reference_target,
        reference
            .occurrence_ordinal
            .ok_or_else(|| invalid("SCIP reference lacks logical occurrence"))?,
    )
    .map_err(|_| invalid("invalid SCIP reference identity"))?;
    graphit_data::CodeRelationUid::derive(
        file,
        source,
        graphit_data::GraphAnchorKey::String(&target.entity_key),
        kind,
        occurrence,
    )
    .map(|uid| uid.to_string())
    .map_err(|_| invalid("invalid SCIP relation identity"))
}

fn invalid(reason: &'static str) -> AstGraphError {
    AstGraphError::NativeContract(reason)
}
fn text<'a>(node: &'a AstGraphNode, key: &str) -> Option<&'a str> {
    match node.properties.get(key) {
        Some(AstGraphValue::String(value)) => Some(value),
        _ => None,
    }
}
fn byte(node: &AstGraphNode, key: &str) -> Option<usize> {
    match node.properties.get(key) {
        Some(AstGraphValue::Int64(value)) => usize::try_from(*value).ok(),
        _ => None,
    }
}
// One table per file avoids rescanning all preceding lines for each compiler occurrence.
fn source_offset(
    source: &SourceText<'_>,
    lines: &[usize],
    row: u32,
    column: u32,
    encoding: i32,
) -> Result<usize, AstGraphError> {
    let row = usize::try_from(row).map_err(|_| AstGraphError::Representation)?;
    let column = usize::try_from(column).map_err(|_| AstGraphError::Representation)?;
    let start = *lines
        .get(row)
        .ok_or_else(|| invalid("SCIP row is outside Source"))?;
    let end = lines
        .get(row + 1)
        .map(|next| next - 1)
        .unwrap_or(source.as_str().len());
    let text = &source.as_str()[start..end];
    if encoding == types::PositionEncoding::UTF8CodeUnitOffsetFromLineStart as i32 {
        if column <= text.len() && text.is_char_boundary(column) {
            return Ok(start + column);
        }
        return Err(invalid("SCIP byte column splits or exceeds Source"));
    }
    let utf16 = encoding == types::PositionEncoding::UTF16CodeUnitOffsetFromLineStart as i32;
    if !utf16 && encoding != types::PositionEncoding::UTF32CodeUnitOffsetFromLineStart as i32 {
        return Err(invalid("SCIP position encoding is unspecified or unknown"));
    }
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units == column {
            return Ok(start + byte);
        }
        units += if utf16 { ch.len_utf16() } else { 1 };
        if units > column {
            return Err(invalid("SCIP column splits Unicode scalar"));
        }
    }
    if units == column {
        Ok(end)
    } else {
        Err(invalid("SCIP column exceeds Source"))
    }
}
fn span<'a>(
    occurrence: &Occurrence,
    encoding: i32,
    source: &SourceText<'a>,
    lines: &[usize],
) -> Result<SourceRange<'a>, AstGraphError> {
    let range = coordinates(occurrence).map_err(|_| invalid("invalid SCIP occurrence range"))?;
    let start = source_offset(
        source,
        lines,
        range.start_line,
        range.start_character,
        encoding,
    )?;
    let end = source_offset(source, lines, range.end_line, range.end_character, encoding)?;
    source
        .range(start, end)
        .map_err(|_| invalid("invalid SCIP Source range"))
}
fn enclosing<'a>(
    occurrence: &Occurrence,
    encoding: i32,
    source: &SourceText<'a>,
    lines: &[usize],
) -> Result<Option<SourceRange<'a>>, AstGraphError> {
    if occurrence.enclosing_range.is_empty() && occurrence.typed_enclosing_range.is_none() {
        return Ok(None);
    }
    let value = Occurrence {
        range: occurrence.enclosing_range.clone(),
        typed_range: match &occurrence.typed_enclosing_range {
            Some(occurrence::Typed_enclosing_range::SingleLineEnclosingRange(value)) => {
                Some(occurrence::Typed_range::SingleLineRange(value.clone()))
            }
            Some(occurrence::Typed_enclosing_range::MultiLineEnclosingRange(value)) => {
                Some(occurrence::Typed_range::MultiLineRange(value.clone()))
            }
            None => None,
            Some(_) => return Err(invalid("unknown SCIP enclosing range")),
        },
        ..Default::default()
    };
    span(&value, encoding, source, lines).map(Some)
}
fn label(info: Option<&SymbolInformation>) -> &'static str {
    match info.and_then(|info| info.kind.enum_value().ok()) {
        Some(
            Kind::Function
            | Kind::Method
            | Kind::Constructor
            | Kind::AbstractMethod
            | Kind::Getter
            | Kind::Setter
            | Kind::StaticMethod,
        ) => "Function",
        Some(Kind::Class | Kind::Struct | Kind::Object) => "Struct",
        Some(Kind::Interface | Kind::Trait | Kind::Protocol) => "Interface",
        Some(Kind::Enum) => "Enum",
        Some(Kind::EnumMember) => "EnumMember",
        Some(Kind::Field | Kind::Property | Kind::StaticDataMember) => "Field",
        Some(Kind::Constant) => "Constant",
        Some(Kind::Namespace | Kind::Package | Kind::Module) => "Namespace",
        Some(Kind::Type | Kind::TypeAlias | Kind::AssociatedType | Kind::TypeParameter) => "Type",
        _ => "Variable",
    }
}
fn information(info: &SymbolInformation) -> Value {
    json!({"symbol":info.symbol,"kind":info.kind.value(),"display_name":info.display_name,
        "documentation":info.documentation,"enclosing_symbol":info.enclosing_symbol,
        "signature":info.signature_documentation.as_ref().map(|s|json!({"language":s.language,"text":s.text})),
        "relationships":info.relationships.iter().map(|r|json!({"symbol":r.symbol,"is_reference":r.is_reference,
            "is_implementation":r.is_implementation,"is_type_definition":r.is_type_definition,"is_definition":r.is_definition})).collect::<Vec<_>>()})
}
fn enrich(
    node: &mut AstGraphNode,
    occurrence: &Occurrence,
    info: Option<&SymbolInformation>,
) -> Result<(), AstGraphError> {
    let entry = json!({"symbol":occurrence.symbol,"symbol_roles":occurrence.symbol_roles,
        "override_documentation":occurrence.override_documentation,"information":info.map(information)});
    let key = "scip_symbols_json";
    let mut entries: Vec<Value> = match node.properties.get(key) {
        Some(AstGraphValue::String(value)) => serde_json::from_str(value)?,
        None => Vec::new(),
        _ => return Err(invalid("invalid SCIP symbol graph property")),
    };
    if !entries.contains(&entry) {
        entries.push(entry);
    }
    node.properties.insert(
        key.into(),
        AstGraphValue::String(serde_json::to_string(&entries)?),
    );
    Ok(())
}
fn new_definition(
    parts: &AstGraphParts,
    occurrence: &Occurrence,
    info: Option<&SymbolInformation>,
    name: SourceRange<'_>,
    extent: SourceRange<'_>,
    original: &SnapshotContext,
) -> Result<AstGraphNode, AstGraphError> {
    original.check()?;
    let components: Vec<_> = parts
        .association
        .relative_components
        .iter()
        .map(String::as_str)
        .collect();
    let file = CodeFileId::from_components(&components)
        .map_err(|_| invalid("invalid SCIP logical FILE"))?;
    let global = !symbol::is_local_symbol(&occurrence.symbol);
    let symbols = [occurrence.symbol.as_str()];
    let mut scopes = Vec::new();
    if let Some(enclosing) = info
        .map(|info| info.enclosing_symbol.as_str())
        .filter(|symbol| !symbol.is_empty())
    {
        symbol::parse_symbol(enclosing).map_err(|_| invalid("invalid SCIP enclosing symbol"))?;
        if !symbol::is_local_symbol(enclosing) {
            scopes.push(CodeScope::ScipSymbol(enclosing));
        }
    }
    let language =
        text(&parts.file_observation, "lang").ok_or_else(|| invalid("SCIP FILE lacks language"))?;
    let mut ordinal = 0u64;
    let code = loop {
        original.check()?;
        let declaration = if global {
            CodeDeclaration::Scip {
                symbols: &symbols,
                kind: label(info),
                ordinal,
            }
        } else {
            CodeDeclaration::Fallback {
                language,
                kind: label(info),
                owners: &scopes,
                name: name.text(),
                declared_type: None,
                anonymous: false,
                ordinal,
            }
        };
        let code = CodeUid::derive(file, declaration)
            .map_err(|_| invalid("invalid SCIP declaration identity"))?;
        let uid = code.to_string();
        if !parts
            .nodes
            .iter()
            .any(|node| text(node, "uid") == Some(uid.as_str()))
        {
            break code;
        }
        ordinal = ordinal
            .checked_add(1)
            .ok_or(AstGraphError::Representation)?;
    };
    let uid = code.to_string();
    let key = CodePrimaryKey::derive(code, "scip_definitions")
        .map_err(|_| invalid("invalid SCIP projection key"))?
        .to_string();
    let mut properties = BTreeMap::new();
    for (key, value) in [
        ("uid", uid.as_str()),
        ("data_key", "scip_definitions"),
        (
            "name",
            info.map(|i| i.display_name.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(name.text()),
        ),
        ("raw_name", name.text()),
        ("path", parts.association.path()),
        ("lang", text(&parts.file_observation, "lang").unwrap_or("")),
        (
            "grammar",
            text(&parts.file_observation, "grammar").unwrap_or(""),
        ),
        ("extent_status", "Exact"),
        ("name_representation", "Identifier"),
    ] {
        properties.insert(key.into(), AstGraphValue::String(value.into()));
    }
    properties.insert("available".into(), AstGraphValue::Bool(true));
    properties.insert("source_empty".into(), AstGraphValue::Bool(false));
    properties.insert(
        "source_stored".into(),
        AstGraphValue::Bool(matches!(
            parts.association.source_observation(),
            AstGraphSourceObservation::IndexedStored { .. }
        )),
    );
    properties.insert(
        "source_sha256".into(),
        AstGraphValue::String(
            parts
                .association
                .source_sha256()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        ),
    );
    properties.insert(
        "raw_version".into(),
        AstGraphValue::String(parts.association.raw_version().to_string()),
    );
    properties.insert(
        "line_number".into(),
        AstGraphValue::Int64(line(name.start_point().row)?),
    );
    for (key, value) in [
        ("physical_branch", parts.association.physical_branch()),
        ("inherited_target", parts.association.inherited_target()),
    ] {
        if let Some(value) = value {
            properties.insert(key.into(), AstGraphValue::String(value.into()));
        }
    }
    add_range(&mut properties, "name_", name)?;
    add_range(&mut properties, "", extent)?;
    Ok(AstGraphNode {
        label: label(info).into(),
        key,
        properties,
    })
}
fn exact(value: &Value) -> Option<(usize, usize)> {
    if value.get("status").and_then(Value::as_str) != Some("Exact") {
        return None;
    }
    Some((
        usize::try_from(value.pointer("/range/start_byte")?.as_u64()?).ok()?,
        usize::try_from(value.pointer("/range/end_byte")?.as_u64()?).ok()?,
    ))
}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct RelationSource {
    kind: String,
    key: String,
    query_data_key: Option<String>,
    relation_index: usize,
    source_index: Option<usize>,
    capture_index: Option<usize>,
    observation_json: String,
    target_labels: Vec<String>,
    graph_edge_uids: Vec<String>,
    parser_target: Option<String>,
}
impl RelationSource {
    fn new(kind: &str, key: &str, outcome: &AstGraphRelationOutcome, observation: &Value) -> Self {
        Self {
            kind: kind.into(),
            key: key.into(),
            query_data_key: observation
                .get("data_key")
                .or_else(|| observation.pointer("/occurrence/policy/data_key"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            relation_index: outcome.relation_index,
            source_index: outcome.source_index,
            capture_index: outcome.capture_index,
            observation_json: outcome.observation_json.clone(),
            target_labels: observation
                .pointer("/target_rule/labels")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            graph_edge_uids: observation
                .get("graph_edge_uids")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            parser_target: match &outcome.target {
                AstGraphRelationTarget::Resolved { key } => Some(key.clone()),
                _ => None,
            },
        }
    }
}
type RelationSources = BTreeMap<(usize, usize), BTreeSet<RelationSource>>;
fn relation_sources(
    parts: &AstGraphParts,
    original: &SnapshotContext,
) -> Result<RelationSources, AstGraphError> {
    let mut result: RelationSources = BTreeMap::new();
    let keys: BTreeSet<_> = parts.nodes.iter().map(|n| n.key.as_str()).collect();
    for outcome in &parts.outcomes {
        original.check()?;
        let observation: Value = serde_json::from_str(&outcome.observation_json)?;
        if let Some(captures) = observation.get("captures").and_then(Value::as_array) {
            let Some(capture_index) = outcome.capture_index else {
                continue;
            };
            let Some(range) = captures
                .iter()
                .find(|c| {
                    c.get("capture_index").and_then(Value::as_u64) == Some(capture_index as u64)
                })
                .and_then(|c| c.get("original"))
                .and_then(exact)
            else {
                continue;
            };
            let Some(kind) = observation
                .get("relation_type")
                .and_then(Value::as_str)
                .filter(|k| !k.is_empty())
            else {
                continue;
            };
            let Some(source_index) = outcome.source_index else {
                continue;
            };
            if let Some(sources) = observation
                .get("sources")
                .and_then(Value::as_array)
                .and_then(|s| s.get(source_index))
                .filter(|s| s.get("state").and_then(Value::as_str) == Some("Bound"))
                .and_then(|s| s.get("entity_keys"))
                .and_then(Value::as_array)
            {
                for key in sources
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|k| keys.contains(k))
                {
                    result.entry(range).or_default().insert(RelationSource::new(
                        kind,
                        key,
                        outcome,
                        &observation,
                    ));
                }
            }
        } else if let Some(range) = observation.get("original").and_then(exact) {
            let Some(kind) = observation
                .get("kind")
                .and_then(Value::as_str)
                .filter(|k| !k.is_empty())
            else {
                continue;
            };
            if let Some(node) = outcome.source_index.and_then(|i| parts.nodes.get(i)) {
                result.entry(range).or_default().insert(RelationSource::new(
                    kind,
                    &node.key,
                    outcome,
                    &observation,
                ));
            }
        } else if let Some(range) = observation
            .pointer("/occurrence/name/capture/original")
            .and_then(exact)
        {
            let Some(kind) = observation
                .pointer("/occurrence/policy/relation_type")
                .and_then(Value::as_str)
                .filter(|k| !k.is_empty())
            else {
                continue;
            };
            if observation
                .pointer("/oracle_call/source/state")
                .and_then(Value::as_str)
                != Some("Bound")
            {
                continue;
            }
            if let Some(node) = observation
                .pointer("/oracle_call/source/entity_index")
                .and_then(Value::as_u64)
                .and_then(|i| usize::try_from(i).ok())
                .and_then(|i| parts.nodes.get(i))
            {
                result.entry(range).or_default().insert(RelationSource::new(
                    kind,
                    &node.key,
                    outcome,
                    &observation,
                ));
            }
        }
    }
    Ok(result)
}

/// Admit a compiler-indexed Source that has no configured parser grammar. The actual text, path,
/// version and branch come from the retained Raw or physical capture. A SCIP document is an
/// observation to match to that Source, never filesystem authority or a fabricated parser model.
pub fn assemble_scip_source(
    document: &Document,
    source: NativeNnSource<'_>,
    limits: crate::symbols::IdentityLimits,
    original: &Arc<SnapshotContext>,
) -> Result<AstSemanticGraphBatch, AstGraphError> {
    original.check()?;
    let result = (|| {
        let raw = match &source {
            NativeNnSource::Indexed(raw) | NativeNnSource::Physical { raw, .. } => *raw,
        };
        let text = match &source {
            NativeNnSource::Indexed(raw) => {
                match raw.source(original).map_err(NativeNnError::Source)? {
                    crate::source_inspection::IndexedSource::Available(text) => text,
                    crate::source_inspection::IndexedSource::Missing => {
                        return Err(NativeNnError::InvalidInput.into());
                    }
                }
            }
            NativeNnSource::Physical { capture, .. } => capture
                .source(original)
                .map_err(|error| NativeNnError::PhysicalSource(Box::new(error)))?,
        };
        if document.relative_path != raw.path()
            || (!document.text.is_empty() && document.text != text)
        {
            return Err(invalid(
                "compiler SCIP document differs from captured Source",
            ));
        }
        let components: Vec<_> = raw.path().split('/').collect();
        let identity = crate::symbols::IdentityDocument::new(
            crate::symbols::IdentityDocumentInput {
                relative_components: &components,
                original_bytes: text.as_bytes(),
                language: &document.language,
                grammar: "scip-compiler",
            },
            limits,
            original,
        )
        .map_err(io::Error::other)?;
        let (path, source_sha256) = associated_source(&source, &identity, text, original)?;
        let empty = text.is_empty();
        let source_observation = match &source {
            NativeNnSource::Indexed(_) => AstGraphSourceObservation::IndexedStored { empty },
            NativeNnSource::Physical { .. } => {
                AstGraphSourceObservation::PhysicalTextNotStored { empty }
            }
        };
        let branch = raw.branch_observation();
        let association = AstGraphAssociation {
            path,
            relative_components: components.iter().map(|s| (*s).to_owned()).collect(),
            raw_version: raw.version(),
            source_sha256,
            source_observation,
            physical_branch: branch.and_then(|b| b.physical_branch).map(str::to_owned),
            inherited_target: branch.and_then(|b| b.inherited_target).map(str::to_owned),
        };
        let diagnostics_json = "[]".to_owned();
        let capabilities_json =
            json!([{"producer":"scip-compiler","index_format":"scip"}]).to_string();
        let mut file_observation = file_observation_node(
            &association,
            identity.language(),
            identity.grammar(),
            &[],
            &diagnostics_json,
            &capabilities_json,
            original,
        )?;
        file_observation
            .properties
            .insert("index_format".into(), AstGraphValue::String("scip".into()));
        Ok(AstSemanticGraphBatch {
            file_observation,
            association,
            nodes: Vec::new(),
            edges: Vec::new(),
            outcomes: Vec::new(),
            diagnostics_json,
            capabilities_json,
        })
    })();
    original.check()?;
    result
}

/// The guarded caller must still recheck captured inputs before publishing. This operation validates
/// each occurrence against the exact bytes already associated with `parts`; it grants no path access.
pub fn apply_scip_document(
    mut parts: AstGraphParts,
    document: &Document,
    source: &str,
    origin: AstScipDocumentOrigin,
    original: &Arc<SnapshotContext>,
) -> Result<AstScipGraphDocument, AstGraphError> {
    original.check()?;
    let result = (|| {
        if document.relative_path != parts.association.path()
            || <[u8; 32]>::from(Sha256::digest(source.as_bytes()))
                != *parts.association.source_sha256()
            || (!document.text.is_empty() && document.text != source)
        {
            return Err(invalid("SCIP document differs from associated Source"));
        }
        let source = SourceText::new(source.as_bytes(), source.len().max(1))
            .map_err(|_| invalid("invalid associated Source"))?;
        let lines = source.line_starts();
        let mut names: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
        for (index, node) in parts.nodes.iter().enumerate() {
            if let (Some(start), Some(end)) =
                (byte(node, "name_start_byte"), byte(node, "name_end_byte"))
            {
                names.entry((start, end)).or_default().push(index);
            }
        }
        let information: BTreeMap<_, _> = document
            .symbols
            .iter()
            .map(|i| (i.symbol.as_str(), i))
            .collect();
        let relations = relation_sources(&parts, original)?;
        let mut definitions = Vec::new();
        let mut references = Vec::new();
        let edge_index: BTreeMap<_, _> = parts
            .edges
            .iter()
            .map(|edge| (edge.uid.as_str(), edge))
            .collect();
        for occurrence in &document.occurrences {
            original.check()?;
            let range = span(
                occurrence,
                document.position_encoding.value(),
                &source,
                lines,
            )?;
            let enclosing = enclosing(
                occurrence,
                document.position_encoding.value(),
                &source,
                lines,
            )?;
            if enclosing.is_some_and(|r| r.start() > range.start() || r.end() < range.end()) {
                return Err(invalid("SCIP enclosing range does not contain occurrence"));
            }
            if occurrence.symbol.is_empty() {
                continue;
            }
            symbol::parse_symbol(&occurrence.symbol).map_err(|_| invalid("invalid SCIP symbol"))?;
            if occurrence.symbol_roles & types::SymbolRole::Definition as i32 != 0 {
                let info = information.get(occurrence.symbol.as_str()).copied();
                let indices = if let Some(indices) = names.get(&(range.start(), range.end())) {
                    indices.clone()
                } else {
                    if origin == AstScipDocumentOrigin::Syntax {
                        return Err(invalid("syntactic SCIP definition has no native entity"));
                    }
                    let node = new_definition(
                        &parts,
                        occurrence,
                        info,
                        range,
                        enclosing.unwrap_or(range),
                        original,
                    )?;
                    let index = parts.nodes.len();
                    parts.nodes.push(node);
                    names.insert((range.start(), range.end()), vec![index]);
                    vec![index]
                };
                for index in indices {
                    let node = &mut parts.nodes[index];
                    enrich(node, occurrence, info)?;
                    definitions.push(AstScipDocumentDefinition {
                        association: parts.association.clone(),
                        path: document.relative_path.clone(),
                        symbol: occurrence.symbol.clone(),
                        start_byte: range.start() as u64,
                        end_byte: range.end() as u64,
                        symbol_roles: occurrence.symbol_roles,
                        entity_key: node.key.clone(),
                        label: node.label.clone(),
                    });
                }
            } else {
                let sources = relations.get(&(range.start(), range.end()));
                if sources.is_none_or(BTreeSet::is_empty) {
                    references.push(AstScipDocumentReference {
                        association: parts.association.clone(),
                        path: document.relative_path.clone(),
                        symbol: occurrence.symbol.clone(),
                        start_byte: range.start() as u64,
                        end_byte: range.end() as u64,
                        symbol_roles: occurrence.symbol_roles,
                        kind: None,
                        source_key: None,
                        query_data_key: None,
                        reference_target: if origin == AstScipDocumentOrigin::Syntax
                            || symbol::is_local_symbol(&occurrence.symbol)
                        {
                            range.text().to_owned()
                        } else {
                            occurrence.symbol.clone()
                        },
                        occurrence_ordinal: None,
                        relation_index: None,
                        source_index: None,
                        capture_index: None,
                        observation_json: None,
                        target_labels: Vec::new(),
                        graph_edge_uids: Vec::new(),
                        parser_target: None,
                    });
                } else {
                    for source in sources.into_iter().flatten() {
                        let graph_edge_uids: Vec<_> = source
                            .graph_edge_uids
                            .iter()
                            .filter(|uid| {
                                edge_index.get(uid.as_str()).is_some_and(|edge| {
                                    edge.kind == source.kind && edge.source_key == source.key
                                })
                            })
                            .cloned()
                            .collect();
                        references.push(AstScipDocumentReference {
                            association: parts.association.clone(),
                            path: document.relative_path.clone(),
                            symbol: occurrence.symbol.clone(),
                            start_byte: range.start() as u64,
                            end_byte: range.end() as u64,
                            symbol_roles: occurrence.symbol_roles,
                            kind: Some(source.kind.clone()),
                            source_key: Some(source.key.clone()),
                            query_data_key: source.query_data_key.clone(),
                            reference_target: if origin == AstScipDocumentOrigin::Syntax
                                || symbol::is_local_symbol(&occurrence.symbol)
                            {
                                range.text().to_owned()
                            } else {
                                occurrence.symbol.clone()
                            },
                            occurrence_ordinal: None,
                            relation_index: Some(source.relation_index),
                            source_index: source.source_index,
                            capture_index: source.capture_index,
                            observation_json: Some(source.observation_json.clone()),
                            target_labels: source.target_labels.clone(),
                            graph_edge_uids,
                            parser_target: source.parser_target.clone(),
                        });
                    }
                }
            }
        }
        drop(edge_index);
        let identity_changes = match origin {
            AstScipDocumentOrigin::Compiler => {
                promote_scip_identities(&mut parts, &mut definitions, &mut references, original)?
            }
            AstScipDocumentOrigin::Syntax => Vec::new(),
        };
        // Promote the proven parser-edge provenance before removing replaced rows.
        // Reference/outcome UIDs then describe the same final endpoints as the batch.
        let replaced_edges = references
            .iter()
            .flat_map(|reference| reference.graph_edge_uids.iter().cloned())
            .collect::<BTreeSet<_>>();
        parts
            .edges
            .retain(|edge| !replaced_edges.contains(&edge.uid));
        let mut occurrences = LogicalRelationOccurrences::new(logical_file(&parts.association)?);
        for reference in &mut references {
            original.check()?;
            if let (Some(source), Some(kind)) = (&reference.source_key, &reference.kind) {
                let start = usize::try_from(reference.start_byte)
                    .map_err(|_| AstGraphError::Representation)?;
                let end = usize::try_from(reference.end_byte)
                    .map_err(|_| AstGraphError::Representation)?;
                reference.occurrence_ordinal = Some(occurrences.ordinal(
                    source,
                    kind,
                    &ParserRelationOccurrence {
                        query_data_key: reference.query_data_key.as_deref(),
                        target: &reference.reference_target,
                        anchor: (start, end),
                    },
                )?);
            }
        }
        parts
            .file_observation
            .properties
            .insert("scip_document".into(), AstGraphValue::Bool(true));
        Ok(AstScipGraphDocument {
            parts,
            definitions,
            references,
            identity_changes,
            origin,
        })
    })();
    original.check()?;
    result
}

fn promote_scip_identities(
    parts: &mut AstGraphParts,
    definitions: &mut [AstScipDocumentDefinition],
    references: &mut [AstScipDocumentReference],
    original: &SnapshotContext,
) -> Result<Vec<AstGraphIdentityChange>, AstGraphError> {
    let file = logical_file(&parts.association)?;
    let mut symbols = BTreeMap::<String, BTreeSet<String>>::new();
    for definition in definitions.iter() {
        original.check()?;
        if !symbol::is_local_symbol(&definition.symbol) {
            symbols
                .entry(definition.entity_key.clone())
                .or_default()
                .insert(definition.symbol.clone());
        }
    }
    // Previous logical UID binds projections of the same declaration. Coordinates
    // only order/bind current observations; neither is part of the global seed.
    let mut groups = BTreeMap::<CodeUid, BTreeMap<(u64, u64, String), u64>>::new();
    let mut seeds = Vec::new();
    for (index, node) in parts.nodes.iter().enumerate() {
        original.check()?;
        let Some(symbols) = symbols.get(&node.key) else {
            continue;
        };
        let symbols = symbols.iter().cloned().collect::<Vec<_>>();
        let symbol_refs = symbols.iter().map(String::as_str).collect::<Vec<_>>();
        let base = CodeUid::derive(
            file,
            CodeDeclaration::Scip {
                symbols: &symbol_refs,
                kind: &node.label,
                ordinal: 0,
            },
        )
        .map_err(|_| invalid("invalid SCIP declaration set"))?;
        let start = byte(node, "name_start_byte")
            .ok_or_else(|| invalid("SCIP declaration lacks name anchor"))?;
        let end = byte(node, "name_end_byte")
            .ok_or_else(|| invalid("SCIP declaration lacks name anchor"))?;
        let previous_uid =
            text(node, "uid").ok_or_else(|| invalid("SCIP declaration lacks logical UID"))?;
        let observation = (
            u64::try_from(start).map_err(|_| AstGraphError::Representation)?,
            u64::try_from(end).map_err(|_| AstGraphError::Representation)?,
            previous_uid.to_owned(),
        );
        groups
            .entry(base)
            .or_default()
            .insert(observation.clone(), 0);
        seeds.push((index, symbols, base, observation));
    }
    for observations in groups.values_mut() {
        for (ordinal, value) in observations.values_mut().enumerate() {
            original.check()?;
            *value = u64::try_from(ordinal).map_err(|_| AstGraphError::Representation)?;
        }
    }
    let mut changes = Vec::new();
    let mut keys = BTreeMap::new();
    for (index, symbols, group, observation) in seeds {
        original.check()?;
        let node = &mut parts.nodes[index];
        let symbols = symbols.iter().map(String::as_str).collect::<Vec<_>>();
        let ordinal = groups[&group][&observation];
        let uid = CodeUid::derive(
            file,
            CodeDeclaration::Scip {
                symbols: &symbols,
                kind: &node.label,
                ordinal,
            },
        )
        .map_err(|_| invalid("invalid SCIP declaration identity"))?;
        let data_key = text(node, "data_key")
            .ok_or_else(|| invalid("SCIP declaration lacks data key"))?
            .to_owned();
        let key = CodePrimaryKey::derive(uid, &data_key)
            .map_err(|_| invalid("invalid SCIP projection identity"))?
            .to_string();
        let previous_key = std::mem::replace(&mut node.key, key.clone());
        let uid = uid.to_string();
        node.properties
            .insert("uid".into(), AstGraphValue::String(uid.clone()));
        if previous_key != key {
            keys.insert(previous_key.clone(), key.clone());
            changes.push(AstGraphIdentityChange {
                label: node.label.clone(),
                data_key,
                start_byte: observation.0,
                end_byte: observation.1,
                previous_uid: observation.2,
                uid,
                previous_key,
                key,
            });
        }
    }
    if keys.is_empty() {
        return Ok(changes);
    }
    let mut unique = BTreeSet::new();
    for node in &parts.nodes {
        original.check()?;
        if !unique.insert((&node.label, &node.key)) {
            return Err(invalid("SCIP promotion aliases distinct projections"));
        }
    }
    let mut edge_uids = BTreeMap::<String, Vec<String>>::new();
    for edge in &mut parts.edges {
        original.check()?;
        if !keys.contains_key(&edge.source_key) && !keys.contains_key(&edge.target_key) {
            continue;
        }
        let old_uid = edge.uid.clone();
        let old_target = edge.target_key.clone();
        remap_key(&mut edge.source_key, &keys);
        remap_key(&mut edge.target_key, &keys);
        if edge.kind == "CONTAINS"
            && edge.query_data_key.is_none()
            && edge.reference_target == old_target
        {
            edge.reference_target.clone_from(&edge.target_key);
        }
        *edge = AstGraphEdge::logical(
            file,
            &edge.kind,
            &edge.source_key,
            &edge.target_key,
            edge.query_data_key.as_deref(),
            &edge.reference_target,
            edge.occurrence_ordinal,
        )?;
        let mapped = edge_uids.entry(old_uid).or_default();
        if !mapped.contains(&edge.uid) {
            mapped.push(edge.uid.clone());
        }
    }
    for outcome in &mut parts.outcomes {
        original.check()?;
        match &mut outcome.target {
            AstGraphRelationTarget::Resolved { key } => remap_key(key, &keys),
            AstGraphRelationTarget::Ambiguous { keys: targets } => {
                for target in targets {
                    remap_key(target, &keys);
                }
            }
            _ => {}
        }
        let mut observation: Value = serde_json::from_str(&outcome.observation_json)?;
        remap_observation(&mut observation, &keys, &edge_uids);
        outcome.observation_json = serde_json::to_string(&observation)?;
    }
    for definition in definitions {
        original.check()?;
        remap_key(&mut definition.entity_key, &keys);
    }
    for reference in references {
        original.check()?;
        if let Some(key) = &mut reference.source_key {
            remap_key(key, &keys);
        }
        if let Some(key) = &mut reference.parser_target {
            remap_key(key, &keys);
        }
        remap_edge_uids(&mut reference.graph_edge_uids, &edge_uids);
        if let Some(encoded) = &mut reference.observation_json {
            let mut observation: Value = serde_json::from_str(encoded)?;
            remap_observation(&mut observation, &keys, &edge_uids);
            *encoded = serde_json::to_string(&observation)?;
        }
    }
    let language =
        text(&parts.file_observation, "lang").ok_or_else(|| invalid("SCIP FILE lacks language"))?;
    let grammar = text(&parts.file_observation, "grammar")
        .ok_or_else(|| invalid("SCIP FILE lacks grammar"))?;
    let mut observation = file_observation_node(
        &parts.association,
        language,
        grammar,
        &parts.outcomes,
        &parts.diagnostics_json,
        &parts.capabilities_json,
        original,
    )?;
    let outcomes = observation
        .properties
        .remove("resolution_outcomes_json")
        .ok_or_else(|| invalid("SCIP FILE lacks resolution outcomes"))?;
    parts
        .file_observation
        .properties
        .insert("resolution_outcomes_json".into(), outcomes);
    Ok(changes)
}

fn remap_key(key: &mut String, keys: &BTreeMap<String, String>) {
    if let Some(replacement) = keys.get(key) {
        key.clone_from(replacement);
    }
}
fn remap_edge_uids(uids: &mut Vec<String>, mappings: &BTreeMap<String, Vec<String>>) {
    let previous = std::mem::take(uids);
    for uid in previous {
        if let Some(replacements) = mappings.get(&uid) {
            uids.extend(replacements.iter().cloned());
        } else {
            uids.push(uid);
        }
    }
}
fn remap_observation(
    observation: &mut Value,
    keys: &BTreeMap<String, String>,
    edges: &BTreeMap<String, Vec<String>>,
) {
    if let Some(sources) = observation.get_mut("sources").and_then(Value::as_array_mut) {
        for source in sources {
            if let Some(source_keys) = source.get_mut("entity_keys").and_then(Value::as_array_mut) {
                for key in source_keys {
                    if let Some(replacement) = key.as_str().and_then(|key| keys.get(key)) {
                        *key = replacement.clone().into();
                    }
                }
            }
        }
    }
    if let Some(uids) = observation
        .get_mut("graph_edge_uids")
        .and_then(Value::as_array_mut)
    {
        let previous = std::mem::take(uids);
        for uid in previous {
            if let Some(replacements) = uid.as_str().and_then(|uid| edges.get(uid)) {
                uids.extend(replacements.iter().cloned().map(Value::String));
            } else {
                uids.push(uid);
            }
        }
    }
    if let Some(key) = observation.get_mut("embedded_host_key")
        && let Some(replacement) = key.as_str().and_then(|key| keys.get(key))
    {
        *key = replacement.clone().into();
    }
    if let Some(target) = observation.get_mut("oracle_target") {
        if let Some(key) = target.get_mut("source_key")
            && let Some(replacement) = key.as_str().and_then(|key| keys.get(key))
        {
            *key = replacement.clone().into();
        }
        if let Some(candidates) = target.get_mut("candidates").and_then(Value::as_array_mut) {
            for candidate in candidates {
                if let Some(key) = candidate.get_mut("key")
                    && let Some(replacement) = key.as_str().and_then(|key| keys.get(key))
                {
                    *key = replacement.clone().into();
                }
            }
        }
    }
    if let Some(native) = observation.get_mut("native") {
        remap_observation(native, keys, edges);
    }
}

/// Keep deferred FILE observations causal with the canonical symbol join. All changes stay local
/// until the complete batch has been checked, so a malformed later reference cannot partly update
/// the file. The caller supplies candidates from its private same-job staging, not arbitrary paths.
pub fn apply_scip_document_resolutions(
    file: &mut AstGraphNode,
    resolutions: &[(AstScipDocumentReference, Vec<AstScipDocumentDefinition>)],
    original: &Arc<SnapshotContext>,
) -> Result<(), AstGraphError> {
    original.check()?;
    let result = (|| {
        if file.properties.get("is_file_observation") != Some(&AstGraphValue::Bool(true)) {
            return Err(invalid("SCIP resolution requires a FILE observation"));
        }
        let encoded = text(file, "resolution_outcomes_json")
            .ok_or_else(|| invalid("SCIP FILE lacks outcomes"))?;
        let mut outcomes: Vec<Value> = serde_json::from_str(encoded)?;
        let mut indices = BTreeMap::new();
        for (index, outcome) in outcomes.iter().enumerate() {
            original.check()?;
            let relation = outcome
                .get("relation_index")
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("FILE outcome lacks relation index"))?;
            let source = outcome.get("source_index").and_then(Value::as_u64);
            let capture = outcome.get("capture_index").and_then(Value::as_u64);
            if indices.insert((relation, source, capture), index).is_some() {
                return Err(invalid("SCIP FILE outcome is not unique"));
            }
        }
        let mut resolved: BTreeMap<usize, BTreeMap<&str, &AstScipDocumentDefinition>> =
            BTreeMap::new();
        for (reference, targets) in resolutions {
            original.check()?;
            let Some(relation) = reference.relation_index else {
                if reference.kind.is_some() || reference.source_key.is_some() {
                    return Err(invalid("SCIP relation lacks original outcome"));
                }
                continue;
            };
            let digest: String = reference
                .association
                .source_sha256()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            if reference.path != reference.association.path()
                || text(file, "path") != Some(reference.path.as_str())
                || text(file, "raw_version")
                    != Some(reference.association.raw_version().to_string().as_str())
                || text(file, "source_sha256") != Some(digest.as_str())
            {
                return Err(invalid(
                    "SCIP resolution references another FILE association",
                ));
            }
            if reference.kind.as_deref().is_none_or(str::is_empty)
                || reference.source_key.as_deref().is_none_or(str::is_empty)
                || reference.start_byte >= reference.end_byte
            {
                return Err(invalid("SCIP relation lacks genuine source or range"));
            }
            let key = (
                u64::try_from(relation).map_err(|_| AstGraphError::Representation)?,
                reference
                    .source_index
                    .map(u64::try_from)
                    .transpose()
                    .map_err(|_| AstGraphError::Representation)?,
                reference
                    .capture_index
                    .map(u64::try_from)
                    .transpose()
                    .map_err(|_| AstGraphError::Representation)?,
            );
            let index = *indices
                .get(&key)
                .ok_or_else(|| invalid("SCIP FILE outcome is absent"))?;
            let observation: Value = serde_json::from_str(
                reference
                    .observation_json
                    .as_deref()
                    .ok_or_else(|| invalid("SCIP relation lacks observation"))?,
            )?;
            // A previous resolution adds only this derived field. The original parser
            // observation remains the proof when another FILE changes its definition.
            let mut observed = outcomes[index].get("observation").cloned();
            if let Some(fields) = observed.as_mut().and_then(Value::as_object_mut) {
                fields.remove("scip_graph_target");
            }
            if observed.as_ref() != Some(&observation) {
                return Err(invalid(
                    "SCIP resolution differs from original FILE observation",
                ));
            }
            let candidates = resolved.entry(index).or_default();
            for target in targets {
                original.check()?;
                if target.symbol != reference.symbol
                    || target.path != target.association.path()
                    || target.entity_key.is_empty()
                    || target.start_byte >= target.end_byte
                    || (symbol::is_local_symbol(&reference.symbol) && target.path != reference.path)
                {
                    return Err(invalid(
                        "SCIP resolution target differs from observed symbol",
                    ));
                }
                if !reference.target_labels.contains(&target.label) {
                    continue;
                }
                if let Some(previous) = candidates.insert(target.entity_key.as_str(), target)
                    && previous != target
                {
                    return Err(invalid("SCIP resolution has conflicting target identities"));
                }
            }
        }
        for (index, targets) in resolved {
            original.check()?;
            match targets.len() {
                0 => {
                    if matches!(
                        outcomes[index]["target"]["state"].as_str(),
                        Some("Resolved" | "Ambiguous")
                    ) {
                        outcomes[index]["target"] =
                            json!({"state":"Unresolved","reason":"ScipUnboundDefinition"});
                        if let Some(observation) = outcomes[index]["observation"].as_object_mut() {
                            observation.remove("scip_graph_target");
                        }
                    }
                }
                1 => {
                    let target = targets
                        .values()
                        .next()
                        .ok_or_else(|| invalid("SCIP target disappeared"))?;
                    outcomes[index]["target"] = json!({"state":"Resolved","key":target.entity_key});
                    outcomes[index]["observation"]["scip_graph_target"] = json!({"key":target.entity_key,"label":target.label,
                        "association":{"relative_path":target.association.path(),"raw_version":target.association.raw_version(),"source_sha256":target.association.source_sha256()},
                        "symbol":target.symbol,"name_start":target.start_byte,"name_end":target.end_byte});
                }
                _ => {
                    outcomes[index]["target"] =
                        json!({"state":"Ambiguous","keys":targets.keys().collect::<Vec<_>>()});
                    // An ambiguous compiler result must not retain an earlier single-target claim.
                    if let Some(observation) = outcomes[index]["observation"].as_object_mut() {
                        observation.remove("scip_graph_target");
                    }
                }
            }
        }
        Ok(serde_json::to_string(&outcomes)?)
    })();
    original.check()?;
    file.properties.insert(
        "resolution_outcomes_json".into(),
        AstGraphValue::String(result?),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use protobuf::MessageField;
    const SOURCE: &str = "😀 fn foo() { foo(); }\n";
    const SYMBOL: &str = "scip-rust cargo demo 1.0.0 foo().";

    fn original() -> Arc<SnapshotContext> {
        Arc::new(SnapshotContext::service(Cancellation::default()))
    }
    fn parts() -> AstGraphParts {
        let association = AstGraphAssociation {
            source_observation: AstGraphSourceObservation::IndexedStored { empty: false },
            path: "src/main.rs".into(),
            relative_components: vec!["src".into(), "main.rs".into()],
            raw_version: 7,
            source_sha256: Sha256::digest(SOURCE.as_bytes()).into(),
            physical_branch: Some("main".into()),
            inherited_target: None,
        };
        let source = SourceText::new(SOURCE.as_bytes(), SOURCE.len()).unwrap();
        let mut properties = BTreeMap::new();
        properties.insert("name".into(), AstGraphValue::String("foo".into()));
        properties.insert("uid".into(), AstGraphValue::String("syntax-foo".into()));
        properties.insert("data_key".into(), AstGraphValue::String("functions".into()));
        properties.insert(
            "metadata_json".into(),
            AstGraphValue::String("{\"complexity\":2}".into()),
        );
        properties.insert("available".into(), AstGraphValue::Bool(true));
        add_range(&mut properties, "name_", source.range(8, 11).unwrap()).unwrap();
        add_range(&mut properties, "", source.range(5, 24).unwrap()).unwrap();
        let node = AstGraphNode {
            label: "Function".into(),
            key: "stable-nn-key".into(),
            properties,
        };
        let origin = json!({"status":"Exact","range":{"start_byte":16,"end_byte":19}});
        AstGraphParts {
            file_observation: AstGraphNode {
                label: "File".into(),
                key: "file".into(),
                properties: BTreeMap::from([
                    ("lang".into(), AstGraphValue::String("rust".into())),
                    (
                        "grammar".into(),
                        AstGraphValue::String("tree-sitter-rust".into()),
                    ),
                ]),
            },
            association,
            nodes: vec![node],
            edges: Vec::new(),
            outcomes: vec![AstGraphRelationOutcome {
                relation_index: 0,
                source_index: Some(0),
                capture_index: Some(0),
                evidence_node_indices: vec![],
                target: AstGraphRelationTarget::NotFound,
                observation_json:
                    json!({"kind":"CALLS","original":origin,"target_rule":{"labels":["Function"]}})
                        .to_string(),
            }],
            diagnostics_json: "[]".into(),
            capabilities_json: "[]".into(),
        }
    }
    fn document() -> Document {
        Document {
            relative_path: "src/main.rs".into(),
            language: "rust".into(),
            position_encoding: types::PositionEncoding::UTF16CodeUnitOffsetFromLineStart.into(),
            occurrences: vec![
                Occurrence {
                    symbol: SYMBOL.into(),
                    symbol_roles: types::SymbolRole::Definition as i32,
                    range: vec![0, 6, 9],
                    enclosing_range: vec![0, 3, 22],
                    ..Default::default()
                },
                Occurrence {
                    symbol: SYMBOL.into(),
                    range: vec![0, 14, 17],
                    ..Default::default()
                },
            ],
            symbols: vec![SymbolInformation {
                symbol: SYMBOL.into(),
                documentation: vec!["Function documentation".into()],
                kind: Kind::Function.into(),
                signature_documentation: MessageField::some(types::Signature {
                    language: "rust".into(),
                    text: "fn foo()".into(),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        }
    }
    #[test]
    fn promotes_parser_identity_and_preserves_its_entities_metadata_and_bindings() {
        let before = parts();
        let metadata = before.nodes[0].properties["metadata_json"].clone();
        let result = apply_scip_document(
            before,
            &document(),
            SOURCE,
            crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
            &original(),
        )
        .unwrap();
        assert_eq!(result.parts.nodes.len(), 1);
        assert_ne!(result.parts.nodes[0].key, "stable-nn-key");
        assert_eq!(result.identity_changes.len(), 1);
        let change = &result.identity_changes[0];
        assert_eq!(change.previous_key, "stable-nn-key");
        assert_eq!(change.previous_uid, "syntax-foo");
        assert_eq!(change.data_key, "functions");
        assert_eq!((change.start_byte, change.end_byte), (8, 11));
        assert_eq!(change.key, result.parts.nodes[0].key);
        assert_eq!(
            Some(change.uid.as_str()),
            text(&result.parts.nodes[0], "uid")
        );
        assert_eq!(result.definitions[0].entity_key, change.key);
        assert_eq!(result.parts.nodes[0].properties["metadata_json"], metadata);
        let AstGraphValue::String(value) = &result.parts.nodes[0].properties["scip_symbols_json"]
        else {
            panic!()
        };
        let value: Value = serde_json::from_str(value).unwrap();
        assert_eq!(
            value[0]["information"]["documentation"][0],
            "Function documentation"
        );
        assert_eq!(value[0]["information"]["signature"]["text"], "fn foo()");
        assert_eq!(
            (
                result.definitions[0].start_byte,
                result.definitions[0].end_byte
            ),
            (8, 11)
        );
        assert_eq!(
            (
                result.references[0].start_byte,
                result.references[0].end_byte
            ),
            (16, 19)
        );
        assert_eq!(result.references[0].kind.as_deref(), Some("CALLS"));
        assert_eq!(
            result.references[0].source_key.as_deref(),
            Some(change.key.as_str())
        );
        assert_eq!(result.parts.outcomes.len(), 1);
        assert!(
            result.parts.edges.is_empty(),
            "cross-file symbol join owns edge materialization"
        );
    }
    #[test]
    fn global_symbol_sets_promote_both_views_and_preserve_parallel_edge_rows() {
        let input = || {
            let mut input = parts();
            let mut view = input.nodes[0].clone();
            view.key = "second-view".into();
            view.properties
                .insert("data_key".into(), AstGraphValue::String("alternate".into()));
            input.nodes.push(view);
            let edge = AstGraphEdge::logical(
                logical_file(&input.association).unwrap(),
                "CONTAINS",
                "stable-nn-key",
                "second-view",
                None,
                "second-view",
                0,
            )
            .unwrap();
            input.edges = vec![edge.clone(), edge];
            input
        };
        let mut doc = document();
        let mut alias = doc.occurrences[0].clone();
        alias.symbol = "scip-rust cargo demo 1.0.0 foo_alias().".into();
        doc.occurrences.push(alias);
        let result = apply_scip_document(
            input(),
            &doc,
            SOURCE,
            crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
            &original(),
        )
        .unwrap();
        assert_eq!(result.parts.nodes.len(), 2);
        assert_eq!(result.identity_changes.len(), 2);
        assert_eq!(
            text(&result.parts.nodes[0], "uid"),
            text(&result.parts.nodes[1], "uid")
        );
        assert_ne!(result.parts.nodes[0].key, result.parts.nodes[1].key);
        assert_eq!(result.parts.edges.len(), 2);
        assert_eq!(result.parts.edges[0], result.parts.edges[1]);
        assert_eq!(result.parts.edges[0].source_key, result.parts.nodes[0].key);
        assert_eq!(result.parts.edges[0].target_key, result.parts.nodes[1].key);
        assert_eq!(result.definitions.len(), 4);
        let facts: Value =
            serde_json::from_str(text(&result.parts.nodes[0], "scip_symbols_json").unwrap())
                .unwrap();
        assert_eq!(facts.as_array().unwrap().len(), 2);
        doc.occurrences.reverse();
        let reordered = apply_scip_document(
            input(),
            &doc,
            SOURCE,
            crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
            &original(),
        )
        .unwrap();
        assert_eq!(result.parts.nodes[0].key, reordered.parts.nodes[0].key);
        assert_eq!(result.parts.nodes[1].key, reordered.parts.nodes[1].key);
        let single = apply_scip_document(
            input(),
            &document(),
            SOURCE,
            crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
            &original(),
        )
        .unwrap();
        assert_ne!(result.parts.nodes[0].key, single.parts.nodes[0].key);
        let encoded = serde_json::to_vec(&result.parts.edges).unwrap();
        let restored: Vec<AstGraphEdge> = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(restored, result.parts.edges);
    }

    #[test]
    fn promoted_scip_uid_survives_unrelated_prefix_and_raw_version_change() {
        let before = apply_scip_document(
            parts(),
            &document(),
            SOURCE,
            crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
            &original(),
        )
        .unwrap();
        let source = format!("\n{SOURCE}");
        let text_source = SourceText::new(source.as_bytes(), source.len()).unwrap();
        let mut input = parts();
        input.association.raw_version += 1;
        input.association.source_sha256 = Sha256::digest(source.as_bytes()).into();
        add_range(
            &mut input.nodes[0].properties,
            "name_",
            text_source.range(9, 12).unwrap(),
        )
        .unwrap();
        add_range(
            &mut input.nodes[0].properties,
            "",
            text_source.range(6, 25).unwrap(),
        )
        .unwrap();
        let mut observation: Value =
            serde_json::from_str(&input.outcomes[0].observation_json).unwrap();
        observation["original"]["range"] = json!({"start_byte":17,"end_byte":20});
        input.outcomes[0].observation_json = observation.to_string();
        let mut doc = document();
        for occurrence in &mut doc.occurrences {
            occurrence.range[0] += 1;
            if !occurrence.enclosing_range.is_empty() {
                occurrence.enclosing_range[0] += 1;
            }
        }
        let after = apply_scip_document(
            input,
            &doc,
            &source,
            crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
            &original(),
        )
        .unwrap();
        assert_eq!(before.parts.nodes[0].key, after.parts.nodes[0].key);
        assert_eq!(
            before.references[0].source_key,
            after.references[0].source_key
        );
        assert_eq!(
            before.references[0].occurrence_ordinal,
            after.references[0].occurrence_ordinal
        );
        assert_eq!(
            scip_edge_uid(&before.references[0], &before.definitions[0]).unwrap(),
            scip_edge_uid(&after.references[0], &after.definitions[0]).unwrap()
        );
        assert_ne!(
            before.parts.association.source_sha256(),
            after.parts.association.source_sha256()
        );
    }

    #[test]
    fn compiler_only_definition_is_source_bound_and_queryable() {
        let mut before = parts();
        before.nodes.clear();
        before.outcomes.clear();
        let original = original();
        let result = apply_scip_document(
            before,
            &document(),
            SOURCE,
            crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
            &original,
        )
        .unwrap();
        assert_eq!(result.parts.nodes.len(), 1);
        let node = &result.parts.nodes[0];
        assert_eq!(node.label, "Function");
        assert_eq!(text(node, "name"), Some("foo"));
        assert_eq!(text(node, "path"), Some("src/main.rs"));
        assert_eq!(byte(node, "start_byte"), Some(5));
        assert_eq!(byte(node, "end_byte"), Some(24));
        assert_ne!(node.key, "stable-nn-key");
        assert_eq!(result.definitions[0].entity_key, node.key);
        assert_eq!(result.references[0].kind, None);
        assert_eq!(result.references[0].source_key, None);
        assert!(
            !node.properties.contains_key("metadata_json"),
            "compiler data is not parser contribution metadata"
        );
        let AstGraphValue::String(symbols) = &node.properties["scip_symbols_json"] else {
            panic!("official symbol metadata")
        };
        let symbols: Value = serde_json::from_str(symbols).unwrap();
        assert_eq!(
            symbols[0]["information"]["documentation"][0],
            "Function documentation"
        );
        assert_eq!(symbols[0]["information"]["signature"]["text"], "fn foo()");
        let searchable =
            crate::entity_search::encode_file(&result.parts, SOURCE, &original).unwrap();
        assert_eq!(searchable.num_rows(), 1);
        let content = searchable
            .column_by_name("content")
            .unwrap()
            .as_any()
            .downcast_ref::<arrow_array::StringArray>()
            .unwrap();
        assert_eq!(content.value(0), "fn foo() { foo(); }");
    }
    #[test]
    fn rejects_foreign_path_text_and_source_hash() {
        let mut doc = document();
        doc.relative_path = "other.rs".into();
        assert!(
            apply_scip_document(
                parts(),
                &doc,
                SOURCE,
                crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
                &original()
            )
            .is_err()
        );
        doc = document();
        doc.text = "wrong text".into();
        assert!(
            apply_scip_document(
                parts(),
                &doc,
                SOURCE,
                crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
                &original()
            )
            .is_err()
        );
        assert!(
            apply_scip_document(
                parts(),
                &document(),
                "other source",
                crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
                &original()
            )
            .is_err()
        );
    }
    #[test]
    fn rejects_split_surrogate_and_disagreeing_or_uncontained_ranges() {
        let mut doc = document();
        doc.occurrences[0].range = vec![0, 1, 2];
        assert!(
            apply_scip_document(
                parts(),
                &doc,
                SOURCE,
                crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
                &original()
            )
            .is_err()
        );
        doc = document();
        doc.occurrences[0].typed_range = Some(occurrence::Typed_range::SingleLineRange(
            types::SingleLineRange {
                line: 0,
                start_character: 6,
                end_character: 10,
                ..Default::default()
            },
        ));
        assert!(
            apply_scip_document(
                parts(),
                &doc,
                SOURCE,
                crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
                &original()
            )
            .is_err()
        );
        doc = document();
        doc.occurrences[0].enclosing_range = vec![0, 3, 5];
        assert!(
            apply_scip_document(
                parts(),
                &doc,
                SOURCE,
                crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
                &original()
            )
            .is_err()
        );
    }
    #[test]
    fn reference_does_not_become_a_call_from_text_or_container_alone() {
        let mut before = parts();
        before.outcomes[0].observation_json=json!({"kind":"CALLS","original":{"status":"Exact","range":{"start_byte":17,"end_byte":19}}}).to_string();
        let result = apply_scip_document(
            before,
            &document(),
            SOURCE,
            crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
            &original(),
        )
        .unwrap();
        assert_eq!(result.references[0].kind, None);
        assert_eq!(result.references[0].source_key, None);
    }
    #[test]
    fn typed_capture_binds_only_its_observed_source_and_kind() {
        let mut before = parts();
        before.outcomes[0].observation_json=json!({"relation_type":"READS_FIELD",
            "captures":[{"capture_index":0,"original":{"status":"Exact","range":{"start_byte":16,"end_byte":19}}}],
            "sources":[{"state":"Bound","entity_keys":["stable-nn-key","foreign-key"]}]}).to_string();
        let result = apply_scip_document(
            before,
            &document(),
            SOURCE,
            crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
            &original(),
        )
        .unwrap();
        assert_eq!(result.references.len(), 1);
        assert_eq!(result.references[0].kind.as_deref(), Some("READS_FIELD"));
        assert_eq!(
            result.references[0].source_key.as_deref(),
            Some(result.parts.nodes[0].key.as_str())
        );
        assert_eq!(result.identity_changes[0].previous_key, "stable-nn-key");
    }
    fn resolution_fixture() -> (
        AstGraphNode,
        AstScipDocumentReference,
        AstScipDocumentDefinition,
    ) {
        let original = original();
        let input = parts();
        let file = file_observation_node(
            &input.association,
            "rust",
            "tree-sitter-rust",
            &input.outcomes,
            &input.diagnostics_json,
            &input.capabilities_json,
            &original,
        )
        .unwrap();
        let result = apply_scip_document(
            input,
            &document(),
            SOURCE,
            crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
            &original,
        )
        .unwrap();
        let mut target = result.definitions[0].clone();
        target.path = "src/target.rs".into();
        target.association.path = target.path.clone();
        target.association.relative_components = vec!["src".into(), "target.rs".into()];
        target.entity_key = "target-key".into();
        (file, result.references[0].clone(), target)
    }
    fn outcomes(file: &AstGraphNode) -> Value {
        serde_json::from_str(text(file, "resolution_outcomes_json").unwrap()).unwrap()
    }
    #[test]
    fn deferred_file_resolution_keeps_target_source_association() {
        let (mut file, reference, target) = resolution_fixture();
        apply_scip_document_resolutions(
            &mut file,
            &[(reference, vec![target.clone()])],
            &original(),
        )
        .unwrap();
        let result = outcomes(&file);
        assert_eq!(
            result[0]["target"],
            json!({"state":"Resolved","key":"target-key"})
        );
        let endpoint = &result[0]["observation"]["scip_graph_target"];
        assert_eq!(endpoint["key"], "target-key");
        assert_eq!(endpoint["association"]["relative_path"], "src/target.rs");
        assert_eq!(endpoint["association"]["raw_version"], 7);
        assert_eq!(
            endpoint["association"]["source_sha256"],
            json!(target.association.source_sha256())
        );
        assert!(endpoint["association"].get("index_sha256").is_none());
    }
    #[test]
    fn competing_targets_remain_ambiguous_in_the_file() {
        let (mut file, reference, first) = resolution_fixture();
        let mut second = first.clone();
        second.entity_key = "second-key".into();
        apply_scip_document_resolutions(
            &mut file,
            &[(reference.clone(), vec![first]), (reference, vec![second])],
            &original(),
        )
        .unwrap();
        let result = outcomes(&file);
        assert_eq!(
            result[0]["target"],
            json!({"state":"Ambiguous","keys":["second-key","target-key"]})
        );
        assert!(result[0]["observation"].get("scip_graph_target").is_none());
    }
    #[test]
    fn deferred_resolution_can_be_reapplied_and_invalidated() {
        let (mut file, reference, first) = resolution_fixture();
        let mut second = first.clone();
        second.entity_key = "second-key".into();
        apply_scip_document_resolutions(
            &mut file,
            &[(reference.clone(), vec![first.clone()])],
            &original(),
        )
        .unwrap();
        let uid = scip_edge_uid(&reference, &first).unwrap();
        assert_eq!(uid, scip_edge_uid(&reference, &first).unwrap());
        apply_scip_document_resolutions(
            &mut file,
            &[(reference.clone(), vec![first, second])],
            &original(),
        )
        .unwrap();
        assert_eq!(outcomes(&file)[0]["target"]["state"], "Ambiguous");
        apply_scip_document_resolutions(&mut file, &[(reference, vec![])], &original()).unwrap();
        assert_eq!(
            outcomes(&file)[0]["target"],
            json!({"state":"Unresolved","reason":"ScipUnboundDefinition"})
        );
        assert!(
            outcomes(&file)[0]["observation"]
                .get("scip_graph_target")
                .is_none()
        );
    }
    #[test]
    fn invalid_later_resolution_cannot_partly_change_the_file() {
        let (mut file, reference, target) = resolution_fixture();
        let before = file.clone();
        let mut wrong = target.clone();
        wrong.symbol = "scip-rust cargo other 1.0.0 different().".into();
        assert!(
            apply_scip_document_resolutions(
                &mut file,
                &[
                    (reference.clone(), vec![target]),
                    (reference.clone(), vec![wrong])
                ],
                &original()
            )
            .is_err()
        );
        assert_eq!(file, before);
        let mut wrong_reference = reference;
        wrong_reference.association.source_sha256 = [0; 32];
        assert!(
            apply_scip_document_resolutions(
                &mut file,
                &[(wrong_reference, Vec::new())],
                &original()
            )
            .is_err()
        );
        assert_eq!(file, before);
    }
    #[test]
    fn compiler_reference_replaces_only_the_proven_call_occurrence() {
        let original = original();
        let body = SOURCE.replace("foo();", "foo(); foo();");
        let mut input = parts();
        input.association.source_sha256 = Sha256::digest(body.as_bytes()).into();
        let source = SourceText::new(body.as_bytes(), body.len()).unwrap();
        add_range(
            &mut input.nodes[0].properties,
            "",
            source.range(5, 31).unwrap(),
        )
        .unwrap();
        input.edges = vec![
            AstGraphEdge {
                kind: "CALLS".into(),
                source_key: "stable-nn-key".into(),
                target_key: "old-parser-target".into(),
                uid: "first-call-uid".into(),
                query_data_key: Some("calls".into()),
                reference_target: "old".into(),
                occurrence_ordinal: 0,
            },
            AstGraphEdge {
                kind: "CALLS".into(),
                source_key: "stable-nn-key".into(),
                target_key: "old-parser-target".into(),
                uid: "second-call-uid".into(),
                query_data_key: Some("calls".into()),
                reference_target: "old".into(),
                occurrence_ordinal: 1,
            },
            AstGraphEdge {
                kind: "CONTAINS".into(),
                source_key: "container".into(),
                target_key: "stable-nn-key".into(),
                uid: "containment-uid".into(),
                query_data_key: None,
                reference_target: "stable-nn-key".into(),
                occurrence_ordinal: 0,
            },
        ];
        input.outcomes[0].target = AstGraphRelationTarget::Resolved {
            key: "old-parser-target".into(),
        };
        let mut observation: Value =
            serde_json::from_str(&input.outcomes[0].observation_json).unwrap();
        observation["graph_edge_uids"] = json!(["first-call-uid"]);
        input.outcomes[0].observation_json = observation.to_string();
        let mut second = input.outcomes[0].clone();
        second.relation_index = 1;
        observation["graph_edge_uids"] = json!(["second-call-uid"]);
        observation["original"]["range"] = json!({"start_byte":23,"end_byte":26});
        second.observation_json = observation.to_string();
        input.outcomes.push(second);
        let mut file = file_observation_node(
            &input.association,
            "rust",
            "tree-sitter-rust",
            &input.outcomes,
            &input.diagnostics_json,
            &input.capabilities_json,
            &original,
        )
        .unwrap();
        let doc = Document {
            relative_path: "src/main.rs".into(),
            position_encoding: types::PositionEncoding::UTF8CodeUnitOffsetFromLineStart.into(),
            occurrences: vec![Occurrence {
                range: vec![0, 16, 19],
                symbol: "scip-rust cargo external 1.0.0 actual().".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let result = apply_scip_document(
            input,
            &doc,
            &body,
            crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
            &original,
        )
        .unwrap();
        assert_eq!(
            result
                .parts
                .edges
                .iter()
                .map(|edge| edge.uid.as_str())
                .collect::<Vec<_>>(),
            vec!["second-call-uid", "containment-uid"]
        );
        assert_eq!(result.references[0].graph_edge_uids, vec!["first-call-uid"]);
        assert_eq!(
            result.references[0].parser_target.as_deref(),
            Some("old-parser-target")
        );
        assert_eq!(result.references[0].target_labels, vec!["Function"]);
        apply_scip_document_resolutions(
            &mut file,
            &[(result.references[0].clone(), Vec::new())],
            &original,
        )
        .unwrap();
        let resolved = outcomes(&file);
        assert_eq!(
            resolved[0]["target"],
            json!({"state":"Unresolved","reason":"ScipUnboundDefinition"})
        );
        assert_eq!(
            resolved[1]["target"],
            json!({"state":"Resolved","key":"old-parser-target"})
        );
    }
    #[test]
    fn target_rule_excludes_other_views_of_the_same_symbol() {
        let (mut file, reference, target) = resolution_fixture();
        let mut other = target.clone();
        other.entity_key = "other-label-key".into();
        other.label = "Variable".into();
        apply_scip_document_resolutions(
            &mut file,
            &[(reference, vec![target, other])],
            &original(),
        )
        .unwrap();
        assert_eq!(
            outcomes(&file)[0]["target"],
            json!({"state":"Resolved","key":"target-key"})
        );
    }
    #[tokio::test]
    async fn observed_only_rust_calls_keep_exact_provenance_for_compiler_scip()
    -> Result<(), Box<dyn Error>> {
        use crate::{
            catalog::{
                CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind,
            },
            indexed_source::{read_indexed_source, source_schema},
            prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput},
            profiles::{
                ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput,
            },
            scalar_policy::{RegexLimits, ScalarPolicy},
            scip_syntax::{AstScipSyntaxInput, produce_scip_syntax_document},
            source_inspection::IndexedSource,
            symbols::{IdentityLimits, IdentityLimitsInput},
            tree_sitter_context::PreparedTreeSitterContexts,
            tree_sitter_engine::builtin,
            tree_sitter_model::ParsedTreeSitterDocument,
        };
        use arrow_array::{RecordBatch, StringArray};
        use graphit_data::lance_module_store::LanceModuleStore;
        let original = original();
        let temp = tempfile::TempDir::new()?;
        let uri = temp.path().join("source.lance");
        let body = "fn callee() {}\nfn caller() { callee(); callee(); }\n";
        let row = RecordBatch::try_new(
            source_schema(),
            vec![
                Arc::new(StringArray::from(vec!["src/lib.rs"])),
                Arc::new(StringArray::from(vec![body])),
            ],
        )?;
        let store =
            LanceModuleStore::create(uri.to_str().unwrap(), source_schema(), vec![row]).await?;
        let raw = read_indexed_source(&store.snapshot(), "src/lib.rs", &original).await?;
        let IndexedSource::Available(text) = raw.source(&original)? else {
            panic!("real Raw text")
        };
        let id = ProfileId::new("observed-rust", 256)?;
        let decoder = ProfileDecoder::new(ScalarPolicy::new(RegexLimits {
            compiled_size_policy_bytes: 1 << 20,
            dfa_cache_policy_bytes: 1 << 20,
        })?);
        let decoded = [decoder
            .decode(
                ProfileInput {
                    id: &id,
                    bytes: include_bytes!("../../profiles/queries/rust.yaml"),
                },
                &ProfileLimits::new(ProfileLimitsInput {
                    input_bytes: 1 << 20,
                    nodes: 32768,
                    depth: 64,
                    text_bytes: 1 << 20,
                    scalar_bytes: 32768,
                    collection_entries: 4096,
                })?,
                &original,
            )
            .map_err(|error| io::Error::other(error.to_string()))?];
        let catalog = CatalogDraft::build(
            &[ProfileLayer {
                kind: ProfileLayerKind::Runtime,
                profiles: &decoded,
            }],
            &CatalogLimits::new(CatalogLimitsInput {
                policy_entries: 4096,
                selector_bytes: 32768,
            })?,
            &original,
        )?;
        let profile = &catalog.profiles()[0];
        let factory = PreparedTreeSitterContexts::prepare(
            profile,
            builtin(profile.grammar().value())?,
            &original,
        )?;
        let span = ByteSpan {
            start: 0,
            end: text.len() as u64,
        };
        let segments = [OriginSegment::Direct {
            prepared: span,
            original: span,
        }];
        let input = PreparedInput::new(
            text.as_bytes(),
            text.as_bytes(),
            OriginPlan {
                segments: &segments,
                removed_original: &[],
                anchors: &[],
            },
            &original,
        )?;
        let path = ["src", "lib.rs"];
        let limits = IdentityLimits::new(IdentityLimitsInput {
            source_bytes: 1 << 20,
            path_components: 16,
            path_component_bytes: 1024,
            identity_text_bytes: 32768,
            owner_depth: 64,
            uid_bytes: 1 << 20,
        })?;
        let parsed = ParsedTreeSitterDocument::collect(&factory, &input, &path, limits)?;
        let semantic = parsed.assemble()?;
        assert!(
            semantic.local_relations()?.is_empty(),
            "do not invent a Rust lexical resolver"
        );
        let batch = assemble_graph_ts(
            AstGraphTsInput {
                document: &semantic,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        assert!(batch.edges().iter().all(|edge| edge.kind() != "CALLS"));
        assert_eq!(batch.outcomes().len(), 2);
        assert!(batch.outcomes().iter().all(|o| matches!(
            o.target(),
            AstGraphRelationTarget::ObservedOnly
        ) && o.source_index().is_some()
            && o.capture_index().is_some()));
        let mut file = batch.file_observation().clone();
        let parts = batch.into_parts();
        let syntax = produce_scip_syntax_document(
            AstScipSyntaxInput {
                graph: &parts,
                source: text,
                project_id: "project",
                project_version: "main",
            },
            &original,
        )?;
        assert!(
            syntax.occurrences.iter().all(|o| o.symbol.is_empty()
                || o.symbol_roles & types::SymbolRole::Definition as i32 != 0),
            "syntax-only Rust must not guess reference bindings"
        );
        let source = SourceText::new(text.as_bytes(), text.len())?;
        let compiler_symbol = "scip-rust cargo fixture 1.0.0 callee().";
        let mut occurrences = Vec::new();
        for (ordinal, (offset, _)) in text.match_indices("callee").enumerate() {
            let a = source.point_at(offset)?;
            let b = source.point_at(offset + 6)?;
            occurrences.push(Occurrence {
                range: vec![a.row as i32, a.column as i32, b.column as i32],
                symbol: compiler_symbol.into(),
                symbol_roles: if ordinal == 0 {
                    types::SymbolRole::Definition as i32
                } else {
                    0
                },
                ..Default::default()
            });
        }
        let compiler = Document {
            relative_path: "src/lib.rs".into(),
            language: "rust".into(),
            position_encoding: types::PositionEncoding::UTF8CodeUnitOffsetFromLineStart.into(),
            occurrences,
            ..Default::default()
        };
        let result = apply_scip_document(
            parts,
            &compiler,
            text,
            crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
            &original,
        )?;
        assert_eq!(result.references.len(), 2);
        assert_eq!(result.definitions.len(), 1);
        let caller = result
            .parts
            .nodes
            .iter()
            .find(|n| super::text(n, "name") == Some("caller"))
            .unwrap();
        for reference in &result.references {
            assert_eq!(reference.kind.as_deref(), Some("CALLS"));
            assert_eq!(reference.source_key.as_deref(), Some(caller.key()));
            assert_eq!(reference.target_labels, vec!["Function"]);
            assert!(reference.graph_edge_uids.is_empty());
            assert!(reference.parser_target.is_none());
        }
        assert_ne!(
            result.references[0].start_byte,
            result.references[1].start_byte
        );
        let resolutions = result
            .references
            .into_iter()
            .map(|reference| (reference, result.definitions.clone()))
            .collect::<Vec<_>>();
        apply_scip_document_resolutions(&mut file, &resolutions, &original)?;
        assert!(
            outcomes(&file)
                .as_array()
                .unwrap()
                .iter()
                .all(|o| o["target"]
                    == json!({"state":"Resolved","key":result.definitions[0].entity_key}))
        );
        Ok(())
    }
    #[tokio::test]
    async fn compiler_only_source_uses_real_raw_without_a_parser_profile()
    -> Result<(), Box<dyn Error>> {
        use crate::{
            indexed_source::{read_indexed_source, source_schema},
            symbols::{
                IdentityError, IdentityErrorCode, IdentityField, IdentityLimits,
                IdentityLimitsInput,
            },
        };
        use arrow_array::{RecordBatch, StringArray};
        use graphit_data::lance_module_store::LanceModuleStore;
        let temp = tempfile::TempDir::new()?;
        let uri = temp.path().join("source.lance");
        let original = original();
        let body = "Module Example\nEnd Module\n";
        let batch = RecordBatch::try_new(
            source_schema(),
            vec![
                Arc::new(StringArray::from(vec!["src/example.vb"])),
                Arc::new(StringArray::from(vec![body])),
            ],
        )?;
        let store =
            LanceModuleStore::create(uri.to_str().unwrap(), source_schema(), vec![batch]).await?;
        let raw = read_indexed_source(&store.snapshot(), "src/example.vb", &original).await?;
        let limits_input = IdentityLimitsInput {
            source_bytes: 1 << 20,
            path_components: 16,
            path_component_bytes: 1024,
            identity_text_bytes: 32768,
            owner_depth: 64,
            uid_bytes: 1 << 20,
        };
        let limits = IdentityLimits::new(limits_input)?;
        let mut document = Document {
            relative_path: "src/example.vb".into(),
            language: "VisualBasic".into(),
            position_encoding: types::PositionEncoding::UTF8CodeUnitOffsetFromLineStart.into(),
            occurrences: vec![Occurrence {
                range: vec![0, 7, 14],
                symbol: "scip-dotnet nuget Demo 1.0.0 Example/".into(),
                symbol_roles: types::SymbolRole::Definition as i32,
                ..Default::default()
            }],
            ..Default::default()
        };
        let assembled =
            assemble_scip_source(&document, NativeNnSource::Indexed(&raw), limits, &original)?;
        assert_eq!(assembled.association.path(), "src/example.vb");
        assert_eq!(assembled.association.raw_version(), raw.version());
        assert_eq!(
            assembled.association.source_sha256(),
            &<[u8; 32]>::from(Sha256::digest(body.as_bytes()))
        );
        assert!(assembled.nodes.is_empty());
        assert_eq!(
            text(&assembled.file_observation, "grammar"),
            Some("scip-compiler")
        );
        assert_eq!(
            text(&assembled.file_observation, "lang"),
            Some("VisualBasic")
        );
        assert_eq!(
            assembled.file_observation.properties.get("model_available"),
            Some(&AstGraphValue::Bool(true))
        );
        let applied = apply_scip_document(
            assembled.into_parts(),
            &document,
            body,
            crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
            &original,
        )?;
        assert_eq!(applied.definitions.len(), 1);
        assert_eq!(text(&applied.parts.nodes[0], "name"), Some("Example"));
        document.relative_path = "elsewhere.vb".into();
        assert!(
            assemble_scip_source(&document, NativeNnSource::Indexed(&raw), limits, &original)
                .is_err()
        );
        document.relative_path = "src/example.vb".into();
        document.text = "another source".into();
        assert!(
            assemble_scip_source(&document, NativeNnSource::Indexed(&raw), limits, &original)
                .is_err()
        );
        document.text.clear();
        document.language.clear();
        let error =
            assemble_scip_source(&document, NativeNnSource::Indexed(&raw), limits, &original)
                .expect_err("empty language must be rejected");
        let AstGraphError::Operation(error) = error else {
            panic!("identity cause was discarded: {error:?}");
        };
        let identity = error
            .get_ref()
            .unwrap()
            .downcast_ref::<IdentityError>()
            .unwrap();
        assert_eq!(identity.code(), IdentityErrorCode::InvalidDocument);
        assert_eq!(identity.field(), Some(IdentityField::Language));
        document.language = "VisualBasic".into();
        let short_limit = IdentityLimits::new(IdentityLimitsInput {
            source_bytes: body.len() - 1,
            ..limits_input
        })?;
        let error = assemble_scip_source(
            &document,
            NativeNnSource::Indexed(&raw),
            short_limit,
            &original,
        )
        .expect_err("source byte limit must be enforced");
        let AstGraphError::Operation(error) = error else {
            panic!("identity cause was discarded: {error:?}");
        };
        let identity = error
            .get_ref()
            .unwrap()
            .downcast_ref::<IdentityError>()
            .unwrap();
        assert_eq!(identity.code(), IdentityErrorCode::LimitExceeded);
        assert_eq!(identity.field(), Some(IdentityField::SourceBytes));
        let foreign_original = Arc::new(SnapshotContext::service(Cancellation::default()));
        assert!(
            assemble_scip_source(
                &document,
                NativeNnSource::Indexed(&raw),
                limits,
                &foreign_original
            )
            .is_err()
        );
        let missing = read_indexed_source(&store.snapshot(), "missing.vb", &original).await?;
        assert!(
            assemble_scip_source(
                &document,
                NativeNnSource::Indexed(&missing),
                limits,
                &original
            )
            .is_err()
        );
        let cancellation = Cancellation::default();
        let cancelled = Arc::new(SnapshotContext::service(cancellation.clone()));
        cancellation.cancel();
        assert!(matches!(
            assemble_scip_source(&document, NativeNnSource::Indexed(&raw), limits, &cancelled),
            Err(AstGraphError::Operation(error)) if error.kind() == io::ErrorKind::Interrupted
        ));
        let expired = Arc::new(SnapshotContext::new(
            std::time::Instant::now() - std::time::Duration::from_secs(1),
            Cancellation::default(),
        ));
        assert!(matches!(
            assemble_scip_source(&document, NativeNnSource::Indexed(&raw), limits, &expired),
            Err(AstGraphError::Operation(error)) if error.kind() == io::ErrorKind::TimedOut
        ));
        Ok(())
    }
    #[test]
    fn canonical_consumer_preserves_utf8_utf16_utf32_and_typed_coordinates() {
        for (encoding, definition, reference, enclosure) in [
            (
                types::PositionEncoding::UTF8CodeUnitOffsetFromLineStart,
                [0, 8, 11],
                [0, 16, 19],
                [0, 5, 24],
            ),
            (
                types::PositionEncoding::UTF16CodeUnitOffsetFromLineStart,
                [0, 6, 9],
                [0, 14, 17],
                [0, 3, 22],
            ),
            (
                types::PositionEncoding::UTF32CodeUnitOffsetFromLineStart,
                [0, 5, 8],
                [0, 13, 16],
                [0, 2, 21],
            ),
        ] {
            let mut doc = document();
            doc.position_encoding = encoding.into();
            doc.occurrences[0].range = definition.to_vec();
            doc.occurrences[0].enclosing_range = enclosure.to_vec();
            doc.occurrences[1].range = reference.to_vec();
            let result = apply_scip_document(
                parts(),
                &doc,
                SOURCE,
                crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
                &original(),
            )
            .unwrap();
            assert_eq!(
                (
                    result.definitions[0].start_byte,
                    result.definitions[0].end_byte
                ),
                (8, 11)
            );
            assert_eq!(
                (
                    result.references[0].start_byte,
                    result.references[0].end_byte
                ),
                (16, 19)
            );
        }
        let mut doc = document();
        doc.occurrences[0].range.clear();
        doc.occurrences[0].typed_range = Some(occurrence::Typed_range::SingleLineRange(
            types::SingleLineRange {
                line: 0,
                start_character: 6,
                end_character: 9,
                ..Default::default()
            },
        ));
        let result = apply_scip_document(
            parts(),
            &doc,
            SOURCE,
            crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
            &original(),
        )
        .unwrap();
        assert_eq!(
            (
                result.definitions[0].start_byte,
                result.definitions[0].end_byte
            ),
            (8, 11)
        );
    }
    #[test]
    fn local_symbols_cannot_resolve_to_another_document() {
        let (mut file, mut reference, mut target) = resolution_fixture();
        reference.symbol = "local 0".into();
        target.symbol = "local 0".into();
        let before = file.clone();
        assert!(
            apply_scip_document_resolutions(
                &mut file,
                &[(reference.clone(), vec![target.clone()])],
                &original()
            )
            .is_err()
        );
        assert_eq!(file, before);
        target.path = reference.path.clone();
        target.association = reference.association.clone();
        apply_scip_document_resolutions(&mut file, &[(reference, vec![target])], &original())
            .unwrap();
        assert_eq!(outcomes(&file)[0]["target"]["state"], "Resolved");
    }
    #[test]
    fn cancellation_is_terminal() {
        let cancellation = Cancellation::default();
        cancellation.cancel();
        let original = Arc::new(SnapshotContext::service(cancellation));
        assert!(matches!(
            apply_scip_document(
                parts(),
                &document(),
                SOURCE,
                crate::semantic_graph::scip_document::AstScipDocumentOrigin::Compiler,
                &original
            ),
            Err(AstGraphError::Operation(_))
        ));
    }
}
