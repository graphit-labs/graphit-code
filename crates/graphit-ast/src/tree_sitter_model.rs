//! Rust and Go declarations assembled from one retained native context collection.

use crate::{
    RangeError, SourceRange, SourceText,
    catalog::{Effective, Query, TargetRule},
    entity_extent::{EntityExtent, EntityExtentError},
    prepared_input::{OriginError, OriginPiece, OriginalProjection, PreparedInput},
    profiles::{ExportStrategy, QueryKind},
    semantic_fields::{ContributionConflict, ContributionConflictField, ExtentObservation},
    symbols::{
        IdentityDocument, IdentityDocumentInput, IdentityError, IdentityLimits, SyntaxIdentity,
    },
    tree_sitter_context::{
        ContextError, ContextName, ContextRecovery, ContextualTreeSitterDocument,
        PreparedTreeSitterContexts,
    },
    tree_sitter_engine::SyntaxNode,
    tree_sitter_profile::{CaptureBinding, TreeSitterProfileCapture, TreeSitterProfileMatch},
};
use graphit_core::file_io::SnapshotContext;
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt, io,
    rc::Rc,
};
use syn::{ext::IdentExt, parse::Parser};

mod ecma;
mod go;
mod python;
mod value_rows;

#[derive(Debug)]
pub enum TreeSitterModelError {
    Operation(io::Error),
    Context(ContextError),
    Origin(OriginError),
    Identity(IdentityError),
    Extent(EntityExtentError),
    Range(RangeError),
    Policy(regex::Error),
    UnsupportedProfile,
    NativeContract(&'static str),
}
impl fmt::Display for TreeSitterModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(e) => e.fmt(f),
            Self::Context(e) => e.fmt(f),
            Self::Origin(e) => e.fmt(f),
            Self::Identity(e) => e.fmt(f),
            Self::Extent(e) => e.fmt(f),
            Self::Range(e) => e.fmt(f),
            Self::Policy(e) => e.fmt(f),
            Self::UnsupportedProfile => f.write_str(
                "semantic model requires a supported exact producing language/grammar pair",
            ),
            Self::NativeContract(e) => f.write_str(e),
        }
    }
}
impl Error for TreeSitterModelError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(e) => Some(e),
            Self::Context(e) => Some(e),
            Self::Origin(e) => Some(e),
            Self::Identity(e) => Some(e),
            Self::Extent(e) => Some(e),
            Self::Range(e) => Some(e),
            Self::Policy(e) => Some(e),
            _ => None,
        }
    }
}
macro_rules! convert_error {
    ($ty:ty,$variant:ident) => {
        impl From<$ty> for TreeSitterModelError {
            fn from(value: $ty) -> Self {
                Self::$variant(value)
            }
        }
    };
}
convert_error!(io::Error, Operation);
convert_error!(ContextError, Context);
convert_error!(OriginError, Origin);
convert_error!(IdentityError, Identity);
convert_error!(EntityExtentError, Extent);
convert_error!(RangeError, Range);
convert_error!(regex::Error, Policy);

struct NodeOrigin<'o, 'v> {
    prepared: SourceRange<'v>,
    original: OriginalProjection<'o>,
    recovery: ContextRecovery,
}

/// The sole factory binds a real input, its Original, and its retained native tree.
pub struct ParsedTreeSitterDocument<'profile, 'o, 'v, 'request> {
    context: ContextualTreeSitterDocument<'profile, 'o, 'v, 'request>,
    identity: IdentityDocument<'o, 'request>,
    source: SourceText<'o>,
    original: &'request SnapshotContext,
    node_indices: HashMap<usize, usize>,
    children: Vec<Vec<usize>>,
    origins: HashMap<usize, NodeOrigin<'o, 'v>>,
    scope_indices: HashMap<(usize, usize, usize), usize>,
    prepared_text: &'v str,
    signatures: HashMap<usize, NodeOrigin<'o, 'v>>,
}
impl<'profile: 'o, 'o, 'v, 'request> ParsedTreeSitterDocument<'profile, 'o, 'v, 'request> {
    pub fn collect(
        factory: &PreparedTreeSitterContexts<'profile>,
        input: &PreparedInput<'o, 'v, '_, 'request>,
        relative_components: &'o [&'o str],
        limits: IdentityLimits,
    ) -> Result<Self, TreeSitterModelError> {
        let original = input.original_context();
        original.check()?;
        let result = (|| {
            let context = factory.collect(input)?;
            let observations = context.observations()?;
            let profile = observations.profile;
            if !matches!(
                (
                    profile.language().value().as_str(),
                    profile.grammar().value().as_str()
                ),
                ("rust", "tree-sitter-rust")
                    | ("go", "tree-sitter-go")
                    | ("javascript", "tree-sitter-javascript")
                    | ("typescript", "tree-sitter-typescript")
                    | ("tsx", "tree-sitter-tsx")
                    | ("python", "tree-sitter-python")
            ) {
                return Err(TreeSitterModelError::UnsupportedProfile);
            }
            let source = SourceText::new(input.original_text().as_bytes(), limits.source_bytes())?;
            let identity = IdentityDocument::new(
                IdentityDocumentInput {
                    relative_components,
                    language: profile.language().value(),
                    grammar: profile.grammar().value(),
                    original_bytes: input.original_text().as_bytes(),
                },
                limits,
                original,
            )?;
            let nodes = &observations.nodes;
            let mut node_indices = HashMap::new();
            let mut children: Vec<Vec<usize>> = (0..nodes.len()).map(|_| Vec::new()).collect();
            let mut recovery = Vec::with_capacity(nodes.len());
            let mut wanted = HashSet::new();
            for (index, node) in nodes.iter().enumerate() {
                original.check()?;
                if node_indices.insert(node.node_id, index).is_some() {
                    return Err(TreeSitterModelError::NativeContract(
                        "duplicate native node identity",
                    ));
                }
                if let Some(parent) = node.parent {
                    if parent >= index || !contains_native(&nodes[parent], node) {
                        return Err(TreeSitterModelError::NativeContract(
                            "native ancestry is inconsistent",
                        ));
                    }
                    children[parent].push(index);
                }
                recovery.push(ContextRecovery {
                    error: node.error,
                    missing: node.missing,
                    contains_error: node.error,
                    contains_missing: node.missing,
                });
                if (node.kind == "{"
                    && node.parent.is_some_and(|p| {
                        nodes[p].field.as_deref() == Some("body")
                            && nodes[p].parent.is_some_and(|d| is_callable_node(&nodes[d]))
                    }))
                    || matches!(
                        node.kind.as_str(),
                        "struct_item"
                            | "field_declaration"
                            | "function_item"
                            | "function_signature_item"
                            | "visibility_modifier"
                            | "line_comment"
                            | "block_comment"
                            | "attribute_item"
                            | "inner_attribute_item"
                            | "default"
                            | "type_spec"
                            | "function_declaration"
                            | "method_declaration"
                            | "comment"
                    )
                    || node.parent.is_some_and(|p| {
                        matches!(
                            nodes[p].kind.as_str(),
                            "function_item"
                                | "function_signature_item"
                                | "function_declaration"
                                | "method_declaration"
                        ) || (matches!(
                            nodes[p].kind.as_str(),
                            "struct_item" | "field_declaration" | "type_spec"
                        ) && matches!(node.field.as_deref(), Some("name" | "type")))
                    })
                {
                    wanted.insert(index);
                }
            }
            if profile.language().value() == "go" {
                for (index, node) in nodes.iter().enumerate() {
                    original.check()?;
                    if matches!(
                        node.kind.as_str(),
                        "const_spec"
                            | "var_spec"
                            | "import_spec"
                            | "parameter_declaration"
                            | "variadic_parameter_declaration"
                            | "type_alias"
                            | "type_declaration"
                            | "const_declaration"
                            | "var_declaration"
                            | "func_literal"
                            | "type_identifier"
                            | "short_var_declaration"
                            | "type_parameter_declaration"
                    ) {
                        wanted.insert(index);
                    }
                    if let Some(parent) = node.parent {
                        let p = &nodes[parent];
                        if matches!(
                            p.kind.as_str(),
                            "const_spec"
                                | "var_spec"
                                | "import_spec"
                                | "parameter_declaration"
                                | "variadic_parameter_declaration"
                                | "type_alias"
                                | "type_parameter_declaration"
                        ) || (p.kind == "expression_list"
                            && p.parent.is_some_and(|d| {
                                matches!(
                                    nodes[d].kind.as_str(),
                                    "const_spec" | "var_spec" | "short_var_declaration"
                                )
                            }))
                            || (p.kind == "selector_expression"
                                && node.field.as_deref() == Some("operand"))
                        {
                            wanted.insert(index);
                        }
                    }
                }
                for group in &observations.groups {
                    for matched in &group.matches {
                        for capture in &matched.captures {
                            if let Some(index) = node_indices.get(&capture.native.node_id) {
                                wanted.insert(*index);
                            }
                        }
                    }
                }
            }
            if profile.language().value() == "rust" {
                for group in &observations.groups {
                    for matched in &group.matches {
                        for capture in &matched.captures {
                            original.check()?;
                            if let Some(index) = node_indices.get(&capture.native.node_id) {
                                wanted.insert(*index);
                            }
                        }
                    }
                }
            }
            // A Go comment's predecessor is also needed to validate Original attachment boundaries.
            if profile.language().value() == "go" {
                for siblings in &children {
                    for pair in siblings.windows(2) {
                        original.check()?;
                        if nodes[pair[1]].kind == "comment" && nodes[pair[0]].kind != "comment" {
                            wanted.insert(pair[0]);
                        }
                    }
                }
            }
            if profile.language().value() == "python"
                || ecma::is_ecma_language(profile.language().value())
            {
                for index in 0..nodes.len() {
                    original.check()?;
                    wanted.insert(index);
                }
            }
            // Compute subtree recovery from retained nodes; no SDK walk or parse.
            for index in (0..nodes.len()).rev() {
                original.check()?;
                if let Some(parent) = nodes[index].parent {
                    recovery[parent].contains_error |= recovery[index].contains_error;
                    recovery[parent].contains_missing |= recovery[index].contains_missing;
                }
            }
            for c in context.contexts()? {
                original.check()?;
                wanted.insert(c.trigger.node_index);
                if let Some(owner) = &c.owner {
                    wanted.insert(owner.node_index);
                }
                if let ContextName::Found { selected, .. } = &c.name {
                    wanted.insert(selected.node_index);
                }
            }
            let mut origins = HashMap::new();
            for index in wanted {
                original.check()?;
                let node = nodes
                    .get(index)
                    .ok_or(TreeSitterModelError::NativeContract(
                        "context references foreign node",
                    ))?;
                let start = u64::try_from(node.range.start)
                    .map_err(|_| TreeSitterModelError::NativeContract("native offset overflow"))?;
                let end = u64::try_from(node.range.end)
                    .map_err(|_| TreeSitterModelError::NativeContract("native offset overflow"))?;
                let prepared = input.prepared_span(start, end)?;
                if prepared.start_point() != node.start || prepared.end_point() != node.end {
                    return Err(TreeSitterModelError::NativeContract(
                        "native points differ from prepared source",
                    ));
                }
                origins.insert(
                    index,
                    NodeOrigin {
                        prepared,
                        original: input.project_prepared(start, end)?,
                        recovery: recovery[index],
                    },
                );
            }
            let mut signatures = HashMap::new();
            for (index, node) in nodes.iter().enumerate() {
                original.check()?;
                if !is_callable_node(node) {
                    continue;
                }
                let first = children[index].iter().find(|&&c| {
                    matches!(nodes[c].kind.as_str(), "fn" | "function_modifiers" | "func")
                });
                let last = children[index]
                    .iter()
                    .find(|&&c| nodes[c].field.as_deref() == Some("body") || nodes[c].kind == ";");
                let first = if profile.language().value() == "python"
                    || ecma::is_ecma_language(profile.language().value())
                {
                    children[index].first()
                } else {
                    first
                };
                if let Some(&first) = first
                    && (last.is_some()
                        || profile.language().value() == "go"
                        || (profile.language().value() == "python"
                            || ecma::is_ecma_language(profile.language().value())))
                {
                    let start = nodes[first].range.start as u64;
                    let end = last.map_or(node.range.end, |&last| nodes[last].range.start) as u64;
                    let mut header_recovery = ContextRecovery {
                        error: false,
                        missing: false,
                        contains_error: false,
                        contains_missing: false,
                    };
                    for &child in &children[index] {
                        original.check()?;
                        if nodes[child].range.start >= start as usize
                            && nodes[child].range.end <= end as usize
                            && Some(&child) != last
                        {
                            header_recovery.contains_error |= recovery[child].contains_error;
                            header_recovery.contains_missing |= recovery[child].contains_missing;
                        }
                    }
                    signatures.insert(
                        index,
                        NodeOrigin {
                            prepared: input.prepared_span(start, end)?,
                            original: input.project_prepared(start, end)?,
                            recovery: header_recovery,
                        },
                    );
                }
            }
            let mut scope_indices = HashMap::new();
            for (index, scope) in context.scopes()?.iter().enumerate() {
                original.check()?;
                if scope_indices
                    .insert(
                        (scope.query_index, scope.match_index, scope.capture_index),
                        index,
                    )
                    .is_some()
                {
                    return Err(TreeSitterModelError::NativeContract(
                        "duplicate capture scope address",
                    ));
                }
            }
            Ok(Self {
                context,
                identity,
                source,
                original,
                node_indices,
                children,
                origins,
                scope_indices,
                prepared_text: input.prepared_text(),
                signatures,
            })
        })();
        original.check()?;
        result
    }
    fn checked<T>(&self, value: T) -> Result<T, TreeSitterModelError> {
        self.original.check()?;
        let result = value;
        self.original.check()?;
        Ok(result)
    }
    pub fn source(&self) -> Result<SourceText<'o>, TreeSitterModelError> {
        self.checked(self.source.clone())
    }
    pub fn original_context(&self) -> Result<&'request SnapshotContext, TreeSitterModelError> {
        self.checked(self.original)
    }
    pub fn identity_document(
        &self,
    ) -> Result<&IdentityDocument<'o, 'request>, TreeSitterModelError> {
        self.checked(&self.identity)
    }
    pub fn context_document(
        &self,
    ) -> Result<&ContextualTreeSitterDocument<'profile, 'o, 'v, 'request>, TreeSitterModelError>
    {
        self.checked(&self.context)
    }
    fn node(&self, index: usize) -> Result<&SyntaxNode, TreeSitterModelError> {
        self.context
            .observations()?
            .nodes
            .get(index)
            .ok_or(TreeSitterModelError::NativeContract("foreign node index"))
    }
    fn origin(&self, index: usize) -> Result<&NodeOrigin<'o, 'v>, TreeSitterModelError> {
        self.original.check()?;
        self.origins
            .get(&index)
            .ok_or(TreeSitterModelError::NativeContract(
                "selected node lacks input projection",
            ))
    }
    fn field(&self, index: usize, field: &str) -> Result<Option<usize>, TreeSitterModelError> {
        let mut found = None;
        for &child in &self.children[index] {
            self.original.check()?;
            if self.node(child)?.field.as_deref() == Some(field) && found.replace(child).is_some() {
                return Err(TreeSitterModelError::NativeContract(
                    "duplicate singular native field",
                ));
            }
        }
        Ok(found)
    }
    fn captured_node(
        &self,
        capture: &TreeSitterProfileCapture<'o, 'v>,
    ) -> Result<usize, TreeSitterModelError> {
        self.original.check()?;
        let &index = self.node_indices.get(&capture.native.node_id).ok_or(
            TreeSitterModelError::NativeContract("capture belongs to another tree"),
        )?;
        let node = self.node(index)?;
        if node.range != capture.native.range
            || node.start != capture.native.start
            || node.end != capture.native.end
            || capture.prepared.start() != node.range.start
            || capture.prepared.end() != node.range.end
            || capture.prepared.start_point() != node.start
            || capture.prepared.end_point() != node.end
            || capture.native.text != capture.prepared.text()
        {
            return Err(TreeSitterModelError::NativeContract(
                "capture binding differs from native node",
            ));
        }
        Ok(index)
    }
}
fn is_callable_node(node: &SyntaxNode) -> bool {
    matches!(
        node.kind.as_str(),
        "function_item"
            | "function_signature_item"
            | "function_declaration"
            | "method_declaration"
            | "generator_function_declaration"
            | "method_definition"
            | "arrow_function"
            | "function_expression"
            | "generator_function"
            | "function_definition"
    )
}
fn contains_native(parent: &SyntaxNode, child: &SyntaxNode) -> bool {
    parent.range.start <= child.range.start && child.range.end <= parent.range.end
}
fn recovered(r: ContextRecovery) -> bool {
    r.error || r.missing || r.contains_error || r.contains_missing
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TsOccurrence {
    pub query_index: usize,
    pub match_index: usize,
    pub name_capture_index: Option<usize>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TsCapability {
    SupportedStruct,
    SupportedNamedStructField,
    SupportedCallable,
    SupportedNativeDeclaration,
    ObservedOnlyFamily,
    ObservedOnlyRelation,
    UnsupportedConfiguration(Vec<&'static str>),
    UnsupportedProfileNormalization,
}
pub struct TsQueryCapability<'profile> {
    pub query_index: usize,
    pub query: &'profile Effective<Query>,
    pub state: TsCapability,
}
#[derive(Debug)]
pub enum TsIssue {
    UnsupportedConfiguration(Vec<&'static str>),
    UnsupportedProfileNormalization,
    MissingCapture {
        role: &'static str,
        declared: bool,
    },
    UnsupportedDeclarationFamily {
        node_index: usize,
    },
    Recovery {
        role: &'static str,
        node_index: usize,
    },
    NonExact {
        role: &'static str,
        node_index: usize,
    },
    MissingOwner {
        context_index: usize,
    },
    UnsupportedOwnerName {
        node_index: usize,
    },
    InvalidOwner {
        context_index: usize,
    },
    InvalidSpan {
        capture_index: usize,
    },
    Identifier(syn::Error),
    EcmaIdentifier(crate::ecma_names::EcmaNameError),
    PythonIdentifier(crate::python_names::PythonNameError),
    DeclaredType(syn::Error),
    MissingDeclaredType,
    UnsupportedDeclaredType {
        node_index: usize,
    },
    EntityBindingConflict,
    ExcludedName {
        node_index: usize,
    },
    InvalidExplicitParent {
        capture_index: usize,
    },
    InvalidValueCapture {
        capture_index: usize,
    },
    UnprovenValueOwner {
        capture_index: usize,
        binding: TsValueBinding,
    },
    UnavailableValueOwner {
        capture_index: usize,
    },
    GoDeclaration {
        issue_index: usize,
    },
    EcmaDeclaration {
        issue_index: usize,
    },
    PythonDeclaration {
        issue_index: usize,
    },
    Callable {
        issue_index: usize,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TsDiagnosticLocation {
    Profile,
    Query { query_index: usize },
    Occurrence(TsOccurrence),
}
pub struct TsDiagnostic<'doc, 'o> {
    pub location: TsDiagnosticLocation,
    pub issue: TsIssue,
    pub original: Option<&'doc OriginalProjection<'o>>,
    declaration: Option<usize>,
    subject: Option<usize>,
    label: &'doc str,
    data_key: &'doc str,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TsOwnerSelection {
    ConfiguredIdentifier,
    ConfiguredRustImplType,
    DerivedDeclarationIdentifier,
    ConfiguredLiteralSource,
    DerivedDeclarationLiteralSource,
}
pub struct TsNamespace<'doc, 'o> {
    pub node_index: usize,
    pub context_index: Option<usize>,
    pub explicit_parent_capture_index: Option<usize>,
    pub label: String,
    pub name: String,
    pub anchor: SourceRange<'o>,
    pub extent: ExtentObservation<'doc, 'o>,
    pub selection: TsOwnerSelection,
}
pub struct TsTypeSyntax<'doc, 'o, 'v> {
    pub node_index: usize,
    pub prepared: SourceRange<'v>,
    pub original: &'doc OriginalProjection<'o>,
    pub raw: SourceRange<'o>,
    pub syntax: Box<syn::Type>,
}
#[derive(Clone, Copy)]
pub struct TsNativeSyntax<'doc, 'o, 'v> {
    pub node_index: usize,
    pub kind: &'doc str,
    pub prepared: SourceRange<'v>,
    pub original: &'doc OriginalProjection<'o>,
    pub raw: Option<SourceRange<'o>>,
}
pub struct TsPythonSignature<'doc, 'o, 'v> {
    pub parameters: Option<TsNativeSyntax<'doc, 'o, 'v>>,
    pub type_parameters: Option<TsNativeSyntax<'doc, 'o, 'v>>,
    pub return_type: Option<TsNativeSyntax<'doc, 'o, 'v>>,
}
pub struct TsPythonDeclarationDetails<'doc, 'o, 'v> {
    pub native_kind: &'doc str,
    pub export: TsExport,
    pub attachment_owner_node_index: usize,
    pub attachments: Vec<TsAttachment<'doc, 'o>>,
    pub leading_string: Option<TsNativeSyntax<'doc, 'o, 'v>>,
    pub issues: Vec<TsCallableIssue<'doc, 'o>>,
}
pub struct TsEcmaSignature<'doc, 'o, 'v> {
    pub parameters: Option<TsNativeSyntax<'doc, 'o, 'v>>,
    pub type_parameters: Option<TsNativeSyntax<'doc, 'o, 'v>>,
    pub return_type: Option<TsNativeSyntax<'doc, 'o, 'v>>,
}
pub struct TsEcmaDeclarationDetails<'doc, 'o> {
    pub native_kind: &'doc str,
    pub export: TsExport,
    pub attachment_owner_node_index: usize,
    pub attachments: Vec<TsAttachment<'doc, 'o>>,
    pub issues: Vec<TsCallableIssue<'doc, 'o>>,
}
pub struct TsGoSignature<'doc, 'o, 'v> {
    pub receiver: Option<TsNativeSyntax<'doc, 'o, 'v>>,
    pub type_parameters: Option<TsNativeSyntax<'doc, 'o, 'v>>,
    pub parameters: Option<TsNativeSyntax<'doc, 'o, 'v>>,
    pub result: Option<TsNativeSyntax<'doc, 'o, 'v>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TsNameRepresentation<'doc> {
    Identifier,
    LiteralSource { native_kind: &'doc str },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TsValueBinding {
    Subject { ordinal: usize },
    OtherSubject { ordinal: usize },
    UnprovenArity { subjects: usize, expressions: usize },
    Initializer,
    ExpressionPart { ordinal: usize },
    OutsideInitializer,
}
pub struct TsValueCapture<'doc, 'o, 'v> {
    pub capture_index: usize,
    pub syntax: TsNativeSyntax<'doc, 'o, 'v>,
    pub binding: TsValueBinding,
}
pub enum TsDeclaredValue<'doc, 'o, 'v> {
    NotConfigured,
    Missing {
        declared: bool,
    },
    Native {
        subject_ordinal: usize,
        subject_count: usize,
        initializer: Option<Box<TsNativeSyntax<'doc, 'o, 'v>>>,
        expressions: Vec<TsNativeSyntax<'doc, 'o, 'v>>,
        captures: Vec<TsValueCapture<'doc, 'o, 'v>>,
    },
}
pub enum TsEntityDeclaredValue<'a, 'doc, 'o, 'v> {
    NotConfigured,
    Missing {
        declared: bool,
    },
    Native {
        contributions: &'a [TsContribution<'doc, 'o, 'v>],
    },
    Conflict(&'a [TsContribution<'doc, 'o, 'v>]),
}
pub struct TsGoDeclarationDetails<'doc, 'o, 'v> {
    pub native_kind: &'doc str,
    pub alias: Option<TsNativeSyntax<'doc, 'o, 'v>>,
    pub export: TsExport,
    pub attachment_owner_node_index: usize,
    pub attachments: Vec<TsAttachment<'doc, 'o>>,
    pub issues: Vec<TsCallableIssue<'doc, 'o>>,
}

pub enum TsDeclaredType<'doc, 'o, 'v> {
    NotApplicable,
    Present(TsTypeSyntax<'doc, 'o, 'v>),
    Native(TsNativeSyntax<'doc, 'o, 'v>),
    Unavailable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TsCallableKind {
    Function,
    ImplMethod,
    TraitMethod,
    TraitSignature,
    ForeignFunction,
    MethodGo,
    MethodEcma,
    ArrowEcma,
    ExpressionEcma,
    GeneratorEcma,
}
pub struct TsSignature<'doc, 'o, 'v> {
    pub prepared: SourceRange<'v>,
    pub original: &'doc OriginalProjection<'o>,
    pub raw: Option<SourceRange<'o>>,
    pub syntax: Option<Box<syn::Signature>>,
    pub go: Option<TsGoSignature<'doc, 'o, 'v>>,
    pub ecma: Option<TsEcmaSignature<'doc, 'o, 'v>>,
    pub python: Option<TsPythonSignature<'doc, 'o, 'v>>,
}
pub struct TsVisibility<'doc, 'o> {
    pub original: Option<&'doc OriginalProjection<'o>>,
    pub raw: Option<SourceRange<'o>>,
    pub syntax: Option<syn::Visibility>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TsAttachmentPlacement {
    Outer,
    Inner,
}
pub enum TsAttachmentContent {
    Comment,
    Documentation(String),
    Attribute(Box<syn::Attribute>),
    Unavailable,
}
pub struct TsAttachment<'doc, 'o> {
    pub node_index: usize,
    pub original: &'doc OriginalProjection<'o>,
    pub raw: Option<SourceRange<'o>>,
    pub placement: TsAttachmentPlacement,
    pub content: TsAttachmentContent,
}
#[derive(Debug)]
pub enum TsCallableIssueKind {
    Signature(syn::Error),
    MissingField { role: &'static str },
    Visibility(syn::Error),
    Attachment(syn::Error),
    NonExact { role: &'static str },
    Recovery { role: &'static str },
    UnsupportedDocumentationExpression,
    UnsupportedExportPolicy,
    UnsupportedComplexityPolicy,
    InvalidAttachmentOrigin,
    DefaultnessMismatch,
}
pub struct TsCallableIssue<'doc, 'o> {
    pub node_index: usize,
    pub original: Option<&'doc OriginalProjection<'o>>,
    pub kind: TsCallableIssueKind,
}
#[derive(Debug, Eq, PartialEq)]
pub enum TsComplexity {
    Disabled,
    NoBody,
    Score {
        value: usize,
        body_node_index: usize,
        decision_node_indices: Vec<usize>,
    },
    Unavailable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TsExport {
    NotConfigured,
    Modifier(bool),
    CapitalizedName(bool),
    NamePrefix(bool),
    Statement(bool),
    Unavailable,
}
pub struct TsFunctionDetails<'doc, 'o, 'v> {
    pub declaration_node_index: usize,
    pub kind: TsCallableKind,
    pub body_node_index: Option<usize>,
    pub defaultness_node_index: Option<usize>,
    pub signature: TsSignature<'doc, 'o, 'v>,
    pub visibility: TsVisibility<'doc, 'o>,
    pub attachments: Vec<TsAttachment<'doc, 'o>>,
    pub complexity: TsComplexity,
    pub export: TsExport,
    pub issues: Vec<TsCallableIssue<'doc, 'o>>,
}
pub enum TsCallable<'doc, 'o, 'v> {
    NotApplicable,
    Function(Rc<TsFunctionDetails<'doc, 'o, 'v>>),
}
pub enum TsEntityCallable<'a, 'doc, 'o, 'v> {
    NotApplicable,
    Function(&'a TsFunctionDetails<'doc, 'o, 'v>),
    Conflict(&'a [TsContribution<'doc, 'o, 'v>]),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TsModelConflictField {
    DeclaredType,
    Callable,
    Value,
    GoDeclaration,
    EcmaDeclaration,
    PythonDeclaration,
}
pub enum TsRelationSourceState {
    Bound {
        container_node_index: usize,
        entity_indices: Vec<usize>,
    },
    UnavailableContainer {
        container_node_index: usize,
    },
    MissingContainer,
}
pub struct TsRelationSource {
    pub capture_index: usize,
    pub node_index: usize,
    pub state: TsRelationSourceState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TsLocalTargetState {
    Resolved { entity_index: usize },
    NotFound,
    Ambiguous { entity_indices: Vec<usize> },
    Unresolved { reason: TsLocalUnresolvedReason },
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TsLocalUnresolvedReason {
    DynamicReceiver { node_index: usize },
    MissingSource,
    UnavailableSource { node_index: usize },
    AnonymousSource { node_index: usize },
    MissingTargetRule,
    UnsupportedPolicy,
    ExcludedName,
    Recovery { node_index: usize },
    NonExact { node_index: usize },
    Shadowed { declaration_node_index: usize },
    TemporalDeadZone { declaration_node_index: usize },
    ExternalImport { declaration_node_index: usize },
    UnknownReceiver { node_index: usize },
    UnknownType { node_index: usize },
    PromotedMember { node_index: usize },
    NotReadAccess { node_index: usize },
    UnsupportedSyntax { node_index: usize },
}
pub struct TsLocalRelationResolution {
    pub relation_index: usize,
    pub source_index: usize,
    pub capture_index: usize,
    pub evidence_node_indices: Vec<usize>,
    pub target: TsLocalTargetState,
}

pub struct TsRelationObservation<'doc, 'profile, 'o, 'v> {
    pub query_index: usize,
    pub match_index: usize,
    pub query: &'profile Effective<Query>,
    pub matched: &'doc TreeSitterProfileMatch<'o, 'v>,
    pub target_rule: Option<&'profile Effective<TargetRule>>,
    pub sources: Vec<TsRelationSource>,
}
/// Provenance of a configured value row derived from an actual declaration.
pub struct TsDerivedValueSubject<'doc> {
    pub owner_uid: String,
    pub owner_label: &'doc str,
    pub owner_declaration_node_index: usize,
    pub owner_name_node_index: usize,
    pub owner_name_capture_index: usize,
    pub value_capture_index: usize,
    pub binding: TsValueBinding,
}
pub struct TsContribution<'doc, 'o, 'v> {
    pub occurrence: TsOccurrence,
    pub declaration_node_index: usize,
    pub container_node_index: Option<usize>,
    pub name_capture: &'doc TreeSitterProfileCapture<'o, 'v>,
    pub span_capture_indices: Vec<usize>,
    pub extent: ExtentObservation<'doc, 'o>,
    pub declared_type: TsDeclaredType<'doc, 'o, 'v>,
    pub callable: TsCallable<'doc, 'o, 'v>,
    pub declared_value: TsDeclaredValue<'doc, 'o, 'v>,
    pub derived_value: Option<TsDerivedValueSubject<'doc>>,
    pub go_declaration: Option<TsGoDeclarationDetails<'doc, 'o, 'v>>,
    pub ecma_declaration: Option<TsEcmaDeclarationDetails<'doc, 'o>>,
    pub python_declaration: Option<TsPythonDeclarationDetails<'doc, 'o, 'v>>,
    pub excluded_context_indices: Vec<usize>,
    pub namespaces: Vec<TsNamespace<'doc, 'o>>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TsModelConflict {
    pub field: TsModelConflictField,
    pub first_contribution: usize,
    pub second_contribution: usize,
}
pub struct TsEntity<'doc, 'o, 'v, 'request> {
    identity: SyntaxIdentity<'doc, 'o, 'request>,
    data_key: &'doc str,
    name: String,
    name_raw: SourceRange<'o>,
    name_representation: TsNameRepresentation<'doc>,
    contributions: Vec<TsContribution<'doc, 'o, 'v>>,
    conflicts: Vec<ContributionConflict>,
    model_conflicts: Vec<TsModelConflict>,
    diagnostic_indices: Vec<usize>,
}
pub enum TsEntityExtent<'a, 'doc, 'o, 'v> {
    Exact(&'a EntityExtent<'o>),
    NonExact(&'doc OriginalProjection<'o>),
    Missing,
    Conflict(&'a [TsContribution<'doc, 'o, 'v>]),
}
pub enum TsEntityDeclaredType<'a, 'doc, 'o, 'v> {
    NotApplicable,
    Present(&'a TsTypeSyntax<'doc, 'o, 'v>),
    Native(&'a TsNativeSyntax<'doc, 'o, 'v>),
    Unavailable,
    Conflict(&'a [TsContribution<'doc, 'o, 'v>]),
}
impl<'doc, 'o, 'v, 'request> TsEntity<'doc, 'o, 'v, 'request> {
    pub fn identity(&self) -> &SyntaxIdentity<'doc, 'o, 'request> {
        &self.identity
    }
    pub fn data_key(&self) -> &'doc str {
        self.data_key
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn name_raw(&self) -> SourceRange<'o> {
        self.name_raw
    }
    pub fn name_representation(&self) -> TsNameRepresentation<'doc> {
        self.name_representation
    }
    pub fn declared_value(&self) -> TsEntityDeclaredValue<'_, 'doc, 'o, 'v> {
        if self
            .model_conflicts
            .iter()
            .any(|c| c.field == TsModelConflictField::Value)
        {
            return TsEntityDeclaredValue::Conflict(&self.contributions);
        }
        if self
            .contributions
            .iter()
            .any(|c| matches!(c.declared_value, TsDeclaredValue::Native { .. }))
        {
            return TsEntityDeclaredValue::Native {
                contributions: &self.contributions,
            };
        }
        for c in &self.contributions {
            if let TsDeclaredValue::Missing { declared } = c.declared_value {
                return TsEntityDeclaredValue::Missing { declared };
            }
        }
        TsEntityDeclaredValue::NotConfigured
    }
    pub fn namespaces(&self) -> &[TsNamespace<'doc, 'o>] {
        &self.contributions[0].namespaces
    }
    pub fn contributions(&self) -> &[TsContribution<'doc, 'o, 'v>] {
        &self.contributions
    }
    pub fn conflicts(&self) -> &[ContributionConflict] {
        &self.conflicts
    }
    pub fn model_conflicts(&self) -> &[TsModelConflict] {
        &self.model_conflicts
    }
    pub fn diagnostic_indices(&self) -> &[usize] {
        &self.diagnostic_indices
    }
    pub fn extent(&self) -> TsEntityExtent<'_, 'doc, 'o, 'v> {
        if !self.conflicts.is_empty() {
            return TsEntityExtent::Conflict(&self.contributions);
        }
        for c in &self.contributions {
            match &c.extent {
                ExtentObservation::Missing => return TsEntityExtent::Missing,
                ExtentObservation::NonExact(p) => return TsEntityExtent::NonExact(p),
                ExtentObservation::Exact(_) => {}
            }
        }
        match &self.contributions[0].extent {
            ExtentObservation::Exact(e) => TsEntityExtent::Exact(e),
            ExtentObservation::NonExact(p) => TsEntityExtent::NonExact(p),
            ExtentObservation::Missing => TsEntityExtent::Missing,
        }
    }
    pub fn callable(&self) -> TsEntityCallable<'_, 'doc, 'o, 'v> {
        if self
            .model_conflicts
            .iter()
            .any(|c| c.field == TsModelConflictField::Callable)
        {
            return TsEntityCallable::Conflict(&self.contributions);
        }
        match &self.contributions[0].callable {
            TsCallable::NotApplicable => TsEntityCallable::NotApplicable,
            TsCallable::Function(details) => TsEntityCallable::Function(details),
        }
    }
    pub fn declared_type(&self) -> TsEntityDeclaredType<'_, 'doc, 'o, 'v> {
        if self
            .model_conflicts
            .iter()
            .any(|c| c.field == TsModelConflictField::DeclaredType)
        {
            return TsEntityDeclaredType::Conflict(&self.contributions);
        }
        for c in &self.contributions {
            if matches!(c.declared_type, TsDeclaredType::Unavailable) {
                return TsEntityDeclaredType::Unavailable;
            }
        }
        match &self.contributions[0].declared_type {
            TsDeclaredType::NotApplicable => TsEntityDeclaredType::NotApplicable,
            TsDeclaredType::Unavailable => TsEntityDeclaredType::Unavailable,
            TsDeclaredType::Present(t) => TsEntityDeclaredType::Present(t),
            TsDeclaredType::Native(t) => TsEntityDeclaredType::Native(t),
        }
    }
}
pub struct TreeSitterSemanticDocument<'doc, 'profile, 'o, 'v, 'request> {
    parsed: &'doc ParsedTreeSitterDocument<'profile, 'o, 'v, 'request>,
    entities: Vec<TsEntity<'doc, 'o, 'v, 'request>>,
    diagnostics: Vec<TsDiagnostic<'doc, 'o>>,
    capabilities: Vec<TsQueryCapability<'profile>>,
    relations: Vec<TsRelationObservation<'doc, 'profile, 'o, 'v>>,
    local_relations: Vec<TsLocalRelationResolution>,
}
pub struct TsCodeProjection<'a, 'doc, 'o, 'v, 'request> {
    pub entity_index: usize,
    pub entity: &'a TsEntity<'doc, 'o, 'v, 'request>,
    pub extent: &'a EntityExtent<'o>,
    pub text: &'o str,
    pub language: &'a str,
    pub grammar: &'a str,
}
pub struct TsUnavailableProjection<'a> {
    pub entity_index: usize,
    pub diagnostic_indices: &'a [usize],
    pub conflicts: &'a [ContributionConflict],
    pub model_conflicts: &'a [TsModelConflict],
}
pub enum TsProjection<'a, 'doc, 'o, 'v, 'request> {
    Available(TsCodeProjection<'a, 'doc, 'o, 'v, 'request>),
    Unavailable(TsUnavailableProjection<'a>),
}

impl<'doc, 'profile: 'o, 'o, 'v, 'request>
    TreeSitterSemanticDocument<'doc, 'profile, 'o, 'v, 'request>
{
    pub fn parsed(
        &self,
    ) -> Result<&'doc ParsedTreeSitterDocument<'profile, 'o, 'v, 'request>, TreeSitterModelError>
    {
        self.parsed.checked(self.parsed)
    }
    pub fn source(&self) -> Result<SourceText<'o>, TreeSitterModelError> {
        self.parsed.source()
    }
    pub fn original_context(&self) -> Result<&'request SnapshotContext, TreeSitterModelError> {
        self.parsed.original_context()
    }
    pub fn identity_document(
        &self,
    ) -> Result<&'doc IdentityDocument<'o, 'request>, TreeSitterModelError> {
        self.parsed.identity_document()
    }
    pub fn entities(&self) -> Result<&[TsEntity<'doc, 'o, 'v, 'request>], TreeSitterModelError> {
        self.parsed.checked(self.entities.as_slice())
    }
    pub fn relations(
        &self,
    ) -> Result<&[TsRelationObservation<'doc, 'profile, 'o, 'v>], TreeSitterModelError> {
        self.parsed.checked(self.relations.as_slice())
    }
    pub fn local_relations(&self) -> Result<&[TsLocalRelationResolution], TreeSitterModelError> {
        self.parsed.checked(self.local_relations.as_slice())
    }
    pub fn diagnostics(&self) -> Result<&[TsDiagnostic<'doc, 'o>], TreeSitterModelError> {
        self.parsed.checked(self.diagnostics.as_slice())
    }
    pub fn capabilities(&self) -> Result<&[TsQueryCapability<'profile>], TreeSitterModelError> {
        self.parsed.checked(self.capabilities.as_slice())
    }
    pub fn code_projections(
        &self,
    ) -> Result<Vec<TsProjection<'_, 'doc, 'o, 'v, 'request>>, TreeSitterModelError> {
        self.parsed.original.check()?;
        let result = (|| {
            let profile = self.parsed.context.observations()?.profile;
            let mut result = Vec::new();
            for (entity_index, entity) in self.entities.iter().enumerate() {
                self.parsed.original.check()?;
                if !profile
                    .embed_labels()
                    .value()
                    .iter()
                    .any(|l| l.value() == entity.identity.label())
                {
                    continue;
                }
                if entity.diagnostic_indices.is_empty()
                    && entity.conflicts.is_empty()
                    && entity.model_conflicts.is_empty()
                    && entity
                        .namespaces()
                        .iter()
                        .all(|n| matches!(n.extent, ExtentObservation::Exact(_)))
                    && let TsEntityExtent::Exact(extent) = entity.extent()
                {
                    result.push(TsProjection::Available(TsCodeProjection {
                        entity_index,
                        entity,
                        extent,
                        text: extent.range().text(),
                        language: profile.language().value(),
                        grammar: profile.grammar().value(),
                    }));
                } else {
                    result.push(TsProjection::Unavailable(TsUnavailableProjection {
                        entity_index,
                        diagnostic_indices: &entity.diagnostic_indices,
                        conflicts: &entity.conflicts,
                        model_conflicts: &entity.model_conflicts,
                    }));
                }
            }
            Ok(result)
        })();
        self.parsed.original.check()?;
        result
    }
}
fn query_capability(query: &Query, go: bool) -> TsCapability {
    if !query.context_capture().value().is_empty() {
        return TsCapability::UnsupportedConfiguration(vec!["context_capture"]);
    }
    if *query.r#type().value() == QueryKind::Relation {
        return TsCapability::ObservedOnlyRelation;
    }
    let state = match query.graph_label().value().as_str() {
        "Struct" => TsCapability::SupportedStruct,
        "Field" => TsCapability::SupportedNamedStructField,
        "Function" | "Method" => TsCapability::SupportedCallable,
        _ if go => TsCapability::SupportedNativeDeclaration,
        _ => return TsCapability::ObservedOnlyFamily,
    };
    let mut fields = Vec::new();
    for (name, value) in [
        ("name_reject", query.name_reject().value()),
        ("declaration_capture", query.declaration_capture().value()),
        ("parent_capture", query.parent_capture().value()),
        ("parent_label", query.parent_label().value()),
        ("qualifier_capture", query.qualifier_capture().value()),
        ("target_fallback", query.target_fallback().value()),
        ("relation_type", query.relation_type().value()),
        ("value_capture", query.value_capture().value()),
        ("value_label", query.value_label().value()),
    ] {
        if !value.is_empty()
            && !(go
                && matches!(
                    name,
                    "name_reject"
                        | "parent_capture"
                        | "parent_label"
                        | "value_capture"
                        | "value_label"
                ))
        {
            fields.push(name);
        }
    }
    if !go && *query.name_is_data().value() {
        fields.push("name_is_data");
    }
    if !query.target_labels().value().is_empty() {
        fields.push("target_labels");
    }
    if fields.is_empty() {
        state
    } else {
        TsCapability::UnsupportedConfiguration(fields)
    }
}
impl<'profile: 'o, 'o, 'v, 'request> ParsedTreeSitterDocument<'profile, 'o, 'v, 'request> {
    pub fn assemble(
        &self,
    ) -> Result<TreeSitterSemanticDocument<'_, 'profile, 'o, 'v, 'request>, TreeSitterModelError>
    {
        self.original.check()?;
        let result = (|| {
            let obs = self.context.observations()?;
            let mut capabilities = Vec::new();
            let mut assembly = Assembly {
                parsed: self,
                entities: Vec::new(),
                diagnostics: Vec::new(),
                entity_indices: HashMap::new(),
                native_entities: HashMap::new(),
                functions: HashMap::new(),
                name_reject: HashMap::new(),
            };
            if obs.profile.language().value() == "python"
                || ecma::is_ecma_language(obs.profile.language().value())
            {
                for (node_index, node) in obs.nodes.iter().enumerate() {
                    self.original.check()?;
                    if node.error || node.missing {
                        assembly.diagnostics.push(TsDiagnostic {
                            location: TsDiagnosticLocation::Profile,
                            issue: TsIssue::Recovery {
                                role: "syntax",
                                node_index,
                            },
                            original: Some(&self.origin(node_index)?.original),
                            declaration: None,
                            subject: None,
                            label: "",
                            data_key: "",
                        });
                    }
                }
            }
            let normalized = !obs.profile.text_normalizers().value().is_empty();
            if normalized {
                assembly.configuration_issue(
                    TsDiagnosticLocation::Profile,
                    TsIssue::UnsupportedProfileNormalization,
                );
            }
            for (query_index, group) in obs.groups.iter().enumerate() {
                self.original.check()?;
                let state = if normalized {
                    TsCapability::UnsupportedProfileNormalization
                } else {
                    query_capability(
                        group.query.value(),
                        obs.profile.language().value() == "go"
                            || obs.profile.language().value() == "python"
                            || ecma::is_ecma_language(obs.profile.language().value()),
                    )
                };
                if group.matches.is_empty()
                    && let TsCapability::UnsupportedConfiguration(fields) = &state
                {
                    assembly.configuration_issue(
                        TsDiagnosticLocation::Query { query_index },
                        TsIssue::UnsupportedConfiguration(fields.clone()),
                    );
                }
                if (obs.profile.language().value() == "go"
                    || obs.profile.language().value() == "python"
                    || ecma::is_ecma_language(obs.profile.language().value()))
                    && !group.query.value().name_reject().value().is_empty()
                {
                    assembly.name_reject.insert(
                        query_index,
                        regex::Regex::new(group.query.value().name_reject().value())?,
                    );
                }
                for match_index in 0..group.matches.len() {
                    assembly.observe(query_index, match_index, &state)?;
                }
                capabilities.push(TsQueryCapability {
                    query_index,
                    query: group.query,
                    state,
                });
            }
            // Attach causes even when the malformed contribution appeared before the valid one.
            for (index, diagnostic) in assembly.diagnostics.iter().enumerate() {
                self.original.check()?;
                if matches!(diagnostic.issue, TsIssue::ExcludedName { .. }) {
                    continue;
                }
                if let Some(declaration) = diagnostic.declaration
                    && let Some(subjects) = assembly.native_entities.get(&(
                        declaration,
                        diagnostic.label,
                        diagnostic.data_key,
                    ))
                {
                    for &(subject, entity_index) in subjects {
                        if diagnostic.subject.is_none_or(|s| s == subject) {
                            assembly.entities[entity_index]
                                .diagnostic_indices
                                .push(index);
                        }
                    }
                }
            }
            let relations = assembly.relations()?;
            let local_relations = if obs.profile.language().value() == "python" {
                assembly.python_local_relations(&relations)?
            } else if ecma::is_ecma_language(obs.profile.language().value()) {
                assembly.ecma_local_relations(&relations)?
            } else if obs.profile.language().value() == "go" {
                assembly.go_local_relations(&relations)?
            } else {
                Vec::new()
            };
            Ok(TreeSitterSemanticDocument {
                parsed: self,
                entities: assembly.entities,
                diagnostics: assembly.diagnostics,
                capabilities,
                relations,
                local_relations,
            })
        })();
        self.original.check()?;
        result
    }
}
#[derive(Clone, Copy)]
struct Target<'doc> {
    occurrence: TsOccurrence,
    declaration: Option<usize>,
    subject: Option<usize>,
    label: &'doc str,
    data_key: &'doc str,
}
type NativeEntitySubjects<'doc> = HashMap<(usize, &'doc str, &'doc str), Vec<(usize, usize)>>;
struct Assembly<'doc, 'profile, 'o, 'v, 'request> {
    parsed: &'doc ParsedTreeSitterDocument<'profile, 'o, 'v, 'request>,
    entities: Vec<TsEntity<'doc, 'o, 'v, 'request>>,
    diagnostics: Vec<TsDiagnostic<'doc, 'o>>,
    entity_indices: HashMap<(String, &'doc str), usize>,
    native_entities: NativeEntitySubjects<'doc>,
    functions: HashMap<usize, Rc<TsFunctionDetails<'doc, 'o, 'v>>>,
    name_reject: HashMap<usize, regex::Regex>,
}
struct EntitySeed<'doc, 'o, 'request> {
    identity: SyntaxIdentity<'doc, 'o, 'request>,
    name: String,
    raw: SourceRange<'o>,
    name_representation: TsNameRepresentation<'doc>,
    namespaces: Vec<TsNamespace<'doc, 'o>>,
    excluded_context_indices: Vec<usize>,
}
impl<'doc, 'profile: 'o, 'o, 'v, 'request> Assembly<'doc, 'profile, 'o, 'v, 'request> {
    fn issue(
        &mut self,
        target: Target<'doc>,
        issue: TsIssue,
        original: Option<&'doc OriginalProjection<'o>>,
    ) {
        self.diagnostics.push(TsDiagnostic {
            location: TsDiagnosticLocation::Occurrence(target.occurrence),
            issue,
            original,
            declaration: target.declaration,
            subject: target.subject,
            label: target.label,
            data_key: target.data_key,
        });
    }
    fn configuration_issue(&mut self, location: TsDiagnosticLocation, issue: TsIssue) {
        self.diagnostics.push(TsDiagnostic {
            location,
            issue,
            original: None,
            declaration: None,
            subject: None,
            label: "",
            data_key: "",
        });
    }
    fn span_owner(
        &self,
        span_node: usize,
        declarations: &HashSet<usize>,
    ) -> Result<Option<usize>, TreeSitterModelError> {
        let mut cursor = Some(span_node);
        while let Some(index) = cursor {
            self.parsed.original.check()?;
            if declarations.contains(&index) {
                return Ok(Some(index));
            }
            cursor = self.parsed.node(index)?.parent;
        }
        Ok(None)
    }
    fn declaration(
        &self,
        name: usize,
        label: &str,
    ) -> Result<Option<(usize, Option<usize>)>, TreeSitterModelError> {
        if self.is_python()? {
            return self.python_declaration(name, label);
        }
        if self.is_ecma()? {
            return self.ecma_declaration(name, label);
        }
        if self.is_go()? {
            return self.go_declaration(name, label);
        }
        let node = self.parsed.node(name)?;
        if node.field.as_deref() != Some("name") {
            return Ok(None);
        }
        let Some(parent) = node.parent else {
            return Ok(None);
        };
        let decl = self.parsed.node(parent)?;
        match label {
            "Struct" if node.kind == "type_identifier" && decl.kind == "struct_item" => {
                Ok(Some((parent, None)))
            }
            "Field" if node.kind == "field_identifier" && decl.kind == "field_declaration" => {
                if let Some(list) = decl.parent
                    && self.parsed.node(list)?.kind == "field_declaration_list"
                    && let Some(container) = self.parsed.node(list)?.parent
                    && self.parsed.node(container)?.kind == "struct_item"
                {
                    Ok(Some((parent, Some(container))))
                } else {
                    Ok(None)
                }
            }
            "Function" | "Method" if node.kind == "identifier" => {
                if let Some(kind) = self.callable_kind(parent)? {
                    if label == "Method"
                        && !matches!(
                            kind,
                            TsCallableKind::ImplMethod
                                | TsCallableKind::TraitMethod
                                | TsCallableKind::TraitSignature
                        )
                    {
                        return Ok(None);
                    }
                    let container = if let Some(p) = decl.parent {
                        let list = self.parsed.node(p)?;
                        if list.kind == "declaration_list" {
                            list.parent
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    Ok(Some((parent, container)))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }
    fn native_ancestor(
        &self,
        ancestor: usize,
        descendant: usize,
    ) -> Result<bool, TreeSitterModelError> {
        let mut cursor = Some(descendant);
        while let Some(index) = cursor {
            self.parsed.original.check()?;
            if index == ancestor {
                return Ok(true);
            }
            cursor = self.parsed.node(index)?.parent;
        }
        Ok(false)
    }
    fn extent(
        &mut self,
        target: Target<'doc>,
        node: usize,
        role: &'static str,
    ) -> Result<ExtentObservation<'doc, 'o>, TreeSitterModelError> {
        let origin = self.parsed.origin(node)?;
        if recovered(origin.recovery) {
            self.issue(
                target,
                TsIssue::Recovery {
                    role,
                    node_index: node,
                },
                Some(&origin.original),
            );
        }
        match &origin.original {
            OriginalProjection::Exact(range) if range.start() != range.end() => {
                Ok(ExtentObservation::Exact(EntityExtent::new(
                    &self.parsed.source,
                    *range,
                    self.parsed.original,
                )?))
            }
            _ => {
                self.issue(
                    target,
                    TsIssue::NonExact {
                        role,
                        node_index: node,
                    },
                    Some(&origin.original),
                );
                Ok(ExtentObservation::NonExact(&origin.original))
            }
        }
    }
    fn seed(
        &mut self,
        target: Target<'doc>,
        name_node: usize,
        raw: SourceRange<'o>,
    ) -> Result<Option<EntitySeed<'doc, 'o, 'request>>, TreeSitterModelError> {
        self.parsed.original.check()?;
        if self.is_python()? {
            return self.python_seed(target, name_node, raw);
        }
        if self.is_ecma()? {
            return self.ecma_seed(target, name_node, raw);
        }
        let name = if self.is_go()? {
            Ok(raw.text().to_owned())
        } else {
            syn::Ident::parse_any
                .parse_str(raw.text())
                .map(|i| i.unraw().to_string())
        };
        self.parsed.original.check()?;
        let name = match name {
            Ok(n) => n,
            Err(e) => {
                self.issue(
                    target,
                    TsIssue::Identifier(e),
                    Some(&self.parsed.origin(name_node)?.original),
                );
                return Ok(None);
            }
        };
        let key =
            (
                target.occurrence.query_index,
                target.occurrence.match_index,
                target.occurrence.name_capture_index.ok_or(
                    TreeSitterModelError::NativeContract("name lacks capture address"),
                )?,
            );
        let scope_index =
            *self
                .parsed
                .scope_indices
                .get(&key)
                .ok_or(TreeSitterModelError::NativeContract(
                    "native capture lacks scope",
                ))?;
        let scope = &self.parsed.context.scopes()?[scope_index];
        if !scope.unresolved.is_empty() {
            for &context_index in &scope.unresolved {
                self.parsed.original.check()?;
                self.issue(target, TsIssue::MissingOwner { context_index }, None);
            }
            return Ok(None);
        }
        let contexts = self.parsed.context.contexts()?;
        let mut symbols = Vec::new();
        let mut namespaces = Vec::new();
        let mut outer_owner = None;
        for &context_index in scope.contexts.iter().rev() {
            self.parsed.original.check()?;
            let context = &contexts[context_index];
            if self.is_go()?
                && context
                    .owner
                    .as_ref()
                    .is_some_and(|owner| target.declaration == Some(owner.node_index))
            {
                continue;
            }
            let (Some(owner), ContextName::Found { selected, .. }) =
                (&context.owner, &context.name)
            else {
                self.issue(target, TsIssue::MissingOwner { context_index }, None);
                return Ok(None);
            };
            let invalid_outer = match outer_owner {
                Some(outer) => !self.native_ancestor(outer, owner.node_index)?,
                None => false,
            };
            if !self.native_ancestor(owner.node_index, name_node)?
                || !self.native_ancestor(owner.node_index, selected.node_index)?
                || invalid_outer
            {
                self.issue(
                    target,
                    TsIssue::InvalidOwner { context_index },
                    Some(&selected.original),
                );
                return Ok(None);
            }
            let origin = self.parsed.origin(selected.node_index)?;
            let OriginalProjection::Exact(anchor) = &origin.original else {
                self.issue(
                    target,
                    TsIssue::NonExact {
                        role: "owner_name",
                        node_index: selected.node_index,
                    },
                    Some(&origin.original),
                );
                return Ok(None);
            };
            if recovered(origin.recovery) || anchor.start() == anchor.end() {
                self.issue(
                    target,
                    TsIssue::Recovery {
                        role: "owner_name",
                        node_index: selected.node_index,
                    },
                    Some(&origin.original),
                );
                return Ok(None);
            }
            let node = self.parsed.node(selected.node_index)?;
            let (owner_name, selection) = if self.is_go()?
                && matches!(
                    node.kind.as_str(),
                    "identifier" | "type_identifier" | "field_identifier"
                ) {
                (
                    anchor.text().to_owned(),
                    TsOwnerSelection::ConfiguredIdentifier,
                )
            } else if self.parsed.node(owner.node_index)?.kind == "impl_item"
                && self.parsed.field(owner.node_index, "type")? == Some(selected.node_index)
            {
                self.parsed.original.check()?;
                let decoded = syn::parse_str::<syn::Type>(anchor.text());
                self.parsed.original.check()?;
                match decoded {
                    Ok(syn::Type::Verbatim(_)) => {
                        self.issue(
                            target,
                            TsIssue::UnsupportedOwnerName {
                                node_index: selected.node_index,
                            },
                            Some(&origin.original),
                        );
                        return Ok(None);
                    }
                    Ok(_) => (
                        anchor.text().to_owned(),
                        TsOwnerSelection::ConfiguredRustImplType,
                    ),
                    Err(e) => {
                        self.issue(target, TsIssue::DeclaredType(e), Some(&origin.original));
                        return Ok(None);
                    }
                }
            } else if matches!(
                node.kind.as_str(),
                "identifier" | "type_identifier" | "field_identifier"
            ) {
                self.parsed.original.check()?;
                let decoded = syn::Ident::parse_any
                    .parse_str(anchor.text())
                    .map(|i| i.unraw().to_string());
                self.parsed.original.check()?;
                match decoded {
                    Ok(n) => (n, TsOwnerSelection::ConfiguredIdentifier),
                    Err(e) => {
                        self.issue(target, TsIssue::Identifier(e), Some(&origin.original));
                        return Ok(None);
                    }
                }
            } else {
                self.issue(
                    target,
                    TsIssue::UnsupportedOwnerName {
                        node_index: selected.node_index,
                    },
                    Some(&origin.original),
                );
                return Ok(None);
            };
            let extent = self.extent(target, owner.node_index, "owner")?;
            if let ExtentObservation::Exact(e) = &extent {
                let r = e.range();
                if anchor.start() < r.start()
                    || anchor.end() > r.end()
                    || raw.start() < r.start()
                    || raw.end() > r.end()
                {
                    self.issue(
                        target,
                        TsIssue::InvalidOwner { context_index },
                        Some(&origin.original),
                    );
                    return Ok(None);
                }
            }
            let references: Vec<_> = symbols.iter().collect();
            symbols.push(
                self.parsed
                    .identity
                    .symbol(&context.label, *anchor, &references)?,
            );
            namespaces.push(TsNamespace {
                node_index: owner.node_index,
                context_index: Some(context_index),
                explicit_parent_capture_index: None,
                label: context.label.clone(),
                name: owner_name,
                anchor: *anchor,
                extent,
                selection,
            });
            outer_owner = Some(owner.node_index);
        }
        let references: Vec<_> = symbols.iter().collect();
        let identity = self
            .parsed
            .identity
            .symbol(target.label, raw, &references)?;
        Ok(Some(EntitySeed {
            identity,
            name,
            raw,
            name_representation: if self.is_go()?
                && matches!(
                    self.parsed.node(name_node)?.kind.as_str(),
                    "interpreted_string_literal" | "raw_string_literal"
                ) {
                TsNameRepresentation::LiteralSource {
                    native_kind: self.parsed.node(name_node)?.kind.as_str(),
                }
            } else {
                TsNameRepresentation::Identifier
            },
            namespaces,
            excluded_context_indices: Vec::new(),
        }))
    }
    fn declared_type(
        &mut self,
        target: Target<'doc>,
        declaration: usize,
    ) -> Result<TsDeclaredType<'doc, 'o, 'v>, TreeSitterModelError> {
        if self.is_python()? {
            return self.python_declared_type(target, declaration);
        }
        if self.is_ecma()? {
            return self.ecma_declared_type(target, declaration);
        }
        if self.is_go()? {
            return self.go_declared_type(target, declaration);
        }
        if target.label != "Field" {
            return Ok(TsDeclaredType::NotApplicable);
        }
        let Some(node_index) = self.parsed.field(declaration, "type")? else {
            self.issue(target, TsIssue::MissingDeclaredType, None);
            return Ok(TsDeclaredType::Unavailable);
        };
        let origin = self.parsed.origin(node_index)?;
        if recovered(origin.recovery) {
            self.issue(
                target,
                TsIssue::Recovery {
                    role: "type",
                    node_index,
                },
                Some(&origin.original),
            );
            return Ok(TsDeclaredType::Unavailable);
        }
        let OriginalProjection::Exact(raw) = &origin.original else {
            self.issue(
                target,
                TsIssue::NonExact {
                    role: "type",
                    node_index,
                },
                Some(&origin.original),
            );
            return Ok(TsDeclaredType::Unavailable);
        };
        if self.is_go()? {
            return Ok(TsDeclaredType::Native(self.go_native(node_index)?));
        }
        self.parsed.original.check()?;
        let syntax = syn::parse_str::<syn::Type>(raw.text());
        self.parsed.original.check()?;
        match syntax {
            Ok(syn::Type::Verbatim(_)) => {
                self.issue(
                    target,
                    TsIssue::UnsupportedDeclaredType { node_index },
                    Some(&origin.original),
                );
                Ok(TsDeclaredType::Unavailable)
            }
            Ok(syntax) => Ok(TsDeclaredType::Present(TsTypeSyntax {
                node_index,
                prepared: origin.prepared,
                original: &origin.original,
                raw: *raw,
                syntax: Box::new(syntax),
            })),
            Err(e) => {
                self.issue(target, TsIssue::DeclaredType(e), Some(&origin.original));
                Ok(TsDeclaredType::Unavailable)
            }
        }
    }

    fn callable_kind(
        &self,
        declaration: usize,
    ) -> Result<Option<TsCallableKind>, TreeSitterModelError> {
        let node = self.parsed.node(declaration)?;
        if self.is_python()? {
            return self.python_callable_kind(declaration);
        }
        if self.is_ecma()? {
            return self.ecma_callable_kind(declaration);
        }
        if self.is_go()? {
            return Ok(match node.kind.as_str() {
                "function_declaration" => Some(TsCallableKind::Function),
                "method_declaration" => Some(TsCallableKind::MethodGo),
                _ => None,
            });
        }
        if !is_callable_node(node) {
            return Ok(None);
        }
        let container = if let Some(parent) = node.parent {
            let list = self.parsed.node(parent)?;
            if list.kind == "declaration_list" {
                list.parent
            } else {
                None
            }
        } else {
            None
        };
        let container_kind = match container {
            Some(c) => Some(self.parsed.node(c)?.kind.as_str()),
            None => None,
        };
        Ok(match (node.kind.as_str(), container_kind) {
            ("function_item", Some("impl_item")) => Some(TsCallableKind::ImplMethod),
            ("function_item", Some("trait_item")) => Some(TsCallableKind::TraitMethod),
            ("function_item", _) => Some(TsCallableKind::Function),
            ("function_signature_item", Some("trait_item")) => Some(TsCallableKind::TraitSignature),
            ("function_signature_item", Some("foreign_mod_item")) => {
                Some(TsCallableKind::ForeignFunction)
            }
            _ => None,
        })
    }
    fn callable(
        &mut self,
        target: Target<'doc>,
        declaration: usize,
    ) -> Result<TsCallable<'doc, 'o, 'v>, TreeSitterModelError> {
        let Some(kind) = self.callable_kind(declaration)? else {
            return Ok(TsCallable::NotApplicable);
        };
        let details = if let Some(details) = self.functions.get(&declaration) {
            Rc::clone(details)
        } else {
            let details = Rc::new(self.function_details(declaration, kind)?);
            self.functions.insert(declaration, Rc::clone(&details));
            details
        };
        for (issue_index, issue) in details.issues.iter().enumerate() {
            self.parsed.original.check()?;
            self.issue(target, TsIssue::Callable { issue_index }, issue.original);
        }
        Ok(TsCallable::Function(details))
    }
    fn function_details(
        &self,
        declaration: usize,
        kind: TsCallableKind,
    ) -> Result<TsFunctionDetails<'doc, 'o, 'v>, TreeSitterModelError> {
        self.parsed.original.check()?;
        if self.is_python()? {
            return self.python_function_details(declaration, kind);
        }
        if self.is_ecma()? {
            return self.ecma_function_details(declaration, kind);
        }
        if self.is_go()? {
            return self.go_function_details(declaration, kind);
        }
        let header = self.parsed.signatures.get(&declaration).ok_or(
            TreeSitterModelError::NativeContract("native callable lacks signature boundaries"),
        )?;
        let body = self.parsed.field(declaration, "body")?;
        let mut details = TsFunctionDetails {
            declaration_node_index: declaration,
            kind,
            body_node_index: body,
            defaultness_node_index: None,
            signature: TsSignature {
                prepared: header.prepared,
                original: &header.original,
                raw: exact_range(&header.original),
                syntax: None,
                go: None,
                ecma: None,
                python: None,
            },
            visibility: TsVisibility {
                original: None,
                raw: None,
                syntax: Some(syn::Visibility::Inherited),
            },
            attachments: Vec::new(),
            complexity: TsComplexity::Disabled,
            export: TsExport::NotConfigured,
            issues: Vec::new(),
        };
        if recovered(header.recovery) {
            details.issues.push(TsCallableIssue {
                node_index: declaration,
                original: Some(&header.original),
                kind: TsCallableIssueKind::Recovery { role: "signature" },
            });
        }
        if details.signature.raw.is_none() {
            details.issues.push(TsCallableIssue {
                node_index: declaration,
                original: Some(&header.original),
                kind: TsCallableIssueKind::NonExact { role: "signature" },
            });
        }
        let mut visibility = None;
        for &index in &self.parsed.children[declaration] {
            self.parsed.original.check()?;
            if self.parsed.node(index)?.kind == "visibility_modifier" {
                visibility = Some(index);
                break;
            }
        }
        if let Some(index) = visibility {
            let origin = self.parsed.origin(index)?;
            details.visibility.original = Some(&origin.original);
            details.visibility.raw = exact_range(&origin.original);
            details.visibility.syntax = None;
            if recovered(origin.recovery) {
                details.issues.push(TsCallableIssue {
                    node_index: index,
                    original: Some(&origin.original),
                    kind: TsCallableIssueKind::Recovery { role: "visibility" },
                });
            }
            if let Some(raw) = details.visibility.raw {
                self.parsed.original.check()?;
                let parsed = syn::parse_str::<syn::Visibility>(raw.text());
                self.parsed.original.check()?;
                match parsed {
                    Ok(v) => details.visibility.syntax = Some(v),
                    Err(e) => details.issues.push(TsCallableIssue {
                        node_index: index,
                        original: Some(&origin.original),
                        kind: TsCallableIssueKind::Visibility(e),
                    }),
                }
            } else {
                details.issues.push(TsCallableIssue {
                    node_index: index,
                    original: Some(&origin.original),
                    kind: TsCallableIssueKind::NonExact { role: "visibility" },
                });
            }
        }
        for &child in &self.parsed.children[declaration] {
            self.parsed.original.check()?;
            if self.parsed.node(child)?.kind == "function_modifiers" {
                for &modifier in &self.parsed.children[child] {
                    self.parsed.original.check()?;
                    if self.parsed.node(modifier)?.kind == "default" {
                        details.defaultness_node_index = Some(modifier);
                    }
                }
            }
        }
        if kind == TsCallableKind::ImplMethod && details.defaultness_node_index.is_some() {
            let origin = self.parsed.origin(declaration)?;
            if let OriginalProjection::Exact(raw) = &origin.original {
                self.parsed.original.check()?;
                let decoded = syn::parse_str::<syn::ImplItemFn>(raw.text());
                self.parsed.original.check()?;
                match decoded {
                    Ok(item) => {
                        if item.defaultness.is_none() {
                            details.issues.push(TsCallableIssue {
                                node_index: declaration,
                                original: Some(&origin.original),
                                kind: TsCallableIssueKind::DefaultnessMismatch,
                            });
                        }
                        details.signature.syntax = Some(Box::new(item.sig));
                        details.visibility.syntax = Some(item.vis);
                    }
                    Err(error) => details.issues.push(TsCallableIssue {
                        node_index: declaration,
                        original: Some(&origin.original),
                        kind: TsCallableIssueKind::Signature(error),
                    }),
                }
            } else {
                details.issues.push(TsCallableIssue {
                    node_index: declaration,
                    original: Some(&origin.original),
                    kind: TsCallableIssueKind::NonExact {
                        role: "default function",
                    },
                });
            }
        } else if !recovered(header.recovery)
            && let Some(raw) = details.signature.raw
        {
            self.parsed.original.check()?;
            let parsed = syn::parse_str::<syn::Signature>(raw.text());
            self.parsed.original.check()?;
            match parsed {
                Ok(sig) => details.signature.syntax = Some(Box::new(sig)),
                Err(e) => details.issues.push(TsCallableIssue {
                    node_index: declaration,
                    original: Some(&header.original),
                    kind: TsCallableIssueKind::Signature(e),
                }),
            }
        }
        let attachments = self.attachment_nodes(declaration)?;
        for &(index, placement) in &attachments {
            details
                .attachments
                .push(self.attachment(index, placement, &mut details.issues)?);
        }
        for (position, &(index, placement)) in attachments.iter().enumerate() {
            self.parsed.original.check()?;
            let next = if placement == TsAttachmentPlacement::Outer {
                attachments
                    .get(position + 1)
                    .filter(|(_, p)| *p == TsAttachmentPlacement::Outer)
                    .map_or(declaration, |(i, _)| *i)
            } else {
                continue;
            };
            if !self.original_gap(index, next)? {
                details.attachments[position].content = TsAttachmentContent::Unavailable;
                details.issues.push(TsCallableIssue {
                    node_index: index,
                    original: Some(&self.parsed.origin(index)?.original),
                    kind: TsCallableIssueKind::InvalidAttachmentOrigin,
                });
            }
        }
        if let Some(body) = body {
            let mut previous = None;
            for &index in &self.parsed.children[body] {
                self.parsed.original.check()?;
                if self.parsed.node(index)?.kind == "{" {
                    previous = Some(index);
                    break;
                }
            }
            for (position, &(index, placement)) in attachments.iter().enumerate() {
                self.parsed.original.check()?;
                if placement != TsAttachmentPlacement::Inner {
                    continue;
                }
                if let Some(previous) = previous
                    && !self.original_gap(previous, index)?
                {
                    details.attachments[position].content = TsAttachmentContent::Unavailable;
                    details.issues.push(TsCallableIssue {
                        node_index: index,
                        original: Some(&self.parsed.origin(index)?.original),
                        kind: TsCallableIssueKind::InvalidAttachmentOrigin,
                    });
                }
                previous = Some(index);
            }
        }
        details.complexity = self.complexity(declaration, body, &mut details.issues)?;
        let profile = self.parsed.context.observations()?.profile;
        let exports = profile.exports().value();
        if *exports.strategy().value() != ExportStrategy::None {
            let keyword = exports
                .config()
                .value()
                .iter()
                .find(|(k, _)| k.value() == "keyword")
                .map(|(_, v)| v.value().as_str());
            if *exports.strategy().value() == ExportStrategy::Modifier
                && keyword == Some("pub")
                && exports.config().value().len() == 1
                && exports.config_list().value().is_empty()
                && details.visibility.syntax.is_some()
            {
                let mut exported = false;
                if let Some(index) = visibility {
                    for &child in &self.parsed.children[index] {
                        self.parsed.original.check()?;
                        exported |= self.parsed.node(child)?.kind == "pub";
                    }
                }
                details.export = TsExport::Modifier(exported);
            } else {
                details.export = TsExport::Unavailable;
                details.issues.push(TsCallableIssue {
                    node_index: declaration,
                    original: None,
                    kind: TsCallableIssueKind::UnsupportedExportPolicy,
                });
            }
        }
        self.parsed.original.check()?;
        Ok(details)
    }
    fn configured_comment(&self, node: usize) -> Result<bool, TreeSitterModelError> {
        let kind = &self.parsed.node(node)?.kind;
        Ok(self
            .parsed
            .context
            .observations()?
            .profile
            .comment_types()
            .value()
            .iter()
            .any(|c| c.value() == kind))
    }
    fn attachment_candidate(&self, index: usize) -> Result<bool, TreeSitterModelError> {
        Ok(matches!(
            self.parsed.node(index)?.kind.as_str(),
            "attribute_item" | "inner_attribute_item"
        ) || self.configured_comment(index)?)
    }
    fn original_gap(&self, first: usize, second: usize) -> Result<bool, TreeSitterModelError> {
        self.parsed.original.check()?;
        match (
            &self.parsed.origin(first)?.original,
            &self.parsed.origin(second)?.original,
        ) {
            (OriginalProjection::Exact(a), OriginalProjection::Exact(b))
                if a.end() <= b.start() =>
            {
                Ok(self
                    .parsed
                    .source
                    .range(a.end(), b.start())?
                    .text()
                    .chars()
                    .all(char::is_whitespace))
            }
            _ => Ok(false),
        }
    }
    fn whitespace_gap(&self, first: usize, second: usize) -> Result<bool, TreeSitterModelError> {
        self.parsed.original.check()?;
        let a = self.parsed.node(first)?;
        let b = self.parsed.node(second)?;
        let gap = self
            .parsed
            .prepared_text
            .get(a.range.end..b.range.start)
            .ok_or(TreeSitterModelError::NativeContract(
                "native sibling gap is inconsistent",
            ))?;
        Ok(gap.chars().all(char::is_whitespace))
    }
    fn attachment_nodes(
        &self,
        declaration: usize,
    ) -> Result<Vec<(usize, TsAttachmentPlacement)>, TreeSitterModelError> {
        let mut outer = Vec::new();
        if let Some(parent) = self.parsed.node(declaration)?.parent {
            let siblings = &self.parsed.children[parent];
            let at = siblings.iter().position(|&i| i == declaration).ok_or(
                TreeSitterModelError::NativeContract("declaration absent from native parent"),
            )?;
            let mut next = declaration;
            for &index in siblings[..at].iter().rev() {
                self.parsed.original.check()?;
                if !self.attachment_candidate(index)?
                    || self.parsed.node(index)?.kind == "inner_attribute_item"
                    || self.parsed.field(index, "inner")?.is_some()
                    || !self.whitespace_gap(index, next)?
                {
                    break;
                }
                outer.push((index, TsAttachmentPlacement::Outer));
                next = index;
            }
            outer.reverse();
        }
        if let Some(body) = self.parsed.field(declaration, "body")? {
            let mut previous = None;
            for &index in &self.parsed.children[body] {
                self.parsed.original.check()?;
                let node = self.parsed.node(index)?;
                if node.kind == "{" {
                    previous = Some(index);
                    continue;
                }
                if !self.attachment_candidate(index)?
                    || node.kind == "attribute_item"
                    || self.parsed.field(index, "outer")?.is_some()
                {
                    break;
                }
                if let Some(previous) = previous
                    && !self.whitespace_gap(previous, index)?
                {
                    break;
                }
                outer.push((index, TsAttachmentPlacement::Inner));
                previous = Some(index);
            }
        }
        Ok(outer)
    }
    fn attachment(
        &self,
        index: usize,
        placement: TsAttachmentPlacement,
        issues: &mut Vec<TsCallableIssue<'doc, 'o>>,
    ) -> Result<TsAttachment<'doc, 'o>, TreeSitterModelError> {
        let origin = self.parsed.origin(index)?;
        let mut attachment = TsAttachment {
            node_index: index,
            original: &origin.original,
            raw: exact_range(&origin.original),
            placement,
            content: TsAttachmentContent::Unavailable,
        };
        if recovered(origin.recovery) {
            issues.push(TsCallableIssue {
                node_index: index,
                original: Some(&origin.original),
                kind: TsCallableIssueKind::Recovery { role: "attachment" },
            });
            return Ok(attachment);
        }
        let Some(raw) = attachment.raw else {
            issues.push(TsCallableIssue {
                node_index: index,
                original: Some(&origin.original),
                kind: TsCallableIssueKind::NonExact { role: "attachment" },
            });
            return Ok(attachment);
        };
        let node = self.parsed.node(index)?;
        let attribute = matches!(
            node.kind.as_str(),
            "attribute_item" | "inner_attribute_item"
        );
        let doc = self.parsed.field(index, "doc")?.is_some();
        if !attribute && !doc {
            attachment.content = TsAttachmentContent::Comment;
            return Ok(attachment);
        }
        self.parsed.original.check()?;
        let parsed = if placement == TsAttachmentPlacement::Outer {
            syn::Attribute::parse_outer.parse_str(raw.text())
        } else {
            syn::Attribute::parse_inner.parse_str(raw.text())
        };
        self.parsed.original.check()?;
        match parsed {
            Ok(mut attributes) if attributes.len() == 1 => {
                let attr = attributes.remove(0);
                if attr.path().is_ident("doc") {
                    if let syn::Meta::NameValue(value) = &attr.meta
                        && let syn::Expr::Lit(value) = &value.value
                        && let syn::Lit::Str(value) = &value.lit
                    {
                        attachment.content = TsAttachmentContent::Documentation(value.value());
                    } else {
                        attachment.content = TsAttachmentContent::Attribute(Box::new(attr));
                        issues.push(TsCallableIssue {
                            node_index: index,
                            original: Some(&origin.original),
                            kind: TsCallableIssueKind::UnsupportedDocumentationExpression,
                        });
                    }
                } else {
                    attachment.content = TsAttachmentContent::Attribute(Box::new(attr));
                }
            }
            Ok(_) => issues.push(TsCallableIssue {
                node_index: index,
                original: Some(&origin.original),
                kind: TsCallableIssueKind::InvalidAttachmentOrigin,
            }),
            Err(e) => issues.push(TsCallableIssue {
                node_index: index,
                original: Some(&origin.original),
                kind: TsCallableIssueKind::Attachment(e),
            }),
        }
        Ok(attachment)
    }
    fn complexity(
        &self,
        declaration: usize,
        body: Option<usize>,
        issues: &mut Vec<TsCallableIssue<'doc, 'o>>,
    ) -> Result<TsComplexity, TreeSitterModelError> {
        let profile = self.parsed.context.observations()?.profile;
        let complexity = profile.complexity().value();
        if complexity.head_calls().value().is_some() {
            issues.push(TsCallableIssue {
                node_index: declaration,
                original: None,
                kind: TsCallableIssueKind::UnsupportedComplexityPolicy,
            });
            return Ok(TsComplexity::Unavailable);
        }
        if complexity.node_types().value().is_empty() && complexity.operators().value().is_empty() {
            return Ok(TsComplexity::Disabled);
        }
        let Some(body) = body else {
            return Ok(TsComplexity::NoBody);
        };
        let origin = self.parsed.origin(body)?;
        if recovered(origin.recovery) {
            issues.push(TsCallableIssue {
                node_index: body,
                original: Some(&origin.original),
                kind: TsCallableIssueKind::Recovery {
                    role: "metric body",
                },
            });
            return Ok(TsComplexity::Unavailable);
        }
        let mut decisions = Vec::new();
        let mut stack = vec![body];
        while let Some(index) = stack.pop() {
            self.parsed.original.check()?;
            let node = self.parsed.node(index)?;
            if index != body
                && (is_callable_node(node)
                    || (self.is_go()? && node.kind == "func_literal")
                    || profile
                        .declaration_types()
                        .value()
                        .iter()
                        .chain(profile.anon_func_types().value().iter())
                        .any(|d| d.value() == &node.kind))
            {
                continue;
            }
            let matched = if node.named {
                complexity
                    .node_types()
                    .value()
                    .iter()
                    .any(|n| n.value() == &node.kind)
            } else {
                self.parsed.children[index].is_empty()
                    && complexity
                        .operators()
                        .value()
                        .iter()
                        .any(|n| n.value() == &node.kind)
            };
            if matched {
                decisions.push(index);
            }
            stack.extend(self.parsed.children[index].iter().rev());
        }
        let value = decisions
            .len()
            .checked_add(1)
            .ok_or(TreeSitterModelError::NativeContract("complexity overflow"))?;
        Ok(TsComplexity::Score {
            value,
            body_node_index: body,
            decision_node_indices: decisions,
        })
    }
    fn relation_container(&self, name: usize) -> Result<Option<usize>, TreeSitterModelError> {
        if self.is_python()? {
            return self.python_relation_container(name);
        }
        if self.is_ecma()? {
            return self.ecma_relation_container(name);
        }
        let mut cursor = Some(name);
        let mut attribute = None;
        while let Some(index) = cursor {
            self.parsed.original.check()?;
            let node = self.parsed.node(index)?;
            if node.kind == "attribute_item" {
                attribute = Some(index);
                break;
            }
            if (self.is_go()? && node.kind == "func_literal")
                || (self
                    .parsed
                    .context
                    .observations()?
                    .profile
                    .language()
                    .value()
                    == "rust"
                    && node.kind == "closure_expression")
            {
                return Ok(Some(index));
            }
            if is_callable_node(node) {
                return Ok(Some(index));
            }
            cursor = node.parent;
        }
        if let Some(attribute) = attribute
            && let Some(parent) = self.parsed.node(attribute)?.parent
        {
            let siblings = &self.parsed.children[parent];
            let at = siblings.iter().position(|&i| i == attribute).ok_or(
                TreeSitterModelError::NativeContract("native attribute lacks parent"),
            )?;
            let mut previous = attribute;
            for &index in &siblings[at + 1..] {
                self.parsed.original.check()?;
                if !self.whitespace_gap(previous, index)? {
                    break;
                }
                if is_callable_node(self.parsed.node(index)?) {
                    return Ok(Some(index));
                }
                if !self.attachment_candidate(index)?
                    || self.parsed.field(index, "inner")?.is_some()
                    || self.parsed.node(index)?.kind == "inner_attribute_item"
                {
                    break;
                }
                previous = index;
            }
        }
        Ok(None)
    }
    fn relations(
        &self,
    ) -> Result<Vec<TsRelationObservation<'doc, 'profile, 'o, 'v>>, TreeSitterModelError> {
        let observations = self.parsed.context.observations()?;
        let mut relations = Vec::new();
        for (query_index, group) in observations.groups.iter().enumerate() {
            self.parsed.original.check()?;
            if *group.query.value().r#type().value() != QueryKind::Relation
                || !group.query.value().context_capture().value().is_empty()
            {
                continue;
            }
            let relation_type = group.query.value().relation_type().value();
            let target_rule = observations
                .profile
                .target_rules()
                .value()
                .iter()
                .find(|(k, _)| k.value() == relation_type)
                .map(|(_, v)| v);
            for (match_index, matched) in group.matches.iter().enumerate() {
                self.parsed.original.check()?;
                let mut sources = Vec::new();
                if let CaptureBinding::Present(names) = &matched.bindings.name {
                    for &capture_index in names {
                        self.parsed.original.check()?;
                        let capture = matched.captures.get(capture_index).ok_or(
                            TreeSitterModelError::NativeContract(
                                "relation references foreign capture",
                            ),
                        )?;
                        let node_index = self.parsed.captured_node(capture)?;
                        let state = if let Some(container) = self.relation_container(node_index)? {
                            let mut indices = Vec::new();
                            for (entity_index, entity) in self.entities.iter().enumerate() {
                                self.parsed.original.check()?;
                                if entity.contributions.iter().any(|c| {
                                    c.derived_value.is_none()
                                        && c.declaration_node_index == container
                                }) {
                                    indices.push(entity_index);
                                }
                            }
                            if indices.is_empty() {
                                TsRelationSourceState::UnavailableContainer {
                                    container_node_index: container,
                                }
                            } else {
                                TsRelationSourceState::Bound {
                                    container_node_index: container,
                                    entity_indices: indices,
                                }
                            }
                        } else {
                            TsRelationSourceState::MissingContainer
                        };
                        sources.push(TsRelationSource {
                            capture_index,
                            node_index,
                            state,
                        });
                    }
                }
                relations.push(TsRelationObservation {
                    query_index,
                    match_index,
                    query: group.query,
                    matched,
                    target_rule,
                    sources,
                });
            }
        }
        self.parsed.original.check()?;
        Ok(relations)
    }

    fn insert_entity(
        &mut self,
        target: Target<'doc>,
        seed: EntitySeed<'doc, 'o, 'request>,
        contribution: TsContribution<'doc, 'o, 'v>,
    ) -> Result<usize, TreeSitterModelError> {
        let capture = contribution.name_capture;
        let key = (seed.identity.uid().as_str().to_owned(), target.data_key);
        let entity_index = if let Some(&index) = self.entity_indices.get(&key) {
            let entity = &self.entities[index];
            let next = entity.contributions.len();
            let mut extent_conflicts = Vec::new();
            let mut type_conflicts = Vec::new();
            let mut binding_conflict =
                entity.name != seed.name || !same_range(entity.name_raw, seed.raw);
            for (previous, c) in entity.contributions.iter().enumerate() {
                self.parsed.original.check()?;
                if !extent_agrees(&c.extent, &contribution.extent, self.parsed.original)? {
                    extent_conflicts.push(ContributionConflict {
                        field: ContributionConflictField::Extent,
                        first_contribution: previous,
                        second_contribution: next,
                    });
                }
                if declared_types_disagree(&c.declared_type, &contribution.declared_type) {
                    type_conflicts.push(TsModelConflict {
                        field: TsModelConflictField::DeclaredType,
                        first_contribution: previous,
                        second_contribution: next,
                    });
                }
                if !callable_agrees(&c.callable, &contribution.callable, self.parsed.original)? {
                    type_conflicts.push(TsModelConflict {
                        field: TsModelConflictField::Callable,
                        first_contribution: previous,
                        second_contribution: next,
                    });
                }
                if go_values_disagree(&c.declared_value, &contribution.declared_value) {
                    type_conflicts.push(TsModelConflict {
                        field: TsModelConflictField::Value,
                        first_contribution: previous,
                        second_contribution: next,
                    });
                }
                if !go_details_agree(
                    &c.go_declaration,
                    &contribution.go_declaration,
                    self.parsed.original,
                )? {
                    type_conflicts.push(TsModelConflict {
                        field: TsModelConflictField::GoDeclaration,
                        first_contribution: previous,
                        second_contribution: next,
                    });
                }
                if !ecma::details_agree(
                    &c.ecma_declaration,
                    &contribution.ecma_declaration,
                    self.parsed.original,
                )? {
                    type_conflicts.push(TsModelConflict {
                        field: TsModelConflictField::EcmaDeclaration,
                        first_contribution: previous,
                        second_contribution: next,
                    });
                }
                if !python::details_agree(
                    &c.python_declaration,
                    &contribution.python_declaration,
                    self.parsed.original,
                )? {
                    type_conflicts.push(TsModelConflict {
                        field: TsModelConflictField::PythonDeclaration,
                        first_contribution: previous,
                        second_contribution: next,
                    });
                }
                if c.namespaces.len() != contribution.namespaces.len() {
                    binding_conflict = true;
                }
                for (a, b) in c.namespaces.iter().zip(&contribution.namespaces) {
                    self.parsed.original.check()?;
                    if a.label != b.label
                        || a.name != b.name
                        || !same_range(a.anchor, b.anchor)
                        || !extent_agrees(&a.extent, &b.extent, self.parsed.original)?
                    {
                        binding_conflict = true;
                    }
                }
            }
            if binding_conflict {
                self.issue(
                    target,
                    TsIssue::EntityBindingConflict,
                    Some(&capture.original),
                );
            }
            let entity = &mut self.entities[index];
            entity.conflicts.extend(extent_conflicts);
            entity.model_conflicts.extend(type_conflicts);
            entity.contributions.push(contribution);
            index
        } else {
            let index = self.entities.len();
            self.entity_indices.insert(key, index);
            self.entities.push(TsEntity {
                identity: seed.identity,
                data_key: target.data_key,
                name: seed.name,
                name_raw: seed.raw,
                name_representation: seed.name_representation,
                contributions: vec![contribution],
                conflicts: Vec::new(),
                model_conflicts: Vec::new(),
                diagnostic_indices: Vec::new(),
            });
            index
        };

        Ok(entity_index)
    }

    fn observe(
        &mut self,
        query_index: usize,
        match_index: usize,
        capability: &TsCapability,
    ) -> Result<(), TreeSitterModelError> {
        self.parsed.original.check()?;
        if matches!(
            capability,
            TsCapability::ObservedOnlyFamily | TsCapability::ObservedOnlyRelation
        ) {
            return Ok(());
        }
        let group = &self.parsed.context.observations()?.groups[query_index];
        let matched = &group.matches[match_index];
        let query = group.query.value();
        let base = Target {
            occurrence: TsOccurrence {
                query_index,
                match_index,
                name_capture_index: None,
            },
            declaration: None,
            subject: None,
            label: query.graph_label().value(),
            data_key: query.data_key().value(),
        };
        let CaptureBinding::Present(names) = &matched.bindings.name else {
            match capability {
                TsCapability::UnsupportedProfileNormalization => {
                    self.issue(base, TsIssue::UnsupportedProfileNormalization, None)
                }
                TsCapability::UnsupportedConfiguration(f) => {
                    self.issue(base, TsIssue::UnsupportedConfiguration(f.clone()), None)
                }
                _ => self.issue(
                    base,
                    TsIssue::MissingCapture {
                        role: "name",
                        declared: matches!(
                            matched.bindings.name,
                            CaptureBinding::Missing { declared: true }
                        ),
                    },
                    None,
                ),
            }
            return Ok(());
        };
        let mut declarations = HashSet::new();
        for &slot in names {
            self.parsed.original.check()?;
            let capture =
                matched
                    .captures
                    .get(slot)
                    .ok_or(TreeSitterModelError::NativeContract(
                        "binding references foreign capture",
                    ))?;
            if let Some((declaration, _)) =
                self.declaration(self.parsed.captured_node(capture)?, base.label)?
            {
                declarations.insert(declaration);
            }
        }
        for &slot in names {
            self.parsed.original.check()?;
            let capture = &matched.captures[slot];
            let name_node = self.parsed.captured_node(capture)?;
            let mut target = base;
            target.occurrence.name_capture_index = Some(slot);
            target.subject = Some(name_node);
            let declaration = self.declaration(name_node, target.label)?;
            target.declaration = declaration.map(|(d, _)| d);
            match capability {
                TsCapability::UnsupportedProfileNormalization => {
                    self.issue(
                        target,
                        TsIssue::UnsupportedProfileNormalization,
                        Some(&capture.original),
                    );
                    continue;
                }
                TsCapability::UnsupportedConfiguration(fields) => {
                    self.issue(
                        target,
                        TsIssue::UnsupportedConfiguration(fields.clone()),
                        Some(&capture.original),
                    );
                    continue;
                }
                _ => {}
            }
            let Some((declaration, container)) = declaration else {
                self.issue(
                    target,
                    TsIssue::UnsupportedDeclarationFamily {
                        node_index: name_node,
                    },
                    Some(&capture.original),
                );
                continue;
            };
            let origin = self.parsed.origin(name_node)?;
            if recovered(origin.recovery) {
                self.issue(
                    target,
                    TsIssue::Recovery {
                        role: "name",
                        node_index: name_node,
                    },
                    Some(&origin.original),
                );
                continue;
            }
            let OriginalProjection::Exact(raw) = &origin.original else {
                self.issue(
                    target,
                    TsIssue::NonExact {
                        role: "name",
                        node_index: name_node,
                    },
                    Some(&origin.original),
                );
                continue;
            };
            if self
                .name_reject
                .get(&query_index)
                .is_some_and(|r| r.is_match(raw.text()))
            {
                self.issue(
                    target,
                    TsIssue::ExcludedName {
                        node_index: name_node,
                    },
                    Some(&origin.original),
                );
                continue;
            }
            let Some(mut seed) = self.seed(target, name_node, *raw)? else {
                continue;
            };
            if self.is_python()? {
                self.python_explicit_parent(target, declaration, matched, &mut seed)?;
            }
            if self.is_ecma()? {
                self.ecma_explicit_parent(target, declaration, matched, &mut seed)?;
            }
            if self.is_go()? {
                self.go_explicit_parent(target, declaration, matched, &mut seed)?;
            }
            let mut span_slots = Vec::new();
            let mut valid_span = true;
            match &matched.bindings.span {
                CaptureBinding::NotConfigured => {}
                CaptureBinding::Missing { declared } => {
                    self.issue(
                        target,
                        TsIssue::MissingCapture {
                            role: "span",
                            declared: *declared,
                        },
                        None,
                    );
                    valid_span = false;
                }
                CaptureBinding::Present(slots) => {
                    for &span_slot in slots {
                        self.parsed.original.check()?;
                        let span = matched.captures.get(span_slot).ok_or(
                            TreeSitterModelError::NativeContract(
                                "span binding references foreign capture",
                            ),
                        )?;
                        let span_node = self.parsed.captured_node(span)?;
                        let owner = self.span_owner(span_node, &declarations)?;
                        if owner == Some(declaration) && span_node == declaration {
                            span_slots.push(span_slot);
                            continue;
                        }
                        if owner.is_none() || owner == Some(declaration) {
                            self.issue(
                                target,
                                TsIssue::InvalidSpan {
                                    capture_index: span_slot,
                                },
                                Some(&span.original),
                            );
                            valid_span = false;
                        }
                    }
                    if span_slots.is_empty() {
                        self.issue(
                            target,
                            TsIssue::MissingCapture {
                                role: "span",
                                declared: true,
                            },
                            None,
                        );
                        valid_span = false;
                    }
                }
            }
            let extent = if valid_span {
                self.extent(target, declaration, "declaration")?
            } else {
                ExtentObservation::Missing
            };
            let declared_type = self.declared_type(target, declaration)?;
            let callable = self.callable(target, declaration)?;
            let declared_value = if self.is_python()? {
                self.python_value(target, declaration, name_node, matched)?
            } else if self.is_ecma()? {
                self.ecma_value(target, declaration, name_node, matched)?
            } else if self.is_go()? {
                self.go_value(target, declaration, name_node, matched)?
            } else {
                TsDeclaredValue::NotConfigured
            };
            let go_declaration =
                if self.is_go()? && !is_callable_node(self.parsed.node(declaration)?) {
                    let details = self.go_declaration_details(declaration, name_node)?;
                    for (issue_index, issue) in details.issues.iter().enumerate() {
                        self.issue(
                            target,
                            TsIssue::GoDeclaration { issue_index },
                            issue.original,
                        );
                    }
                    Some(details)
                } else {
                    None
                };
            let ecma_declaration = if self.is_ecma()? {
                let details = self.ecma_declaration_details(declaration)?;
                for (issue_index, issue) in details.issues.iter().enumerate() {
                    self.issue(
                        target,
                        TsIssue::EcmaDeclaration { issue_index },
                        issue.original,
                    );
                }
                Some(details)
            } else {
                None
            };
            let python_declaration = if self.is_python()? {
                let details = self.python_declaration_details(declaration, name_node)?;
                for (issue_index, issue) in details.issues.iter().enumerate() {
                    self.issue(
                        target,
                        TsIssue::PythonDeclaration { issue_index },
                        issue.original,
                    );
                }
                Some(details)
            } else {
                None
            };
            let contribution = TsContribution {
                occurrence: target.occurrence,
                declaration_node_index: declaration,
                container_node_index: container,
                name_capture: capture,
                span_capture_indices: span_slots,
                extent,
                declared_type,
                callable,
                declared_value,
                derived_value: None,
                go_declaration,
                ecma_declaration,
                python_declaration,
                excluded_context_indices: std::mem::take(&mut seed.excluded_context_indices),
                namespaces: std::mem::take(&mut seed.namespaces),
            };
            let value_rows = self.value_rows(target, &seed, &contribution, matched)?;
            let entity_index = self.insert_entity(target, seed, contribution)?;
            let subjects = self
                .native_entities
                .entry((declaration, target.label, target.data_key))
                .or_default();
            if !subjects.contains(&(name_node, entity_index)) {
                subjects.push((name_node, entity_index));
            }
            for row in value_rows {
                let value_node = row.contribution.declaration_node_index;
                let index = self.insert_entity(row.target, row.seed, row.contribution)?;
                let subjects = self
                    .native_entities
                    .entry((value_node, row.target.label, row.target.data_key))
                    .or_default();
                if !subjects.contains(&(value_node, index)) {
                    subjects.push((value_node, index));
                }
            }
        }
        self.parsed.original.check()?;
        Ok(())
    }
}
fn declared_types_disagree(a: &TsDeclaredType<'_, '_, '_>, b: &TsDeclaredType<'_, '_, '_>) -> bool {
    match (a, b) {
        (TsDeclaredType::Present(a), TsDeclaredType::Present(b)) => a.raw.text() != b.raw.text(),
        (TsDeclaredType::Native(a), TsDeclaredType::Native(b)) => {
            a.raw.map(|r| r.text()) != b.raw.map(|r| r.text()) || a.kind != b.kind
        }
        _ => false,
    }
}
fn same_range(a: SourceRange<'_>, b: SourceRange<'_>) -> bool {
    a.start() == b.start()
        && a.end() == b.end()
        && a.text().as_ptr() == b.text().as_ptr()
        && a.text().len() == b.text().len()
        && a.start_point() == b.start_point()
        && a.end_point() == b.end_point()
}
fn extent_agrees(
    a: &ExtentObservation<'_, '_>,
    b: &ExtentObservation<'_, '_>,
    original: &SnapshotContext,
) -> Result<bool, TreeSitterModelError> {
    original.check()?;
    match (a, b) {
        (ExtentObservation::Exact(a), ExtentObservation::Exact(b)) => {
            Ok(same_range(a.range(), b.range()))
        }
        (ExtentObservation::NonExact(a), ExtentObservation::NonExact(b)) => {
            origin_agrees(a, b, original)
        }
        (ExtentObservation::Missing, ExtentObservation::Missing) => Ok(true),
        _ => Ok(false),
    }
}
fn origin_agrees(
    a: &OriginalProjection<'_>,
    b: &OriginalProjection<'_>,
    original: &SnapshotContext,
) -> Result<bool, TreeSitterModelError> {
    original.check()?;
    match (a, b) {
        (OriginalProjection::Exact(a), OriginalProjection::Exact(b)) => Ok(same_range(*a, *b)),
        (OriginalProjection::Synthetic(a), OriginalProjection::Synthetic(b)) => Ok(a == b),
        (OriginalProjection::Unmappable(a), OriginalProjection::Unmappable(b)) => Ok(a == b),
        (OriginalProjection::Ambiguous(a), OriginalProjection::Ambiguous(b)) => {
            if a.len() != b.len() {
                return Ok(false);
            }
            for (a, b) in a.iter().zip(b) {
                original.check()?;
                if !same_range(*a, *b) {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (OriginalProjection::Compound(a), OriginalProjection::Compound(b)) => {
            if a.len() != b.len() {
                return Ok(false);
            }
            for (a, b) in a.iter().zip(b) {
                original.check()?;
                let agrees = match (a, b) {
                    (OriginPiece::Original(a), OriginPiece::Original(b)) => same_range(*a, *b),
                    (OriginPiece::Synthetic(a), OriginPiece::Synthetic(b)) => a == b,
                    (OriginPiece::Unmappable(a), OriginPiece::Unmappable(b)) => a == b,
                    _ => false,
                };
                if !agrees {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn exact_range<'o>(origin: &OriginalProjection<'o>) -> Option<SourceRange<'o>> {
    match origin {
        OriginalProjection::Exact(raw) => Some(*raw),
        _ => None,
    }
}
fn callable_agrees(
    a: &TsCallable<'_, '_, '_>,
    b: &TsCallable<'_, '_, '_>,
    original: &SnapshotContext,
) -> Result<bool, TreeSitterModelError> {
    original.check()?;
    match (a, b) {
        (TsCallable::NotApplicable, TsCallable::NotApplicable) => Ok(true),
        (TsCallable::Function(a), TsCallable::Function(b)) => {
            if Rc::ptr_eq(a, b) {
                return Ok(true);
            }
            if a.kind != b.kind
                || !origin_agrees(a.signature.original, b.signature.original, original)?
                || a.export != b.export
                || a.attachments.len() != b.attachments.len()
            {
                return Ok(false);
            }
            match (a.visibility.original, b.visibility.original) {
                (Some(a), Some(b)) if !origin_agrees(a, b, original)? => return Ok(false),
                (None, None) | (Some(_), Some(_)) => {}
                _ => return Ok(false),
            }
            for (a, b) in a.attachments.iter().zip(&b.attachments) {
                original.check()?;
                if a.placement != b.placement || !origin_agrees(a.original, b.original, original)? {
                    return Ok(false);
                }
            }
            match (&a.complexity, &b.complexity) {
                (TsComplexity::Score { value: a, .. }, TsComplexity::Score { value: b, .. }) => {
                    Ok(a == b)
                }
                (TsComplexity::Disabled, TsComplexity::Disabled)
                | (TsComplexity::NoBody, TsComplexity::NoBody)
                | (TsComplexity::Unavailable, TsComplexity::Unavailable) => Ok(true),
                _ => Ok(false),
            }
        }
        _ => Ok(false),
    }
}

fn go_values_disagree(a: &TsDeclaredValue<'_, '_, '_>, b: &TsDeclaredValue<'_, '_, '_>) -> bool {
    let selected = |value: &TsDeclaredValue<'_, '_, '_>| match value {
        TsDeclaredValue::Native { captures, .. } => captures
            .iter()
            .filter(|c| matches!(c.binding, TsValueBinding::Subject { .. }))
            .map(|c| {
                (
                    c.syntax.node_index,
                    c.syntax.raw.map(|r| (r.start(), r.end())),
                )
            })
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    if matches!(a, TsDeclaredValue::NotConfigured) || matches!(b, TsDeclaredValue::NotConfigured) {
        return false;
    }
    let a_values = selected(a);
    let b_values = selected(b);
    match (a, b) {
        (TsDeclaredValue::Missing { .. }, TsDeclaredValue::Native { .. }) => !b_values.is_empty(),
        (TsDeclaredValue::Native { .. }, TsDeclaredValue::Missing { .. }) => !a_values.is_empty(),
        _ => {
            !a_values.is_empty()
                && !b_values.is_empty()
                && (a_values.iter().any(|v| !b_values.contains(v))
                    || b_values.iter().any(|v| !a_values.contains(v)))
        }
    }
}
fn go_details_agree(
    a: &Option<TsGoDeclarationDetails<'_, '_, '_>>,
    b: &Option<TsGoDeclarationDetails<'_, '_, '_>>,
    original: &SnapshotContext,
) -> Result<bool, TreeSitterModelError> {
    match (a, b) {
        (None, None) => Ok(true),
        (Some(a), Some(b)) => {
            if a.native_kind != b.native_kind
                || a.export != b.export
                || a.attachment_owner_node_index != b.attachment_owner_node_index
                || a.alias.as_ref().map(|n| n.node_index) != b.alias.as_ref().map(|n| n.node_index)
                || a.attachments.len() != b.attachments.len()
            {
                return Ok(false);
            }
            for (a, b) in a.attachments.iter().zip(&b.attachments) {
                if a.placement != b.placement || !origin_agrees(a.original, b.original, original)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}
