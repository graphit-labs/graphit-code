//! Owned semantic Graph rows and observational FILE records bound to genuine Source.
mod generic;
pub use generic::{
    AstGraphEmbeddedComposition, AstGraphGenericInput, assemble_graph_generic,
    bind_graph_generic_embedded_host, merge_graph_embedded, observe_graph_embedded,
};
mod antlr;
pub mod antlr_generic;
pub mod oracle;
pub use antlr::{AstGraphAntlrInput, assemble_graph_antlr};

pub mod scip_document;

mod json;
pub use json::{AstGraphJsonInput, assemble_graph_json};

use crate::{
    SourceRange,
    native_nn::{NativeNnError, NativeNnSource, associated_source},
    prepared_input::{OriginPiece, OriginalProjection},
    semantic_fields::ExtentObservation,
    tree_sitter_model::*,
};
use graphit_core::file_io::SnapshotContext;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, io,
    sync::Arc,
};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AstGraphValue {
    String(String),
    Int64(i64),
    Bool(bool),
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AstGraphNode {
    label: String,
    key: String,
    properties: BTreeMap<String, AstGraphValue>,
}
impl AstGraphNode {
    pub fn into_parts(self) -> (String, String, BTreeMap<String, AstGraphValue>) {
        (self.label, self.key, self.properties)
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn properties(&self) -> &BTreeMap<String, AstGraphValue> {
        &self.properties
    }
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AstGraphEdge {
    kind: String,
    source_key: String,
    target_key: String,
    uid: String,
}
impl AstGraphEdge {
    pub fn into_parts(self) -> (String, String, String, String) {
        (self.kind, self.source_key, self.target_key, self.uid)
    }
    pub fn kind(&self) -> &str {
        &self.kind
    }
    pub fn source_key(&self) -> &str {
        &self.source_key
    }
    pub fn target_key(&self) -> &str {
        &self.target_key
    }
    pub fn uid(&self) -> &str {
        &self.uid
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AstGraphSourceObservation {
    IndexedStored { empty: bool },
    PhysicalTextNotStored { empty: bool },
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AstGraphAssociation {
    source_observation: AstGraphSourceObservation,
    path: String,
    relative_components: Vec<String>,
    raw_version: u64,
    source_sha256: [u8; 32],
    physical_branch: Option<String>,
    inherited_target: Option<String>,
}
impl AstGraphAssociation {
    pub fn source_observation(&self) -> AstGraphSourceObservation {
        self.source_observation
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn relative_components(&self) -> &[String] {
        &self.relative_components
    }
    pub fn raw_version(&self) -> u64 {
        self.raw_version
    }
    pub fn source_sha256(&self) -> &[u8; 32] {
        &self.source_sha256
    }
    pub fn physical_branch(&self) -> Option<&str> {
        self.physical_branch.as_deref()
    }
    pub fn inherited_target(&self) -> Option<&str> {
        self.inherited_target.as_deref()
    }
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AstGraphRelationTarget {
    Resolved { key: String },
    NotFound,
    Ambiguous { keys: Vec<String> },
    Unresolved { reason: TsLocalUnresolvedReason },
    GenericUnresolved { reason: String },
    ObservedOnly,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AstGraphRelationOutcome {
    relation_index: usize,
    source_index: Option<usize>,
    capture_index: Option<usize>,
    evidence_node_indices: Vec<usize>,
    target: AstGraphRelationTarget,
    observation_json: String,
}
impl AstGraphRelationOutcome {
    pub fn relation_index(&self) -> usize {
        self.relation_index
    }
    pub fn source_index(&self) -> Option<usize> {
        self.source_index
    }
    pub fn capture_index(&self) -> Option<usize> {
        self.capture_index
    }
    pub fn evidence_node_indices(&self) -> &[usize] {
        &self.evidence_node_indices
    }
    pub fn target(&self) -> &AstGraphRelationTarget {
        &self.target
    }
    pub fn observation_json(&self) -> &str {
        &self.observation_json
    }
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AstSemanticGraphBatch {
    file_observation: AstGraphNode,
    association: AstGraphAssociation,
    nodes: Vec<AstGraphNode>,
    edges: Vec<AstGraphEdge>,
    outcomes: Vec<AstGraphRelationOutcome>,
    diagnostics_json: String,
    capabilities_json: String,
}
pub struct AstGraphParts {
    pub file_observation: AstGraphNode,
    pub association: AstGraphAssociation,
    pub nodes: Vec<AstGraphNode>,
    pub edges: Vec<AstGraphEdge>,
    pub outcomes: Vec<AstGraphRelationOutcome>,
    pub diagnostics_json: String,
    pub capabilities_json: String,
}
impl AstSemanticGraphBatch {
    pub fn into_parts(self) -> AstGraphParts {
        AstGraphParts {
            file_observation: self.file_observation,
            association: self.association,
            nodes: self.nodes,
            edges: self.edges,
            outcomes: self.outcomes,
            diagnostics_json: self.diagnostics_json,
            capabilities_json: self.capabilities_json,
        }
    }
    pub fn file_observation(&self) -> &AstGraphNode {
        &self.file_observation
    }
    pub fn association(&self) -> &AstGraphAssociation {
        &self.association
    }
    pub fn nodes(&self) -> &[AstGraphNode] {
        &self.nodes
    }
    pub fn edges(&self) -> &[AstGraphEdge] {
        &self.edges
    }
    pub fn outcomes(&self) -> &[AstGraphRelationOutcome] {
        &self.outcomes
    }
    pub fn capabilities_json(&self) -> &str {
        &self.capabilities_json
    }
    pub fn diagnostics_json(&self) -> &str {
        &self.diagnostics_json
    }
}

fn file_observation_node(
    association: &AstGraphAssociation,
    language: &str,
    grammar: &str,
    outcomes: &[AstGraphRelationOutcome],
    diagnostics_json: &str,
    capabilities_json: &str,
    original: &SnapshotContext,
) -> Result<AstGraphNode, AstGraphError> {
    let mut properties = BTreeMap::new();
    properties.insert("is_file_observation".into(), AstGraphValue::Bool(true));
    properties.insert("model_available".into(), AstGraphValue::Bool(true));
    for (key, value) in [
        ("name", association.path()),
        ("path", association.path()),
        ("lang", language),
        ("grammar", grammar),
        ("diagnostics_json", diagnostics_json),
        ("capabilities_json", capabilities_json),
    ] {
        properties.insert(key.into(), AstGraphValue::String(value.into()));
    }
    properties.insert(
        "relative_components_json".into(),
        AstGraphValue::String(serde_json::to_string(&association.relative_components)?),
    );
    properties.insert(
        "raw_version".into(),
        AstGraphValue::String(association.raw_version().to_string()),
    );
    properties.insert(
        "source_sha256".into(),
        AstGraphValue::String(
            association
                .source_sha256
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        ),
    );
    let (stored, empty) = match association.source_observation {
        AstGraphSourceObservation::IndexedStored { empty } => (true, empty),
        AstGraphSourceObservation::PhysicalTextNotStored { empty } => (false, empty),
    };
    properties.insert("source_stored".into(), AstGraphValue::Bool(stored));
    properties.insert("source_empty".into(), AstGraphValue::Bool(empty));
    for (key, value) in [
        ("physical_branch", association.physical_branch()),
        ("inherited_target", association.inherited_target()),
    ] {
        if let Some(value) = value {
            properties.insert(key.into(), AstGraphValue::String(value.into()));
        }
    }
    let mut resolved = Vec::with_capacity(outcomes.len());
    for outcome in outcomes {
        original.check()?;
        let target = match &outcome.target {
            AstGraphRelationTarget::Resolved { key } => json!({"state":"Resolved","key":key}),
            AstGraphRelationTarget::NotFound => json!({"state":"NotFound"}),
            AstGraphRelationTarget::Ambiguous { keys } => json!({"state":"Ambiguous","keys":keys}),
            AstGraphRelationTarget::Unresolved { reason } => {
                json!({"state":"Unresolved","reason":format!("{reason:?}")})
            }
            AstGraphRelationTarget::GenericUnresolved { reason } => {
                json!({"state":"Unresolved","reason":reason})
            }
            AstGraphRelationTarget::ObservedOnly => json!({"state":"ObservedOnly"}),
        };
        let observation: Value = serde_json::from_str(&outcome.observation_json)?;
        resolved.push(json!({"relation_index":outcome.relation_index,"source_index":outcome.source_index,"capture_index":outcome.capture_index,"evidence_node_indices":outcome.evidence_node_indices,"target":target,"observation":observation}));
    }
    properties.insert(
        "resolution_outcomes_json".into(),
        AstGraphValue::String(serde_json::to_string(&resolved)?),
    );
    Ok(AstGraphNode {
        label: "AstFileObservation".into(),
        key: serde_json::to_string(&json!([
            "file_observation",
            association.path(),
            [language, grammar]
        ]))?,
        properties,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AstGraphModelUnavailableReason {
    NoNativeSemanticModel,
    UnsupportedNativeContext,
}
pub struct AstGraphUnavailableFileInput<'a> {
    pub profile: &'a crate::catalog::EffectiveProfile,
    pub source: NativeNnSource<'a>,
    pub reason: AstGraphModelUnavailableReason,
    pub identity_limits: crate::symbols::IdentityLimits,
}
/// Observe a selected FILE whose semantic model did not run. No syntax model is constructed.
pub fn observe_unavailable_graph_file(
    input: AstGraphUnavailableFileInput<'_>,
    original: &Arc<SnapshotContext>,
) -> Result<AstGraphNode, AstGraphError> {
    original.check()?;
    let result = (|| {
        let raw = match &input.source {
            NativeNnSource::Indexed(raw) | NativeNnSource::Physical { raw, .. } => *raw,
        };
        let source = match &input.source {
            NativeNnSource::Indexed(raw) => {
                match raw.source(original).map_err(NativeNnError::Source)? {
                    crate::source_inspection::IndexedSource::Available(source) => source,
                    crate::source_inspection::IndexedSource::Missing => {
                        return Err(NativeNnError::InvalidInput.into());
                    }
                }
            }
            NativeNnSource::Physical { capture, .. } => capture
                .source(original)
                .map_err(|e| NativeNnError::PhysicalSource(Box::new(e)))?,
        };
        let components: Vec<_> = raw.path().split('/').collect();
        let profile = input.profile;
        // The pure identity kernel binds genuine Source bytes/path/Original only; no symbol is made.
        let identity = crate::symbols::IdentityDocument::new(
            crate::symbols::IdentityDocumentInput {
                relative_components: &components,
                language: profile.language().value(),
                grammar: profile.grammar().value(),
                original_bytes: source.as_bytes(),
            },
            input.identity_limits,
            original,
        )
        .map_err(TreeSitterModelError::Identity)?;
        let (path, hash) = associated_source(&input.source, &identity, source, original)?;
        let source_observation = match &input.source {
            NativeNnSource::Indexed(_) => AstGraphSourceObservation::IndexedStored {
                empty: source.is_empty(),
            },
            NativeNnSource::Physical { .. } => AstGraphSourceObservation::PhysicalTextNotStored {
                empty: source.is_empty(),
            },
        };
        let branch = raw.branch_observation();
        let association = AstGraphAssociation {
            path,
            relative_components: components.iter().map(|s| (*s).into()).collect(),
            raw_version: raw.version(),
            source_sha256: hash,
            source_observation,
            physical_branch: branch.and_then(|b| b.physical_branch).map(str::to_owned),
            inherited_target: branch.and_then(|b| b.inherited_target).map(str::to_owned),
        };
        let reason = format!("{:?}", input.reason);
        let diagnostics = serde_json::to_string(
            &json!([{"scope":"file","issue":"SemanticModelUnavailable","reason":reason}]),
        )?;
        let mut capabilities = Vec::new();
        for (query_index, query) in profile.queries().value().iter().enumerate() {
            original.check()?;
            capabilities.push(json!({"query_index":query_index,"policy":query_json(query.value()),"state":"NotEvaluated","model_available":false,"reason":reason}));
        }
        let capabilities = serde_json::to_string(&capabilities)?;
        let mut observation = file_observation_node(
            &association,
            profile.language().value(),
            profile.grammar().value(),
            &[],
            &diagnostics,
            &capabilities,
            original,
        )?;
        observation
            .properties
            .insert("model_available".into(), AstGraphValue::Bool(false));
        observation.properties.insert(
            "model_unavailable_reason".into(),
            AstGraphValue::String(reason),
        );
        Ok(observation)
    })();
    let revalidation = match &input.source {
        NativeNnSource::Indexed(_) => Ok(()),
        NativeNnSource::Physical { capture, .. } => capture
            .revalidate(original)
            .map_err(|e| NativeNnError::PhysicalSource(Box::new(e))),
    };
    original.check()?;
    revalidation?;
    result
}

pub struct AstGraphTsInput<'a, 'doc, 'profile, 'o, 'v, 'request> {
    pub document: &'a TreeSitterSemanticDocument<'doc, 'profile, 'o, 'v, 'request>,
    pub source: NativeNnSource<'a>,
}
#[derive(Debug)]
pub enum AstGraphError {
    Operation(io::Error),
    Model(Box<TreeSitterModelError>),
    Generic(Box<crate::tree_sitter_generic_model::GenericModelError>),
    Antlr(Box<crate::antlr_engine::model::AntlrModelError>),
    Source(Box<NativeNnError>),
    Serialization(serde_json::Error),
    Representation,
    NativeContract(&'static str),
}
impl fmt::Display for AstGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(e) => write!(f, "AST Graph operation: {e}"),
            Self::Model(e) => write!(f, "AST Graph model: {e}"),
            Self::Generic(e) => write!(f, "AST Graph generic model: {e}"),
            Self::Antlr(e) => write!(f, "AST Graph ANTLR model: {e}"),
            Self::Source(e) => write!(f, "AST Graph Source: {e}"),
            Self::Serialization(e) => write!(f, "AST Graph JSON: {e}"),
            Self::Representation => f.write_str("AST Graph coordinate representation overflow"),
            Self::NativeContract(e) => write!(f, "AST Graph native contract: {e}"),
        }
    }
}
impl Error for AstGraphError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(e) => Some(e),
            Self::Model(e) => Some(e.as_ref()),
            Self::Generic(e) => Some(e.as_ref()),
            Self::Antlr(e) => Some(e.as_ref()),
            Self::Source(e) => Some(e.as_ref()),
            Self::Serialization(e) => Some(e),
            Self::Representation | Self::NativeContract(_) => None,
        }
    }
}
impl From<io::Error> for AstGraphError {
    fn from(e: io::Error) -> Self {
        Self::Operation(e)
    }
}
impl From<TreeSitterModelError> for AstGraphError {
    fn from(e: TreeSitterModelError) -> Self {
        Self::Model(Box::new(e))
    }
}
impl From<crate::antlr_engine::model::AntlrModelError> for AstGraphError {
    fn from(e: crate::antlr_engine::model::AntlrModelError) -> Self {
        Self::Antlr(Box::new(e))
    }
}
impl From<NativeNnError> for AstGraphError {
    fn from(e: NativeNnError) -> Self {
        Self::Source(Box::new(e))
    }
}
impl From<serde_json::Error> for AstGraphError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e)
    }
}

fn integer(n: usize) -> Result<i64, AstGraphError> {
    i64::try_from(n).map_err(|_| AstGraphError::Representation)
}
fn line(row: usize) -> Result<i64, AstGraphError> {
    integer(row.checked_add(1).ok_or(AstGraphError::Representation)?)
}
fn range_json(range: SourceRange<'_>) -> Result<Value, AstGraphError> {
    let a = range.start_point();
    let b = range.end_point();
    Ok(
        json!({"start_byte":integer(range.start())?, "end_byte":integer(range.end())?,
        "start_row":integer(a.row)?, "start_column":integer(a.column)?,
        "end_row":integer(b.row)?, "end_column":integer(b.column)?,
        "start_line":line(a.row)?, "end_line":line(b.row)?}),
    )
}
fn origin_json(origin: &OriginalProjection<'_>) -> Result<Value, AstGraphError> {
    Ok(match origin {
        OriginalProjection::Exact(r) => json!({"status":"Exact","range":range_json(*r)?}),
        OriginalProjection::Compound(pieces) => {
            let mut values = Vec::new();
            for piece in pieces {
                values.push(match piece {
                    OriginPiece::Original(r) => {
                        json!({"status":"Original","range":range_json(*r)?})
                    }
                    OriginPiece::Synthetic(reason) => {
                        json!({"status":"Synthetic","reason":format!("{reason:?}")})
                    }
                    OriginPiece::Unmappable(reason) => {
                        json!({"status":"Unmappable","reason":format!("{reason:?}")})
                    }
                });
            }
            json!({"status":"Compound","pieces":values})
        }
        OriginalProjection::Synthetic(reason) => {
            json!({"status":"Synthetic","reason":format!("{reason:?}")})
        }
        OriginalProjection::Unmappable(reason) => {
            json!({"status":"Unmappable","reason":format!("{reason:?}")})
        }
        OriginalProjection::Ambiguous(ranges) => {
            json!({"status":"Ambiguous","ranges":ranges.iter().copied().map(range_json).collect::<Result<Vec<_>,_>>()?})
        }
    })
}
fn extent_json(extent: &ExtentObservation<'_, '_>) -> Result<Value, AstGraphError> {
    Ok(match extent {
        ExtentObservation::Exact(e) => json!({"status":"Exact","range":range_json(e.range())?}),
        ExtentObservation::NonExact(p) => json!({"status":"NonExact","origin":origin_json(p)?}),
        ExtentObservation::Missing => json!({"status":"Missing"}),
    })
}
fn syntax_json(syntax: &TsNativeSyntax<'_, '_, '_>) -> Result<Value, AstGraphError> {
    Ok(json!({"node_index":syntax.node_index,"kind":syntax.kind,
        "prepared":range_json(syntax.prepared)?, "original":origin_json(syntax.original)?,
        "literal_source":syntax.raw.map(|r| r.text())}))
}
fn attachment_json(a: &TsAttachment<'_, '_>) -> Result<Value, AstGraphError> {
    let content = match &a.content {
        TsAttachmentContent::Comment => json!({"kind":"Comment"}),
        TsAttachmentContent::Documentation(text) => json!({"kind":"Documentation","text":text}),
        TsAttachmentContent::Attribute(_) => json!({"kind":"Attribute"}),
        TsAttachmentContent::Unavailable => json!({"kind":"Unavailable"}),
    };
    Ok(
        json!({"node_index":a.node_index,"placement":format!("{:?}",a.placement),
        "original":origin_json(a.original)?,"literal_source":a.raw.map(|r| r.text()),"content":content}),
    )
}
fn issue_json(i: &TsCallableIssue<'_, '_>) -> Result<Value, AstGraphError> {
    Ok(
        json!({"node_index":i.node_index,"kind":format!("{:?}",i.kind),
        "original":i.original.map(origin_json).transpose()?}),
    )
}

fn value_json(value: &TsDeclaredValue<'_, '_, '_>) -> Result<Value, AstGraphError> {
    Ok(match value {
        TsDeclaredValue::NotConfigured => json!({"status":"NotConfigured"}),
        TsDeclaredValue::Missing { declared } => json!({"status":"Missing","declared":declared}),
        TsDeclaredValue::Native {
            subject_ordinal,
            subject_count,
            initializer,
            expressions,
            captures,
        } => {
            let mut captured = Vec::new();
            for c in captures {
                captured.push(json!({"capture_index":c.capture_index,"syntax":syntax_json(&c.syntax)?,"binding":format!("{:?}",c.binding)}));
            }
            json!({"status":"Native","subject_ordinal":subject_ordinal,"subject_count":subject_count,
                "initializer":initializer.as_deref().map(syntax_json).transpose()?,
                "expressions":expressions.iter().map(syntax_json).collect::<Result<Vec<_>,_>>()?,
                "captures":captured})
        }
    })
}
fn python_syntax_json(syntax: &TsNativeSyntax<'_, '_, '_>) -> Result<Value, AstGraphError> {
    Ok(json!({"node_index":syntax.node_index,"kind":syntax.kind,
        "prepared":range_json(syntax.prepared)?,"original":origin_json(syntax.original)?}))
}
fn contribution_json(c: &TsContribution<'_, '_, '_>) -> Result<Value, AstGraphError> {
    let declared_type = match &c.declared_type {
        TsDeclaredType::NotApplicable => json!({"status":"NotApplicable"}),
        TsDeclaredType::Unavailable => json!({"status":"Unavailable"}),
        TsDeclaredType::Native(s) => json!({"status":"Native","syntax":syntax_json(s)?}),
        TsDeclaredType::Present(s) => {
            json!({"status":"Rust","node_index":s.node_index,"literal_source":s.raw.text(),"original":origin_json(s.original)?})
        }
    };
    let mut namespaces = Vec::new();
    for n in &c.namespaces {
        namespaces.push(json!({"node_index":n.node_index,"context_index":n.context_index,
            "parent_capture_index":n.explicit_parent_capture_index,"label":n.label,"name":n.name,
            "anchor":range_json(n.anchor)?,"extent":extent_json(&n.extent)?,"selection":format!("{:?}",n.selection)}));
    }
    let declaration = c.go_declaration.as_ref().map(|d| -> Result<Value,AstGraphError> {
        Ok(json!({"native_kind":d.native_kind,"alias":d.alias.as_ref().map(syntax_json).transpose()?,
            "export":format!("{:?}",d.export),"attachment_owner_node_index":d.attachment_owner_node_index,
            "attachments":d.attachments.iter().map(attachment_json).collect::<Result<Vec<_>,_>>()?,
            "issues":d.issues.iter().map(issue_json).collect::<Result<Vec<_>,_>>()?}))
    }).transpose()?;
    let ecma_declaration = c
        .ecma_declaration
        .as_ref()
        .map(|d| -> Result<Value, AstGraphError> {
            Ok(
                json!({"native_kind":d.native_kind,"export":format!("{:?}",d.export),
            "attachment_owner_node_index":d.attachment_owner_node_index,
            "attachments":d.attachments.iter().map(attachment_json).collect::<Result<Vec<_>,_>>()?,
            "issues":d.issues.iter().map(issue_json).collect::<Result<Vec<_>,_>>()?}),
            )
        })
        .transpose()?;
    let callable = match &c.callable {
        TsCallable::NotApplicable => json!({"status":"NotApplicable"}),
        TsCallable::Function(f) => {
            let go = f
                .signature
                .go
                .as_ref()
                .map(|g| -> Result<Value, AstGraphError> {
                    Ok(
                        json!({"receiver":g.receiver.as_ref().map(syntax_json).transpose()?,
                    "type_parameters":g.type_parameters.as_ref().map(syntax_json).transpose()?,
                    "parameters":g.parameters.as_ref().map(syntax_json).transpose()?,
                    "result":g.result.as_ref().map(syntax_json).transpose()?}),
                    )
                })
                .transpose()?;
            let ecma = f
                .signature
                .ecma
                .as_ref()
                .map(|e| -> Result<Value, AstGraphError> {
                    Ok(
                        json!({"parameters":e.parameters.as_ref().map(syntax_json).transpose()?,
                    "type_parameters":e.type_parameters.as_ref().map(syntax_json).transpose()?,
                    "return_type":e.return_type.as_ref().map(syntax_json).transpose()?}),
                    )
                })
                .transpose()?;
            let mut output = json!({"status":"Function","kind":format!("{:?}",f.kind),"body_node_index":f.body_node_index,
                "signature_source":f.signature.raw.map(|r| r.text()),"signature_origin":origin_json(f.signature.original)?,
                "go_signature":go,"ecma_signature":ecma,"export":format!("{:?}",f.export),"complexity":format!("{:?}",f.complexity),
                "attachments":f.attachments.iter().map(attachment_json).collect::<Result<Vec<_>,_>>()?,
                "issues":f.issues.iter().map(issue_json).collect::<Result<Vec<_>,_>>()?}
            );
            if let Some(python) = &f.signature.python {
                output["python_signature"] = json!({
                    "parameters":python.parameters.as_ref().map(python_syntax_json).transpose()?,
                    "type_parameters":python.type_parameters.as_ref().map(python_syntax_json).transpose()?,
                    "return_type":python.return_type.as_ref().map(python_syntax_json).transpose()?});
            }
            output
        }
    };
    let mut output = json!({"query_index":c.occurrence.query_index,"match_index":c.occurrence.match_index,
        "name_capture_index":c.occurrence.name_capture_index,"name_capture_name":c.name_capture.native.name,
        "declaration_node_index":c.declaration_node_index,"container_node_index":c.container_node_index,
        "name_original":origin_json(&c.name_capture.original)?,"span_capture_indices":c.span_capture_indices,
        "extent":extent_json(&c.extent)?,"declared_type":declared_type,"declared_value":value_json(&c.declared_value)?,
        "go_declaration":declaration,"ecma_declaration":ecma_declaration,"excluded_context_indices":c.excluded_context_indices,"callable":callable,"namespaces":namespaces,
        "derived_value":c.derived_value.as_ref().map(|d| json!({
            "owner_uid":d.owner_uid,"owner_label":d.owner_label,
            "owner_declaration_node_index":d.owner_declaration_node_index,
            "owner_name_node_index":d.owner_name_node_index,"owner_name_capture_index":d.owner_name_capture_index,
            "value_capture_index":d.value_capture_index,"binding":format!("{:?}",d.binding)}))});
    if let Some(python) = &c.python_declaration {
        output["python_declaration"] = json!({"native_kind":python.native_kind,
            "export":format!("{:?}",python.export),
            "attachment_owner_node_index":python.attachment_owner_node_index,
            "attachments":python.attachments.iter().map(attachment_json).collect::<Result<Vec<_>,_>>()?,
            "leading_string":python.leading_string.as_ref().map(python_syntax_json).transpose()?,
            "issues":python.issues.iter().map(issue_json).collect::<Result<Vec<_>,_>>()?});
    }
    Ok(output)
}
fn query_json(q: &crate::catalog::Query) -> Value {
    json!({"data_key":q.data_key().value(),"graph_label":q.graph_label().value(),
        "pattern":q.pattern().value(),"relation_type":q.relation_type().value(),
        "name_capture":q.name_capture().value(),"span_capture":q.span_capture().value(),
        "declaration_capture":q.declaration_capture().value(),
        "context_capture":q.context_capture().value(),
        "parent_capture":q.parent_capture().value(),"parent_label":q.parent_label().value(),
        "value_capture":q.value_capture().value(),"value_label":q.value_label().value(),
        "name_reject":q.name_reject().value(),"name_is_data":q.name_is_data().value(),
        "qualifier_capture":q.qualifier_capture().value(),"target_fallback":q.target_fallback().value(),
        "target_labels":q.target_labels().value().iter().map(|s|s.value()).collect::<Vec<_>>()})
}

fn add_range(
    properties: &mut BTreeMap<String, AstGraphValue>,
    prefix: &str,
    range: SourceRange<'_>,
) -> Result<(), AstGraphError> {
    let Value::Object(values) = range_json(range)? else {
        return Err(AstGraphError::NativeContract("range is not an object"));
    };
    for (key, value) in values {
        properties.insert(
            format!("{prefix}{key}"),
            AstGraphValue::Int64(value.as_i64().ok_or(AstGraphError::Representation)?),
        );
    }
    Ok(())
}
fn node_key(nodes: &[AstGraphNode], index: usize) -> Result<&str, AstGraphError> {
    nodes
        .get(index)
        .map(AstGraphNode::key)
        .ok_or(AstGraphError::NativeContract(
            "relation references foreign entity",
        ))
}

/// Association is checked once per file. Output rows carry observations, never parser or Source authority.
pub fn assemble_graph_ts(
    input: AstGraphTsInput<'_, '_, '_, '_, '_, '_>,
    original: &Arc<SnapshotContext>,
) -> Result<AstSemanticGraphBatch, AstGraphError> {
    original.check()?;
    let result = (|| {
        let document = input.document;
        if !std::ptr::eq(document.original_context()?, original.as_ref()) {
            return Err(NativeNnError::RequestMismatch.into());
        }
        let identity = document.identity_document()?;
        let (path, hash) = associated_source(
            &input.source,
            identity,
            document.source()?.as_str(),
            original,
        )?;
        let empty = document.source()?.as_str().is_empty();
        let source_observation = match &input.source {
            NativeNnSource::Indexed(_) => AstGraphSourceObservation::IndexedStored { empty },
            NativeNnSource::Physical { .. } => {
                AstGraphSourceObservation::PhysicalTextNotStored { empty }
            }
        };
        let raw = match input.source {
            NativeNnSource::Indexed(raw) | NativeNnSource::Physical { raw, .. } => raw,
        };
        let observation = raw.branch_observation();
        let association = AstGraphAssociation {
            source_observation,
            path: path.clone(),
            relative_components: identity
                .relative_components()
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            raw_version: raw.version(),
            source_sha256: hash,
            physical_branch: observation
                .and_then(|o| o.physical_branch)
                .map(str::to_owned),
            inherited_target: observation
                .and_then(|o| o.inherited_target)
                .map(str::to_owned),
        };
        let parsed = document.parsed()?;
        let observations = parsed
            .context_document()?
            .observations()
            .map_err(TreeSitterModelError::from)?;
        let profile = observations.profile;
        let mut nodes = Vec::new();
        let mut diagnostics = Vec::new();
        for d in document.diagnostics()? {
            original.check()?;
            diagnostics.push(
                json!({"location":format!("{:?}",d.location),"issue":format!("{:?}",d.issue),
                "original":d.original.map(origin_json).transpose()?}),
            );
        }
        for e in document.entities()? {
            original.check()?;
            let uid = e.identity().uid().as_str();
            let key = serde_json::to_string(&[uid, e.data_key()])?;
            let mut properties = BTreeMap::new();
            for (name, value) in [
                ("uid", uid),
                ("data_key", e.data_key()),
                ("name", e.name()),
                ("raw_name", e.name_raw().text()),
                ("path", path.as_str()),
                ("lang", profile.language().value().as_str()),
                ("grammar", profile.grammar().value().as_str()),
            ] {
                properties.insert(name.to_owned(), AstGraphValue::String(value.to_owned()));
            }
            properties.insert(
                "raw_version".into(),
                AstGraphValue::String(raw.version().to_string()),
            );
            properties.insert(
                "source_sha256".into(),
                AstGraphValue::String(hash.iter().map(|b| format!("{b:02x}")).collect()),
            );
            properties.insert(
                "source_stored".into(),
                AstGraphValue::Bool(matches!(
                    source_observation,
                    AstGraphSourceObservation::IndexedStored { .. }
                )),
            );
            properties.insert("source_empty".into(), AstGraphValue::Bool(empty));
            properties.insert(
                "line_number".into(),
                AstGraphValue::Int64(line(e.name_raw().start_point().row)?),
            );
            properties.insert(
                "name_representation".into(),
                AstGraphValue::String(
                    match e.name_representation() {
                        TsNameRepresentation::Identifier => "Identifier",
                        TsNameRepresentation::LiteralSource { .. } => "LiteralSource",
                    }
                    .into(),
                ),
            );
            add_range(&mut properties, "name_", e.name_raw())?;
            let extent_status = match e.extent() {
                TsEntityExtent::Exact(extent) => {
                    add_range(&mut properties, "", extent.range())?;
                    "Exact"
                }
                TsEntityExtent::Missing => "Missing",
                TsEntityExtent::NonExact(_) => "NonExact",
                TsEntityExtent::Conflict(_) => "Conflict",
            };
            properties.insert(
                "extent_status".into(),
                AstGraphValue::String(extent_status.into()),
            );
            let available = extent_status == "Exact"
                && e.diagnostic_indices().is_empty()
                && e.conflicts().is_empty()
                && e.model_conflicts().is_empty()
                && e.namespaces()
                    .iter()
                    .all(|n| matches!(n.extent, ExtentObservation::Exact(_)));
            properties.insert("available".into(), AstGraphValue::Bool(available));
            let metadata = json!({"diagnostic_indices":e.diagnostic_indices(),"extent_conflicts":format!("{:?}",e.conflicts()),
                "model_conflicts":format!("{:?}",e.model_conflicts()),
                "contributions":e.contributions().iter().map(|c| {
                    let mut value = contribution_json(c)?;
                    let group = observations.groups.get(c.occurrence.query_index).ok_or(AstGraphError::NativeContract("contribution references foreign query"))?;
                    let matched = group.matches.get(c.occurrence.match_index).ok_or(AstGraphError::NativeContract("contribution references foreign match"))?;
                    value["query_policy"] = query_json(group.query.value());
                    value["query_capture_names"] = json!(group.capture_names());
                    value["query_properties"] = json!(matched.properties.iter().map(|p|json!({"key":p.key,"value":p.value,"capture_id":p.capture_id})).collect::<Vec<_>>());
                    Ok(value)
                }).collect::<Result<Vec<_>,AstGraphError>>()?});
            properties.insert(
                "metadata_json".into(),
                AstGraphValue::String(serde_json::to_string(&metadata)?),
            );
            if let Some(value) = association.physical_branch() {
                properties.insert(
                    "physical_branch".into(),
                    AstGraphValue::String(value.into()),
                );
            }
            if let Some(value) = association.inherited_target() {
                properties.insert(
                    "inherited_target".into(),
                    AstGraphValue::String(value.into()),
                );
            }
            nodes.push(AstGraphNode {
                label: e.identity().label().into(),
                key,
                properties,
            });
        }
        let mut edges = Vec::new();
        let mut edge_keys = BTreeSet::new();
        let mut outcomes = Vec::new();
        for (relation_index, r) in document.relations()?.iter().enumerate() {
            original.check()?;
            let mut captures = Vec::new();
            for (capture_index, capture) in r.matched.captures.iter().enumerate() {
                captures.push(json!({"capture_index":capture_index,"name":capture.native.name,
                    "prepared":range_json(capture.prepared)?,"original":origin_json(&capture.original)?}));
            }
            let mut sources = Vec::new();
            for source in &r.sources {
                sources.push(match &source.state {
                    TsRelationSourceState::Bound { container_node_index,entity_indices } => json!({"state":"Bound","container_node_index":container_node_index,"entity_keys":entity_indices.iter().map(|&i|node_key(&nodes,i)).collect::<Result<Vec<_>,_>>()?}),
                    TsRelationSourceState::MissingContainer => json!({"state":"MissingContainer"}),
                    TsRelationSourceState::UnavailableContainer { container_node_index } => json!({"state":"UnavailableContainer","container_node_index":container_node_index}),
                });
            }
            let observation = json!({"query_index":r.query_index,"match_index":r.match_index,
                "data_key":r.query.value().data_key().value(),"relation_type":r.query.value().relation_type().value(),
                "policy":query_json(r.query.value()),"captures":captures,"sources":sources,
                "target_rule":r.target_rule.map(|rule|json!({"labels":rule.value().labels().value().iter().map(|s|s.value()).collect::<Vec<_>>(),"fallback":rule.value().fallback().value()})),
                "properties":r.matched.properties.iter().map(|p|json!({"key":p.key,"value":p.value,"capture_id":p.capture_id})).collect::<Vec<_>>()});
            let observation_json = serde_json::to_string(&observation)?;
            let local: Vec<_> = document
                .local_relations()?
                .iter()
                .filter(|l| l.relation_index == relation_index)
                .collect();
            if local.is_empty() {
                if r.sources.is_empty() {
                    outcomes.push(AstGraphRelationOutcome {
                        relation_index,
                        source_index: None,
                        capture_index: None,
                        evidence_node_indices: Vec::new(),
                        target: AstGraphRelationTarget::ObservedOnly,
                        observation_json,
                    });
                } else {
                    for (source_index, source) in r.sources.iter().enumerate() {
                        original.check()?;
                        r.matched.captures.get(source.capture_index).ok_or(
                            AstGraphError::NativeContract(
                                "observed relation references foreign capture",
                            ),
                        )?;
                        let mut evidence_node_indices = vec![source.node_index];
                        match &source.state {
                            TsRelationSourceState::Bound {
                                container_node_index,
                                ..
                            }
                            | TsRelationSourceState::UnavailableContainer {
                                container_node_index,
                            } => {
                                evidence_node_indices.push(*container_node_index);
                            }
                            TsRelationSourceState::MissingContainer => {}
                        }
                        outcomes.push(AstGraphRelationOutcome {
                            relation_index,
                            source_index: Some(source_index),
                            capture_index: Some(source.capture_index),
                            evidence_node_indices,
                            target: AstGraphRelationTarget::ObservedOnly,
                            observation_json: observation_json.clone(),
                        });
                    }
                }
            } else {
                for l in local {
                    original.check()?;
                    let mut graph_edge_uids = Vec::new();
                    let target = match &l.target {
                        TsLocalTargetState::Resolved { entity_index } => {
                            let target_key = node_key(&nodes, *entity_index)?;
                            let source = r.sources.get(l.source_index).ok_or(
                                AstGraphError::NativeContract(
                                    "local result references foreign source",
                                ),
                            )?;
                            let TsRelationSourceState::Bound { entity_indices, .. } = &source.state
                            else {
                                return Err(AstGraphError::NativeContract(
                                    "resolved relation lacks genuine source",
                                ));
                            };
                            let kind = r.query.value().relation_type().value();
                            for &index in entity_indices {
                                let source_key = node_key(&nodes, index)?;
                                let capture = r.matched.captures.get(l.capture_index).ok_or(
                                    AstGraphError::NativeContract(
                                        "resolved relation references foreign capture",
                                    ),
                                )?;
                                let OriginalProjection::Exact(anchor) = &capture.original else {
                                    return Err(AstGraphError::NativeContract(
                                        "resolved relation lacks exact Original capture",
                                    ));
                                };
                                let uid = serde_json::to_string(&(
                                    kind,
                                    source_key,
                                    target_key,
                                    r.query.value().data_key().value(),
                                    anchor.start(),
                                    anchor.end(),
                                ))?;
                                graph_edge_uids.push(uid.clone());
                                if edge_keys.insert(uid.clone()) {
                                    edges.push(AstGraphEdge {
                                        kind: kind.clone(),
                                        source_key: source_key.into(),
                                        target_key: target_key.into(),
                                        uid,
                                    });
                                }
                            }
                            AstGraphRelationTarget::Resolved {
                                key: target_key.into(),
                            }
                        }
                        TsLocalTargetState::NotFound => AstGraphRelationTarget::NotFound,
                        TsLocalTargetState::Ambiguous { entity_indices } => {
                            AstGraphRelationTarget::Ambiguous {
                                keys: entity_indices
                                    .iter()
                                    .map(|&i| node_key(&nodes, i).map(str::to_owned))
                                    .collect::<Result<Vec<_>, _>>()?,
                            }
                        }
                        TsLocalTargetState::Unresolved { reason } => {
                            AstGraphRelationTarget::Unresolved {
                                reason: reason.clone(),
                            }
                        }
                    };
                    let mut outcome_observation = observation.clone();
                    outcome_observation["graph_edge_uids"] = json!(graph_edge_uids);
                    outcomes.push(AstGraphRelationOutcome {
                        relation_index,
                        source_index: Some(l.source_index),
                        capture_index: Some(l.capture_index),
                        evidence_node_indices: l.evidence_node_indices.clone(),
                        target,
                        observation_json: serde_json::to_string(&outcome_observation)?,
                    });
                }
            }
        }
        let diagnostics_json = serde_json::to_string(&diagnostics)?;
        let capabilities_json = serde_json::to_string(&document.capabilities()?.iter().map(|c|json!({"query_index":c.query_index,"policy":query_json(c.query.value()),"state":format!("{:?}",c.state)})).collect::<Vec<_>>())?;
        let file_observation = file_observation_node(
            &association,
            identity.language(),
            identity.grammar(),
            &outcomes,
            &diagnostics_json,
            &capabilities_json,
            original,
        )?;
        Ok(AstSemanticGraphBatch {
            file_observation,
            association,
            nodes,
            edges,
            outcomes,
            diagnostics_json,
            capabilities_json,
        })
    })();
    original.check()?;
    result
}

impl From<crate::tree_sitter_generic_model::GenericModelError> for AstGraphError {
    fn from(error: crate::tree_sitter_generic_model::GenericModelError) -> Self {
        Self::Generic(Box::new(error))
    }
}
