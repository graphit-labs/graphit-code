//! Oracle and DB2 declaration assembly over genuine raw ANTLR observations.
//! Other declarations and unresolved relations remain observable in the retained input.

pub(crate) mod db2;
mod oracle_applications;
mod oracle_bindings;
mod oracle_calls;
pub(crate) mod oracle_targets;
pub use oracle_applications::{
    AntlrNativeSyntax, AntlrOracleApplication, AntlrOracleApplicationKind, AntlrOracleArgument,
    AntlrOracleArgumentGroup, AntlrOracleAssignmentContext, AntlrOracleAssignmentSide,
    AntlrOracleCollectionStatement, AntlrOracleMemberApplication,
};
pub use oracle_bindings::{
    AntlrOracleApplicationBinding, AntlrOracleApplicationClass, AntlrOracleBindingIssue,
    AntlrOracleBindingState, AntlrOracleBoundDeclaration, AntlrOracleBoundKind,
    AntlrOracleCollectionMethodKind, AntlrOracleLookup, AntlrOracleMemberContinuity,
    AntlrOracleScopeContinuity,
};
pub use oracle_calls::{
    AntlrOracleCallGroupOutcome, AntlrOracleCallIssue, AntlrOracleCallOutcome,
    AntlrOracleCallReason, AntlrOracleCallSourceState, AntlrOracleCallTargetState,
};
mod oracle_declarations;
pub use oracle_declarations::{
    AntlrOracleDeclaration, AntlrOracleExpression, AntlrOracleKeyword, AntlrOracleTypeDeclaration,
    AntlrOracleTypeKind,
};

use std::{collections::HashMap, error::Error, fmt, io, sync::Arc};

use graphit_core::file_io::SnapshotContext;
use regex::Regex;
use sqlparser::{
    dialect::GenericDialect,
    keywords::Keyword,
    tokenizer::{Token, Tokenizer, TokenizerError},
};

use super::{
    NodeClass, SpanStatus, SyntaxNode,
    semantic::{
        CaptureBinding, SemanticDocument, SemanticError, SemanticOccurrence,
        parse_semantic_occurrences,
    },
};
use crate::{
    RangeError, SourceRange, SourceText,
    catalog::EffectiveProfile,
    entity_extent::{EntityExtent, EntityExtentError},
    prepared_input::{OriginError, OriginalProjection, PreparedInput},
    profiles::{ProfileParser, QueryKind},
    semantic_fields::{ContributionConflict, ContributionConflictField, ExtentObservation},
    symbols::{
        IdentityDocument, IdentityDocumentInput, IdentityError, IdentityLimits, SyntaxIdentity,
    },
};

#[derive(Debug)]
pub enum AntlrModelError {
    Operation(io::Error),
    Raw(SemanticError),
    Origin(OriginError),
    Identity(IdentityError),
    Extent(EntityExtentError),
    Range(RangeError),
    UnsupportedProfile,
    InvalidQueryPolicy {
        query_index: usize,
        cause: regex::Error,
    },
    NativeContract(&'static str),
}
impl fmt::Display for AntlrModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(e) => e.fmt(f),
            Self::Raw(e) => e.fmt(f),
            Self::Origin(e) => e.fmt(f),
            Self::Identity(e) => e.fmt(f),
            Self::Extent(e) => e.fmt(f),
            Self::Range(e) => e.fmt(f),
            Self::UnsupportedProfile => {
                f.write_str("ANTLR model supports Oracle PL/SQL and DB2 profiles")
            }
            Self::InvalidQueryPolicy { query_index, cause } => {
                write!(f, "query {query_index} rejection policy: {cause}")
            }
            Self::NativeContract(message) => f.write_str(message),
        }
    }
}
impl Error for AntlrModelError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(e) => Some(e),
            Self::Raw(e) => Some(e),
            Self::Origin(e) => Some(e),
            Self::Identity(e) => Some(e),
            Self::Extent(e) => Some(e),
            Self::Range(e) => Some(e),
            Self::InvalidQueryPolicy { cause, .. } => Some(cause),
            _ => None,
        }
    }
}
impl From<io::Error> for AntlrModelError {
    fn from(e: io::Error) -> Self {
        Self::Operation(e)
    }
}
impl From<SemanticError> for AntlrModelError {
    fn from(e: SemanticError) -> Self {
        Self::Raw(e)
    }
}
impl From<OriginError> for AntlrModelError {
    fn from(e: OriginError) -> Self {
        Self::Origin(e)
    }
}
impl From<IdentityError> for AntlrModelError {
    fn from(e: IdentityError) -> Self {
        Self::Identity(e)
    }
}
impl From<EntityExtentError> for AntlrModelError {
    fn from(e: EntityExtentError) -> Self {
        Self::Extent(e)
    }
}
impl From<RangeError> for AntlrModelError {
    fn from(e: RangeError) -> Self {
        Self::Range(e)
    }
}

struct Declaration {
    name: Option<usize>,
    r#type: Option<usize>,
    recovered: bool,
    name_components: Vec<usize>,
    oracle_detail: Option<oracle_declarations::DeclarationNodes>,
}

fn oracle_declaration_name(kind: &str) -> Option<&'static str> {
    match kind {
        "parameter" => Some("parameter_name"),
        "type_declaration" => Some("identifier"),
        "create_function_body" => Some("function_name"),
        "create_procedure_body" => Some("procedure_name"),
        "function_body" | "function_spec" | "procedure_body" | "procedure_spec" => {
            Some("identifier")
        }
        "create_package" | "create_package_body" => Some("package_name"),
        _ => None,
    }
}
fn oracle_callable_label(kind: &str) -> Option<&'static str> {
    match kind {
        "create_function_body" | "function_body" | "function_spec" => Some("Function"),
        "create_procedure_body" | "procedure_body" | "procedure_spec" => Some("Procedure"),
        "create_package" | "create_package_body" => Some("Package"),
        _ => None,
    }
}
fn oracle_declaration_label(kind: &str) -> Option<&'static str> {
    match kind {
        "parameter" => Some("Parameter"),
        "type_declaration" => Some("Type"),
        _ => oracle_callable_label(kind),
    }
}
fn oracle_owner(kind: &str) -> bool {
    matches!(kind, "create_table" | "type_declaration") || oracle_callable_label(kind).is_some()
}

/// Private acquisition owner: public raw observations cannot reconstruct this value.
pub struct ParsedAntlrDocument<'profile, 'o, 'v, 'request> {
    raw: SemanticDocument<'profile, 'o, 'v>,
    identity: IdentityDocument<'o, 'request>,
    source: SourceText<'o>,
    prepared: SourceText<'v>,
    original: &'request SnapshotContext,
    declarations: HashMap<usize, Declaration>,
    db2: Option<db2::Registry>,
    applications: Vec<oracle_applications::ApplicationNodes>,
    bindings: oracle_bindings::Registry<'o>,
    origins: HashMap<usize, OriginalProjection<'o>>,
    rejections: HashMap<usize, Arc<Regex>>,
}
impl<'profile: 'o, 'o, 'v, 'request> ParsedAntlrDocument<'profile, 'o, 'v, 'request> {
    pub fn collect(
        profile: &'profile EffectiveProfile,
        input: &PreparedInput<'o, 'v, '_, 'request>,
        relative_components: &'o [&'o str],
        limits: IdentityLimits,
    ) -> Result<Self, AntlrModelError> {
        let original = input.original_context();
        original.check()?;
        let result = (|| {
            if !matches!(
                (
                    profile.language().value().as_str(),
                    profile.grammar().value().as_str()
                ),
                ("plsql", "antlr-plsql") | ("db2", "antlr-db2")
            ) || *profile.parser().value() != ProfileParser::Antlr4
            {
                return Err(AntlrModelError::UnsupportedProfile);
            }
            let mut rejections = HashMap::new();
            let mut policies: HashMap<&str, Arc<Regex>> = HashMap::new();
            for (index, entry) in profile.queries().value().iter().enumerate() {
                original.check()?;
                let query = entry.value();
                let policy = query.name_reject().value().as_str();
                if *query.r#type().value() == QueryKind::Entity && !policy.is_empty() {
                    let compiled = match policies.get(policy) {
                        Some(compiled) => Arc::clone(compiled),
                        None => {
                            let compiled = Arc::new(Regex::new(policy).map_err(|cause| {
                                AntlrModelError::InvalidQueryPolicy {
                                    query_index: index,
                                    cause,
                                }
                            })?);
                            policies.insert(policy, Arc::clone(&compiled));
                            compiled
                        }
                    };
                    rejections.insert(index, compiled);
                }
            }
            let raw = parse_semantic_occurrences(profile, input)?;
            let source = SourceText::new(input.original_text().as_bytes(), limits.source_bytes())?;
            let prepared = SourceText::new(
                input.prepared_text().as_bytes(),
                input.prepared_text().len().max(1),
            )?;
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
            let db2 = if profile.grammar().value() == "antlr-db2" {
                Some(db2::Registry::build(&raw, original)?)
            } else {
                None
            };
            let mut declarations = HashMap::new();
            for (index, node) in raw.syntax.nodes.iter().enumerate() {
                original.check()?;
                if db2.is_none()
                    && node.class == NodeClass::Rule
                    && (matches!(
                        node.kind.as_str(),
                        "create_table" | "column_definition" | "variable_declaration"
                    ) || oracle_declaration_name(&node.kind).is_some())
                {
                    declarations.insert(
                        index,
                        Declaration {
                            name: None,
                            r#type: None,
                            recovered: false,
                            name_components: Vec::new(),
                            oracle_detail: None,
                        },
                    );
                }
            }
            for (index, node) in raw.syntax.nodes.iter().enumerate() {
                original.check()?;
                if node.parent.is_some_and(|parent| parent >= index) {
                    return Err(AntlrModelError::NativeContract(
                        "parent is not a native ancestor",
                    ));
                }
                if let Some(parent) = node.parent
                    && let Some(declaration) = declarations.get_mut(&parent)
                {
                    let expected = match raw.syntax.nodes[parent].kind.as_str() {
                        "create_table" => "table_name",
                        "column_definition" => "column_name",
                        kind => oracle_declaration_name(kind).unwrap_or("identifier"),
                    };
                    if node.class == NodeClass::Rule && node.kind == expected {
                        // The optional package_name following END is a reference.
                        // The mandatory header is the first direct package_name in the G4.
                        if declaration.name.is_none() {
                            declaration.name = Some(index);
                        } else if !matches!(
                            raw.syntax.nodes[parent].kind.as_str(),
                            "create_package" | "create_package_body"
                        ) {
                            return Err(AntlrModelError::NativeContract(
                                "multiple canonical declaration names",
                            ));
                        }
                    }
                    if node.class == NodeClass::Rule
                        && matches!(node.kind.as_str(), "datatype" | "type_name" | "type_spec")
                    {
                        declaration.r#type = Some(index);
                    }
                }
                if node.error
                    || node.missing
                    || node.class == NodeClass::Error
                    || matches!(node.status, SpanStatus::Unindexed(_))
                {
                    let mut ancestor = Some(index);
                    while let Some(parent) = ancestor {
                        original.check()?;
                        if let Some(declaration) = declarations.get_mut(&parent) {
                            declaration.recovered = true;
                        }
                        ancestor = raw.syntax.nodes[parent].parent;
                    }
                }
            }
            for (&declaration_index, declaration) in &mut declarations {
                original.check()?;
                if oracle_declaration_name(&raw.syntax.nodes[declaration_index].kind).is_none() {
                    continue;
                }
                declaration.oracle_detail =
                    oracle_declarations::collect(&raw.syntax.nodes, declaration_index, original)?;
                let Some(name) = declaration.name else {
                    continue;
                };
                let schema = raw
                    .syntax
                    .nodes
                    .iter()
                    .enumerate()
                    .find_map(|(index, node)| {
                        (node.parent == Some(declaration_index)
                            && node.class == NodeClass::Rule
                            && node.kind == "schema_object_name")
                            .then_some(index)
                    });
                let mut unsupported = false;
                for root in schema.into_iter().chain(std::iter::once(name)) {
                    for (index, node) in raw.syntax.nodes.iter().enumerate() {
                        original.check()?;
                        let mut ancestor = Some(index);
                        let mut belongs = false;
                        while let Some(parent) = ancestor {
                            if parent == root {
                                belongs = true;
                                break;
                            }
                            ancestor = raw.syntax.nodes[parent].parent;
                        }
                        if belongs {
                            unsupported |=
                                node.class == NodeClass::Terminal && node.kind == "INTRODUCER";
                            if node.class == NodeClass::Rule && node.kind == "id_expression" {
                                declaration.name_components.push(index);
                            }
                        }
                    }
                }
                if unsupported || !(1..=2).contains(&declaration.name_components.len()) {
                    declaration.name_components.clear();
                }
            }
            let applications = oracle_applications::collect(&raw, original)?;
            let bindings = oracle_bindings::Registry::collect(
                &raw,
                &declarations,
                &applications,
                input,
                original,
            )?;
            let mut origins = HashMap::new();
            for (&index, declaration) in &declarations {
                for selected in [Some(index), declaration.name, declaration.r#type]
                    .into_iter()
                    .flatten()
                    .chain(declaration.name_components.iter().copied())
                    .chain(
                        declaration
                            .oracle_detail
                            .as_ref()
                            .into_iter()
                            .flat_map(|d| d.indices()),
                    )
                {
                    original.check()?;
                    let node = &raw.syntax.nodes[selected];
                    if node.status == SpanStatus::Regular
                        && let Some(span) = &node.span
                    {
                        origins.insert(
                            selected,
                            input.project_prepared(
                                u64::try_from(span.start).map_err(|_| {
                                    AntlrModelError::NativeContract("native start overflow")
                                })?,
                                u64::try_from(span.end).map_err(|_| {
                                    AntlrModelError::NativeContract("native end overflow")
                                })?,
                            )?,
                        );
                    }
                }
            }
            if let Some(registry) = &db2 {
                for selected in registry.required_origins(&raw.syntax.nodes, original)? {
                    original.check()?;
                    let node = &raw.syntax.nodes[selected];
                    if node.status == SpanStatus::Regular
                        && let Some(span) = &node.span
                    {
                        origins.insert(
                            selected,
                            input.project_prepared(
                                u64::try_from(span.start).map_err(|_| {
                                    AntlrModelError::NativeContract("native start overflow")
                                })?,
                                u64::try_from(span.end).map_err(|_| {
                                    AntlrModelError::NativeContract("native end overflow")
                                })?,
                            )?,
                        );
                    }
                }
            }
            for selected in applications
                .iter()
                .flat_map(|application| application.indices())
                .chain(bindings.required_origins(&raw.syntax.nodes, &declarations))
                .chain(if db2.is_none() {
                    oracle_targets::required_origins(&raw)
                } else {
                    Vec::new()
                })
            {
                original.check()?;
                if origins.contains_key(&selected) {
                    continue;
                }
                let node = &raw.syntax.nodes[selected];
                if node.status == SpanStatus::Regular
                    && let Some(span) = &node.span
                {
                    origins.insert(
                        selected,
                        input.project_prepared(
                            u64::try_from(span.start).map_err(|_| {
                                AntlrModelError::NativeContract("native start overflow")
                            })?,
                            u64::try_from(span.end).map_err(|_| {
                                AntlrModelError::NativeContract("native end overflow")
                            })?,
                        )?,
                    );
                }
            }
            Ok(Self {
                raw,
                identity,
                source,
                prepared,
                original,
                declarations,
                db2,
                applications,
                bindings,
                origins,
                rejections,
            })
        })();
        original.check()?;
        result
    }
    pub fn observations(&self) -> &SemanticDocument<'profile, 'o, 'v> {
        &self.raw
    }
    pub(crate) fn source(&self) -> &'o str {
        self.source.as_str()
    }
    pub(crate) fn original_context(&self) -> &'request SnapshotContext {
        self.original
    }
    pub(crate) fn identity_document(&self) -> &IdentityDocument<'o, 'request> {
        &self.identity
    }
    pub fn assemble(
        &self,
    ) -> Result<AntlrSemanticDocument<'_, 'profile, 'o, 'v, 'request>, AntlrModelError> {
        self.original.check()?;
        let result = (|| {
            let mut assembly = Assembly {
                parsed: self,
                entities: Vec::new(),
                diagnostics: Vec::new(),
                indices: HashMap::new(),
            };
            let mut relations = Vec::new();
            for (index, occurrence) in self.raw.occurrences.iter().enumerate() {
                self.original.check()?;
                if *occurrence.query.r#type().value() == QueryKind::Relation {
                    relations.push(index);
                } else {
                    assembly.observe(index, occurrence)?;
                }
            }
            assembly.associate_diagnostics()?;
            let mut owner_indices: HashMap<&str, Vec<usize>> = HashMap::new();
            for (index, entity) in assembly.entities.iter().enumerate() {
                self.original.check()?;
                owner_indices
                    .entry(entity.identity.uid().as_str())
                    .or_default()
                    .push(index);
            }
            let mut owner_links = Vec::new();
            for (child_index, entity) in assembly.entities.iter().enumerate() {
                for (namespace_index, owner) in entity.namespace.iter().enumerate() {
                    self.original.check()?;
                    let owner_entity_indices = owner_indices
                        .get(owner.identity.uid().as_str())
                        .cloned()
                        .unwrap_or_default();
                    owner_links.push(AntlrOwnerLink {
                        child_entity_index: child_index,
                        namespace_index,
                        owner_entity_indices,
                    });
                }
            }
            let oracle_applications: Vec<_> = self
                .applications
                .iter()
                .map(|application| {
                    oracle_applications::observe(application, &self.raw.syntax.nodes, &self.origins)
                })
                .collect();
            let oracle_bindings = oracle_applications
                .iter()
                .map(|application| self.bindings.bind(self, &assembly.entities, application))
                .collect::<Result<Vec<_>, _>>()?;
            let oracle_calls = oracle_calls::resolve(
                self,
                &assembly.entities,
                &oracle_applications,
                &oracle_bindings,
                &assembly.diagnostics,
            )?;
            Ok(AntlrSemanticDocument {
                parsed: self,
                entities: assembly.entities,
                diagnostics: assembly.diagnostics,
                owner_links,
                relations,
                oracle_applications,
                oracle_bindings,
                oracle_calls,
            })
        })();
        self.original.check()?;
        result
    }
    fn node(&self, index: usize) -> &SyntaxNode {
        &self.raw.syntax.nodes[index]
    }
    fn declaration(&self, index: usize) -> Option<usize> {
        if self.declarations.contains_key(&index) {
            return Some(index);
        }
        let node = self.node(index);
        node.parent.filter(|parent| {
            self.declarations.contains_key(parent)
                && matches!(
                    node.kind.as_str(),
                    "table_name"
                        | "column_name"
                        | "identifier"
                        | "function_name"
                        | "procedure_name"
                        | "package_name"
                        | "parameter_name"
                )
        })
    }
    fn descendant(&self, mut index: usize, ancestor: usize) -> Result<bool, AntlrModelError> {
        loop {
            self.original.check()?;
            if index == ancestor {
                return Ok(true);
            }
            let Some(parent) = self.node(index).parent else {
                return Ok(false);
            };
            index = parent;
        }
    }
    fn extent(&self, index: usize) -> Result<ExtentObservation<'_, 'o>, AntlrModelError> {
        match self.origins.get(&index) {
            Some(OriginalProjection::Exact(range)) => Ok(ExtentObservation::Exact(
                EntityExtent::new(&self.source, *range, self.original)?,
            )),
            Some(origin) => Ok(ExtentObservation::NonExact(origin)),
            None => Ok(ExtentObservation::Missing),
        }
    }
}

#[derive(Debug)]
pub enum AntlrIssue {
    MissingNativeField {
        role: &'static str,
        parent_node_index: usize,
    },
    UnsupportedDeclaration,
    DeclarationReference {
        declaration_node_index: usize,
    },
    UnsupportedOwner {
        node_index: usize,
    },
    MissingName,
    MissingSpan,
    MissingParent,
    NonExact {
        role: &'static str,
        node_index: usize,
    },
    Recovery {
        node_index: usize,
    },
    InvalidSpan,
    NameKindMismatch,
    NameCodec(TokenizerError),
    NameSyntax(sqlparser::parser::ParserError),
    InvalidExplicitParent,
    OwnerConflict,
    TypeConflict {
        first_contribution: usize,
        second_contribution: usize,
    },
    ExcludedName,
    UnsupportedDecodedLiteral(TokenizerError),
}
pub struct AntlrDiagnostic<'doc, 'o> {
    pub occurrence_index: usize,
    pub declaration_node_index: Option<usize>,
    pub subject_node_index: Option<usize>,
    pub issue: AntlrIssue,
    pub original: Option<&'doc OriginalProjection<'o>>,
}
pub enum AntlrValueObservation<'doc, 'o> {
    NotConfigured,
    Missing,
    NonExact(&'doc OriginalProjection<'o>),
    Scalar { raw: SourceRange<'o>, token: Token },
    Expression { raw: SourceRange<'o> },
}
impl AntlrValueObservation<'_, '_> {
    fn agrees(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Scalar { token: a, .. }, Self::Scalar { token: b, .. }) => a == b,
            (Self::Expression { raw: a }, Self::Expression { raw: b }) => a.text() == b.text(),
            (Self::Missing, Self::Missing) => true,
            _ => false,
        }
    }
    fn configured_value(&self) -> bool {
        matches!(
            self,
            Self::Missing | Self::Scalar { .. } | Self::Expression { .. }
        )
    }
}
pub enum AntlrTypeObservation<'doc, 'o> {
    Missing,
    NonExact(&'doc OriginalProjection<'o>),
    Present {
        node_index: usize,
        raw: SourceRange<'o>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AntlrTypeRole {
    Declared,
    Return,
    CastSource,
    CastTarget,
    ReturnColumn,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AntlrDeclarationRole {
    Declaration,
    IndexKeySpecification,
}
impl AntlrTypeObservation<'_, '_> {
    fn agrees(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Missing, Self::Missing) => true,
            (
                Self::Present {
                    raw: a,
                    node_index: an,
                },
                Self::Present {
                    raw: b,
                    node_index: bn,
                },
            ) => an == bn && a.text() == b.text(),
            (Self::NonExact(a), Self::NonExact(b)) => std::ptr::eq(*a, *b),
            _ => false,
        }
    }
}
pub struct AntlrTypedObservation<'doc, 'o> {
    pub role: AntlrTypeRole,
    pub name_node_index: Option<usize>,
    pub observation: AntlrTypeObservation<'doc, 'o>,
}
pub struct AntlrQualifiedNameComponent<'o> {
    pub node_index: usize,
    pub raw: SourceRange<'o>,
    pub decoded: Arc<str>,
    pub quote_style: Option<char>,
}
pub struct AntlrQualifiedName<'o> {
    components: Vec<AntlrQualifiedNameComponent<'o>>,
}
impl<'o> AntlrQualifiedName<'o> {
    pub fn components(&self) -> &[AntlrQualifiedNameComponent<'o>] {
        &self.components
    }
}
pub struct AntlrContribution<'doc, 'profile, 'o, 'v> {
    pub occurrence_index: usize,
    pub occurrence: &'doc SemanticOccurrence<'profile, 'o, 'v>,
    pub extent: ExtentObservation<'doc, 'o>,
    pub declared_value: AntlrValueObservation<'doc, 'o>,
    pub declared_type: AntlrTypeObservation<'doc, 'o>,
    pub type_roles: Vec<AntlrTypedObservation<'doc, 'o>>,
}
pub struct AntlrNamespace<'doc, 'o, 'request> {
    identity: SyntaxIdentity<'doc, 'o, 'request>,
    name: Arc<str>,
    node_index: usize,
    context_node_index: usize,
    qualified_name: Option<AntlrQualifiedName<'o>>,
    extent: ExtentObservation<'doc, 'o>,
}
impl<'doc, 'o, 'request> AntlrNamespace<'doc, 'o, 'request> {
    pub fn identity(&self) -> &SyntaxIdentity<'doc, 'o, 'request> {
        &self.identity
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn anchor(&self) -> SourceRange<'o> {
        *self.identity.subject()
    }
    pub fn qualified_name(&self) -> Option<&AntlrQualifiedName<'o>> {
        self.qualified_name.as_ref()
    }
    pub fn context_node_index(&self) -> usize {
        self.context_node_index
    }
    pub fn node_index(&self) -> usize {
        self.node_index
    }
    pub fn extent(&self) -> &ExtentObservation<'doc, 'o> {
        &self.extent
    }
}
pub struct AntlrEntity<'doc, 'profile, 'o, 'v, 'request> {
    identity: SyntaxIdentity<'doc, 'o, 'request>,
    data_key: &'doc str,
    name: Arc<str>,
    declaration_node_index: usize,
    name_node_index: usize,
    role: AntlrDeclarationRole,
    qualified_name: Option<AntlrQualifiedName<'o>>,
    oracle_declaration: Option<AntlrOracleDeclaration<'doc, 'o>>,
    namespace: Vec<AntlrNamespace<'doc, 'o, 'request>>,
    contributions: Vec<AntlrContribution<'doc, 'profile, 'o, 'v>>,
    conflicts: Vec<ContributionConflict>,
    diagnostic_indices: Vec<usize>,
}
pub enum AntlrEntityExtent<'a, 'doc, 'profile, 'o, 'v> {
    Exact(&'a EntityExtent<'o>),
    NonExact(&'doc OriginalProjection<'o>),
    Missing,
    Conflict(&'a [AntlrContribution<'doc, 'profile, 'o, 'v>]),
}
impl<'doc, 'profile, 'o, 'v, 'request> AntlrEntity<'doc, 'profile, 'o, 'v, 'request> {
    pub fn identity(&self) -> &SyntaxIdentity<'doc, 'o, 'request> {
        &self.identity
    }
    pub fn data_key(&self) -> &'doc str {
        self.data_key
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn oracle_declaration(&self) -> Option<&AntlrOracleDeclaration<'doc, 'o>> {
        self.oracle_declaration.as_ref()
    }
    pub fn role(&self) -> AntlrDeclarationRole {
        self.role
    }
    pub fn qualified_name(&self) -> Option<&AntlrQualifiedName<'o>> {
        self.qualified_name.as_ref()
    }
    pub fn name_raw(&self) -> SourceRange<'o> {
        *self.identity.subject()
    }
    pub fn declaration_node_index(&self) -> usize {
        self.declaration_node_index
    }
    pub fn namespace(&self) -> &[AntlrNamespace<'doc, 'o, 'request>] {
        &self.namespace
    }
    pub fn contributions(&self) -> &[AntlrContribution<'doc, 'profile, 'o, 'v>] {
        &self.contributions
    }
    pub fn conflicts(&self) -> &[ContributionConflict] {
        &self.conflicts
    }
    pub fn diagnostic_indices(&self) -> &[usize] {
        &self.diagnostic_indices
    }
    pub fn extent(&self) -> AntlrEntityExtent<'_, 'doc, 'profile, 'o, 'v> {
        if self
            .conflicts
            .iter()
            .any(|c| c.field == ContributionConflictField::Extent)
        {
            return AntlrEntityExtent::Conflict(&self.contributions);
        }
        for contribution in &self.contributions {
            match contribution.extent {
                ExtentObservation::NonExact(origin) => return AntlrEntityExtent::NonExact(origin),
                ExtentObservation::Missing => return AntlrEntityExtent::Missing,
                ExtentObservation::Exact(_) => {}
            }
        }
        match &self.contributions[0].extent {
            ExtentObservation::Exact(extent) => AntlrEntityExtent::Exact(extent),
            ExtentObservation::NonExact(origin) => AntlrEntityExtent::NonExact(origin),
            ExtentObservation::Missing => AntlrEntityExtent::Missing,
        }
    }
}
pub struct AntlrOwnerLink {
    pub child_entity_index: usize,
    pub namespace_index: usize,
    pub owner_entity_indices: Vec<usize>,
}
pub struct AntlrSemanticDocument<'doc, 'profile, 'o, 'v, 'request> {
    parsed: &'doc ParsedAntlrDocument<'profile, 'o, 'v, 'request>,
    entities: Vec<AntlrEntity<'doc, 'profile, 'o, 'v, 'request>>,
    diagnostics: Vec<AntlrDiagnostic<'doc, 'o>>,
    owner_links: Vec<AntlrOwnerLink>,
    relations: Vec<usize>,
    oracle_applications: Vec<AntlrOracleApplication<'doc, 'o>>,
    oracle_bindings: Vec<AntlrOracleApplicationBinding<'doc, 'o>>,
    oracle_calls: Vec<AntlrOracleCallOutcome<'doc, 'o>>,
}
pub struct AntlrCodeProjection<'a, 'doc, 'profile, 'o, 'v, 'request> {
    pub entity_index: usize,
    pub entity: &'a AntlrEntity<'doc, 'profile, 'o, 'v, 'request>,
    pub extent: &'a EntityExtent<'o>,
    pub text: &'o str,
    pub language: &'a str,
    pub grammar: &'a str,
}
pub struct AntlrUnavailableProjection<'a> {
    pub entity_index: usize,
    pub diagnostics: &'a [usize],
    pub conflicts: &'a [ContributionConflict],
}
pub enum AntlrProjection<'a, 'doc, 'profile, 'o, 'v, 'request> {
    Available(AntlrCodeProjection<'a, 'doc, 'profile, 'o, 'v, 'request>),
    Unavailable(AntlrUnavailableProjection<'a>),
}
impl<'doc, 'profile: 'o, 'o, 'v, 'request> AntlrSemanticDocument<'doc, 'profile, 'o, 'v, 'request> {
    pub fn parsed(&self) -> &'doc ParsedAntlrDocument<'profile, 'o, 'v, 'request> {
        self.parsed
    }
    pub fn entities(&self) -> &[AntlrEntity<'doc, 'profile, 'o, 'v, 'request>] {
        &self.entities
    }
    pub fn diagnostics(&self) -> &[AntlrDiagnostic<'doc, 'o>] {
        &self.diagnostics
    }
    pub fn owner_links(&self) -> &[AntlrOwnerLink] {
        &self.owner_links
    }
    /// Complete retained relation occurrence inventory, including locally resolved Oracle calls.
    pub fn unresolved_relations(&self) -> &[usize] {
        &self.relations
    }
    /// Native Oracle application syntax, one entry per eligible retained occurrence.
    pub fn oracle_applications(&self) -> &[AntlrOracleApplication<'doc, 'o>] {
        &self.oracle_applications
    }
    /// Absence means the occurrence is outside this producer's supported native forms.
    pub fn oracle_application(
        &self,
        occurrence_index: usize,
    ) -> Option<&AntlrOracleApplication<'doc, 'o>> {
        self.oracle_applications
            .iter()
            .find(|application| application.occurrence_index == occurrence_index)
    }
    /// Local binding facts do not grant a resolved CALLS endpoint.
    pub fn oracle_bindings(&self) -> &[AntlrOracleApplicationBinding<'doc, 'o>] {
        &self.oracle_bindings
    }
    pub fn oracle_binding(
        &self,
        occurrence_index: usize,
    ) -> Option<&AntlrOracleApplicationBinding<'doc, 'o>> {
        self.oracle_bindings
            .iter()
            .find(|binding| binding.occurrence_index == occurrence_index)
    }
    /// Local Oracle callable references; absence preserves the raw/B1/B2 observation.
    pub fn oracle_calls(&self) -> &[AntlrOracleCallOutcome<'doc, 'o>] {
        &self.oracle_calls
    }
    pub fn oracle_call(
        &self,
        occurrence_index: usize,
    ) -> Option<&AntlrOracleCallOutcome<'doc, 'o>> {
        self.oracle_calls
            .iter()
            .find(|c| c.occurrence_index == occurrence_index)
    }
    pub fn code_projections(
        &self,
    ) -> Result<Vec<AntlrProjection<'_, 'doc, 'profile, 'o, 'v, 'request>>, AntlrModelError> {
        self.parsed.original.check()?;
        let result = (|| {
            let profile = self.parsed.raw.profile;
            let mut projections = Vec::new();
            for (entity_index, entity) in self.entities.iter().enumerate() {
                self.parsed.original.check()?;
                if !profile
                    .embed_labels()
                    .value()
                    .iter()
                    .any(|label| label.value() == entity.identity.label())
                {
                    continue;
                }
                if entity.conflicts.is_empty()
                    && entity.diagnostic_indices.is_empty()
                    && entity
                        .namespace
                        .iter()
                        .all(|owner| matches!(owner.extent, ExtentObservation::Exact(_)))
                    && let AntlrEntityExtent::Exact(extent) = entity.extent()
                {
                    projections.push(AntlrProjection::Available(AntlrCodeProjection {
                        entity_index,
                        entity,
                        extent,
                        text: extent.range().text(),
                        language: profile.language().value(),
                        grammar: profile.grammar().value(),
                    }));
                } else {
                    projections.push(AntlrProjection::Unavailable(AntlrUnavailableProjection {
                        entity_index,
                        diagnostics: &entity.diagnostic_indices,
                        conflicts: &entity.conflicts,
                    }));
                }
            }
            Ok(projections)
        })();
        self.parsed.original.check()?;
        result
    }
}

fn identifier(text: &str) -> Result<Option<Arc<str>>, TokenizerError> {
    let mut tokens = Tokenizer::new(&GenericDialect, text)
        .with_unescape(true)
        .tokenize()?
        .into_iter();
    match (tokens.next(), tokens.next()) {
        (Some(Token::Word(word)), None)
            if !word.value.is_empty() && matches!(word.quote_style, None | Some('"')) =>
        {
            Ok(Some(Arc::from(word.value)))
        }
        _ => Ok(None),
    }
}

struct Assembly<'doc, 'profile, 'o, 'v, 'request> {
    parsed: &'doc ParsedAntlrDocument<'profile, 'o, 'v, 'request>,
    entities: Vec<AntlrEntity<'doc, 'profile, 'o, 'v, 'request>>,
    diagnostics: Vec<AntlrDiagnostic<'doc, 'o>>,
    indices: HashMap<(String, &'doc str), usize>,
}
impl<'doc, 'profile: 'o, 'o, 'v, 'request> Assembly<'doc, 'profile, 'o, 'v, 'request> {
    fn diagnostic(
        &mut self,
        occurrence_index: usize,
        declaration_node_index: Option<usize>,
        issue: AntlrIssue,
        original: Option<&'doc OriginalProjection<'o>>,
    ) {
        self.diagnostics.push(AntlrDiagnostic {
            occurrence_index,
            declaration_node_index,
            subject_node_index: self.parsed.db2.as_ref().and_then(|r| {
                let matched = self.parsed.raw.occurrences[occurrence_index]
                    .matched
                    .node_index;
                if r.subjects.contains_key(&matched) {
                    Some(matched)
                } else {
                    match &self.parsed.raw.occurrences[occurrence_index].name {
                        CaptureBinding::Present(n) => r.selected(matched, n.node_index),
                        _ => r.unique(matched),
                    }
                }
            }),
            issue,
            original,
        });
    }
    fn name(
        &mut self,
        index: usize,
        declaration: usize,
        node_index: usize,
        prepared: Option<SourceRange<'v>>,
        origin: Option<&'doc OriginalProjection<'o>>,
    ) -> Result<Option<(Arc<str>, SourceRange<'o>)>, AntlrModelError> {
        self.parsed.original.check()?;
        let Some(OriginalProjection::Exact(raw)) = origin else {
            self.diagnostic(
                index,
                Some(declaration),
                AntlrIssue::NonExact {
                    role: "name",
                    node_index,
                },
                origin,
            );
            return Ok(None);
        };
        if self.parsed.node(node_index).error
            || self.parsed.node(node_index).missing
            || self.parsed.node(node_index).status != SpanStatus::Regular
        {
            self.diagnostic(
                index,
                Some(declaration),
                AntlrIssue::Recovery { node_index },
                origin,
            );
            return Ok(None);
        }
        let decoded = match identifier(raw.text()) {
            Ok(Some(name)) => name,
            Ok(None) => {
                self.diagnostic(
                    index,
                    Some(declaration),
                    AntlrIssue::NameKindMismatch,
                    origin,
                );
                return Ok(None);
            }
            Err(cause) => {
                self.diagnostic(
                    index,
                    Some(declaration),
                    AntlrIssue::NameCodec(cause),
                    origin,
                );
                return Ok(None);
            }
        };
        let Some(prepared) = prepared else {
            self.diagnostic(
                index,
                Some(declaration),
                AntlrIssue::NameKindMismatch,
                origin,
            );
            return Ok(None);
        };
        match identifier(prepared.text()) {
            Ok(Some(value)) if value == decoded => {}
            Ok(_) => {
                self.diagnostic(
                    index,
                    Some(declaration),
                    AntlrIssue::NameKindMismatch,
                    origin,
                );
                return Ok(None);
            }
            Err(cause) => {
                self.diagnostic(
                    index,
                    Some(declaration),
                    AntlrIssue::NameCodec(cause),
                    origin,
                );
                return Ok(None);
            }
        };
        Ok(Some((decoded, *raw)))
    }
    fn oracle_name(
        &mut self,
        occurrence: usize,
        declaration: usize,
    ) -> Result<Option<AntlrQualifiedName<'o>>, AntlrModelError> {
        let indices = self.parsed.declarations[&declaration]
            .name_components
            .clone();
        if indices.is_empty() {
            self.diagnostic(
                occurrence,
                Some(declaration),
                AntlrIssue::NameKindMismatch,
                self.parsed.origins.get(&declaration),
            );
            return Ok(None);
        }
        let mut components = Vec::with_capacity(indices.len());
        for node_index in indices {
            self.parsed.original.check()?;
            let prepared = self
                .parsed
                .node(node_index)
                .span
                .as_ref()
                .map(|span| self.parsed.prepared.range(span.start, span.end))
                .transpose()?;
            let Some((decoded, raw)) = self.name(
                occurrence,
                declaration,
                node_index,
                prepared,
                self.parsed.origins.get(&node_index),
            )?
            else {
                return Ok(None);
            };
            // The same mature codec already validated exactly one Word above.
            let Ok(tokens) = Tokenizer::new(&GenericDialect, raw.text())
                .with_unescape(true)
                .tokenize()
            else {
                return Err(AntlrModelError::NativeContract(
                    "validated Oracle name codec diverged",
                ));
            };
            let [Token::Word(word)] = tokens.as_slice() else {
                return Err(AntlrModelError::NativeContract(
                    "validated Oracle name shape diverged",
                ));
            };
            components.push(AntlrQualifiedNameComponent {
                node_index,
                raw,
                decoded,
                quote_style: word.quote_style,
            });
        }
        Ok(Some(AntlrQualifiedName { components }))
    }
    fn canonical_name(
        &mut self,
        occurrence: usize,
        declaration: usize,
    ) -> Result<Option<(Arc<str>, SourceRange<'o>)>, AntlrModelError> {
        let Some(node_index) = self.parsed.declarations[&declaration].name else {
            self.diagnostic(occurrence, Some(declaration), AntlrIssue::MissingName, None);
            return Ok(None);
        };
        let prepared = self
            .parsed
            .node(node_index)
            .span
            .as_ref()
            .map(|span| self.parsed.prepared.range(span.start, span.end))
            .transpose()?;
        self.name(
            occurrence,
            declaration,
            node_index,
            prepared,
            self.parsed.origins.get(&node_index),
        )
    }
    fn owners(
        &mut self,
        index: usize,
        occurrence: &'doc SemanticOccurrence<'profile, 'o, 'v>,
        declaration: usize,
    ) -> Result<Option<Vec<AntlrNamespace<'doc, 'o, 'request>>>, AntlrModelError> {
        if self.parsed.db2.is_some() {
            return self.db2_owners(index, occurrence, declaration);
        }
        let mut selected = Vec::new();
        for &context_index in occurrence.contexts.iter().rev() {
            self.parsed.original.check()?;
            let context = &self.parsed.raw.contexts[context_index];
            let owner = context.matched.node_index;
            if owner == declaration {
                continue;
            }
            let Some(label) = context.label else { continue };
            if !oracle_owner(&self.parsed.node(owner).kind)
                || !self.parsed.declarations.contains_key(&owner)
            {
                self.diagnostic(
                    index,
                    Some(declaration),
                    AntlrIssue::UnsupportedOwner { node_index: owner },
                    context.matched.original.as_ref(),
                );
                return Ok(None);
            }
            if !self.parsed.descendant(declaration, owner)? {
                return Err(AntlrModelError::NativeContract(
                    "context is not a declaration ancestor",
                ));
            }
            if context.name_is_explicit {
                let CaptureBinding::Present(name) = &context.name else {
                    self.diagnostic(index, Some(declaration), AntlrIssue::MissingParent, None);
                    return Ok(None);
                };
                if Some(name.node_index) != self.parsed.declarations[&owner].name {
                    self.diagnostic(
                        index,
                        Some(declaration),
                        AntlrIssue::InvalidExplicitParent,
                        name.original.as_ref(),
                    );
                    return Ok(None);
                }
                if !matches!(name.original, Some(OriginalProjection::Exact(_))) {
                    self.diagnostic(
                        index,
                        Some(declaration),
                        AntlrIssue::NonExact {
                            role: "owner",
                            node_index: name.node_index,
                        },
                        name.original.as_ref(),
                    );
                    return Ok(None);
                }
            }
            selected.push((owner, label));
        }
        match &occurrence.parent {
            CaptureBinding::NotConfigured => {}
            CaptureBinding::Missing => {
                self.diagnostic(index, Some(declaration), AntlrIssue::MissingParent, None);
                return Ok(None);
            }
            CaptureBinding::Present(parent) => {
                let label = occurrence.query.parent_label().value().as_str();
                let candidate = self.parsed.declaration(parent.node_index);
                let Some(owner) = candidate else {
                    self.diagnostic(
                        index,
                        Some(declaration),
                        AntlrIssue::InvalidExplicitParent,
                        parent.original.as_ref(),
                    );
                    return Ok(None);
                };
                if owner == declaration
                    || label.is_empty()
                    || !self.parsed.descendant(declaration, owner)?
                    || !oracle_owner(&self.parsed.node(owner).kind)
                {
                    self.diagnostic(
                        index,
                        Some(declaration),
                        AntlrIssue::InvalidExplicitParent,
                        parent.original.as_ref(),
                    );
                    return Ok(None);
                }
                if let Some((_, existing)) = selected.iter().find(|(node, _)| *node == owner) {
                    if *existing != label {
                        self.diagnostic(
                            index,
                            Some(declaration),
                            AntlrIssue::OwnerConflict,
                            parent.original.as_ref(),
                        );
                        return Ok(None);
                    }
                } else {
                    selected.push((owner, label));
                    selected.sort_by_key(|(node, _)| *node);
                }
            }
        }
        let mut namespace: Vec<AntlrNamespace<'doc, 'o, 'request>> = Vec::new();
        for (owner, label) in selected {
            let qualified_name = if oracle_declaration_name(&self.parsed.node(owner).kind).is_some()
            {
                let Some(value) = self.oracle_name(index, owner)? else {
                    return Ok(None);
                };
                Some(value)
            } else {
                None
            };
            let (name, anchor) = if let Some(value) = &qualified_name {
                let last = value
                    .components
                    .last()
                    .expect("nonempty native Oracle name");
                (Arc::clone(&last.decoded), last.raw)
            } else {
                let Some(value) = self.canonical_name(index, owner)? else {
                    return Ok(None);
                };
                value
            };
            let extent = self.parsed.extent(owner)?;
            if self.parsed.declarations[&owner].recovered {
                self.diagnostic(
                    index,
                    Some(declaration),
                    AntlrIssue::Recovery { node_index: owner },
                    self.parsed.origins.get(&owner),
                );
            }
            if !matches!(extent, ExtentObservation::Exact(_)) {
                self.diagnostic(
                    index,
                    Some(declaration),
                    AntlrIssue::NonExact {
                        role: "owner",
                        node_index: owner,
                    },
                    self.parsed.origins.get(&owner),
                );
            }
            let preceding: Vec<_> = namespace.iter().map(|frame| &frame.identity).collect();
            let identity = self.parsed.identity.symbol(label, anchor, &preceding)?;
            namespace.push(AntlrNamespace {
                identity,
                name,
                node_index: owner,
                context_node_index: owner,
                qualified_name,
                extent,
            });
        }
        Ok(Some(namespace))
    }
    fn value(
        &mut self,
        index: usize,
        declaration: usize,
        binding: &'doc CaptureBinding<'o, 'v>,
    ) -> AntlrValueObservation<'doc, 'o> {
        let CaptureBinding::Present(value) = binding else {
            return if matches!(binding, CaptureBinding::Missing) {
                AntlrValueObservation::Missing
            } else {
                AntlrValueObservation::NotConfigured
            };
        };
        let Some(origin) = &value.original else {
            return AntlrValueObservation::Missing;
        };
        let OriginalProjection::Exact(raw) = origin else {
            return AntlrValueObservation::NonExact(origin);
        };
        match Tokenizer::new(&GenericDialect, raw.text())
            .with_unescape(true)
            .tokenize()
        {
            Ok(tokens) => {
                let meaningful: Vec<_> = tokens
                    .into_iter()
                    .filter(|token| !matches!(token, Token::Whitespace(_)))
                    .collect();
                if meaningful.len() == 1
                    && matches!(
                        &meaningful[0],
                        Token::Number(_, false)
                            | Token::SingleQuotedString(_)
                            | Token::NationalStringLiteral(_)
                    )
                {
                    return AntlrValueObservation::Scalar {
                        raw: *raw,
                        token: meaningful.into_iter().next().unwrap(),
                    };
                }
                if meaningful.len() == 1
                    && matches!(&meaningful[0],Token::Word(word) if matches!(word.keyword,Keyword::TRUE|Keyword::FALSE|Keyword::NULL) && word.quote_style.is_none())
                {
                    return AntlrValueObservation::Scalar {
                        raw: *raw,
                        token: meaningful.into_iter().next().unwrap(),
                    };
                }
            }
            Err(cause) => self.diagnostic(
                index,
                Some(declaration),
                AntlrIssue::UnsupportedDecodedLiteral(cause),
                Some(origin),
            ),
        }
        AntlrValueObservation::Expression { raw: *raw }
    }
    fn observe(
        &mut self,
        index: usize,
        occurrence: &'doc SemanticOccurrence<'profile, 'o, 'v>,
    ) -> Result<(), AntlrModelError> {
        let CaptureBinding::Present(captured) = &occurrence.name else {
            let declaration = self
                .parsed
                .db2
                .as_ref()
                .and_then(|r| {
                    r.unique(occurrence.matched.node_index)
                        .or_else(|| {
                            r.subjects
                                .contains_key(&occurrence.matched.node_index)
                                .then_some(occurrence.matched.node_index)
                        })
                        .map(|n| r.subjects[&n].declaration)
                        .or_else(|| {
                            r.declarations
                                .contains_key(&r.canonical(occurrence.matched.node_index))
                                .then_some(r.canonical(occurrence.matched.node_index))
                        })
                })
                .or_else(|| self.parsed.declaration(occurrence.matched.node_index));
            self.diagnostic(index, declaration, AntlrIssue::MissingName, None);
            return Ok(());
        };
        let subject = self
            .parsed
            .db2
            .as_ref()
            .and_then(|r| r.selected(occurrence.matched.node_index, captured.node_index))
            .and_then(|n| self.parsed.db2.as_ref()?.subjects.get(&n));
        let declaration = if self.parsed.db2.is_some() {
            let Some(s) = subject else {
                let registry = self.parsed.db2.as_ref().expect("DB2 subject path");
                let matched = registry.canonical(occurrence.matched.node_index);
                let declaration = registry
                    .declarations
                    .contains_key(&matched)
                    .then_some(matched);
                self.diagnostic(
                    index,
                    declaration,
                    if declaration.is_some() {
                        AntlrIssue::NameKindMismatch
                    } else {
                        AntlrIssue::UnsupportedDeclaration
                    },
                    occurrence.matched.original.as_ref(),
                );
                return Ok(());
            };
            s.declaration
        } else {
            let Some(d) = self.parsed.declaration(occurrence.matched.node_index) else {
                self.diagnostic(
                    index,
                    None,
                    AntlrIssue::UnsupportedDeclaration,
                    occurrence.matched.original.as_ref(),
                );
                return Ok(());
            };
            d
        };
        let label = occurrence.query.graph_label().value().as_str();
        if self.parsed.db2.is_none()
            && !(matches!(label, "Table" | "Column" | "Variable" | "Constant")
                && oracle_declaration_label(&self.parsed.node(declaration).kind).is_none()
                || oracle_declaration_label(&self.parsed.node(declaration).kind) == Some(label))
        {
            self.diagnostic(
                index,
                Some(declaration),
                AntlrIssue::UnsupportedDeclaration,
                occurrence.matched.original.as_ref(),
            );
            return Ok(());
        }
        let canonical = subject.map(|s| s.name).or_else(|| {
            self.parsed
                .declarations
                .get(&declaration)
                .and_then(|d| d.name)
        });
        if canonical != Some(captured.node_index)
            && matches!(
                self.parsed.node(declaration).kind.as_str(),
                "create_package" | "create_package_body"
            )
            && self.parsed.node(captured.node_index).parent == Some(declaration)
            && self.parsed.node(captured.node_index).kind == "package_name"
        {
            self.diagnostic(
                index,
                None,
                AntlrIssue::DeclarationReference {
                    declaration_node_index: declaration,
                },
                captured.original.as_ref(),
            );
            return Ok(());
        }
        if canonical != Some(captured.node_index) {
            self.diagnostic(
                index,
                Some(declaration),
                AntlrIssue::NameKindMismatch,
                captured.original.as_ref(),
            );
            return Ok(());
        }
        let qualified_name = if self.parsed.db2.is_some() {
            let Some(q) = self.db2_name(
                index,
                declaration,
                captured.node_index,
                captured.prepared,
                captured.original.as_ref(),
            )?
            else {
                return Ok(());
            };
            Some(q)
        } else if oracle_declaration_name(&self.parsed.node(declaration).kind).is_some() {
            let Some(q) = self.oracle_name(index, declaration)? else {
                return Ok(());
            };
            Some(q)
        } else {
            None
        };
        let (name, anchor) = if let Some(q) = &qualified_name {
            (
                Arc::clone(&q.components.last().expect("nonempty native QName").decoded),
                if self.parsed.db2.is_none() {
                    q.components
                        .last()
                        .expect("nonempty native Oracle name")
                        .raw
                } else {
                    match captured.original {
                        Some(OriginalProjection::Exact(raw)) => raw,
                        _ => {
                            return Err(AntlrModelError::NativeContract(
                                "qualified name lost exact origin",
                            ));
                        }
                    }
                },
            )
        } else {
            let Some(pair) = self.name(
                index,
                declaration,
                captured.node_index,
                captured.prepared,
                captured.original.as_ref(),
            )?
            else {
                return Ok(());
            };
            pair
        };
        let extent_node = subject.map_or(declaration, |s| s.extent);
        if self
            .parsed
            .rejections
            .get(&occurrence.query_index)
            .is_some_and(|policy| policy.is_match(&name))
        {
            self.diagnostic(
                index,
                Some(declaration),
                AntlrIssue::ExcludedName,
                captured.original.as_ref(),
            );
            return Ok(());
        };
        let Some(namespace) = self.owners(index, occurrence, declaration)? else {
            return Ok(());
        };
        let frames: Vec<_> = namespace.iter().map(|owner| &owner.identity).collect();
        let identity = self.parsed.identity.symbol(label, anchor, &frames)?;
        let extent = match &occurrence.span {
            CaptureBinding::NotConfigured => self.parsed.extent(extent_node)?,
            CaptureBinding::Missing => {
                self.diagnostic(index, Some(declaration), AntlrIssue::MissingSpan, None);
                ExtentObservation::Missing
            }
            CaptureBinding::Present(span) => {
                let extent = match &span.original {
                    Some(OriginalProjection::Exact(range)) => ExtentObservation::Exact(
                        EntityExtent::new(&self.parsed.source, *range, self.parsed.original)?,
                    ),
                    Some(origin) => ExtentObservation::NonExact(origin),
                    None => ExtentObservation::Missing,
                };
                let complete = span.node_index == declaration
                    || (self.parsed.descendant(declaration, span.node_index)?
                        && self.parsed.node(span.node_index).class == NodeClass::Rule);
                if !complete {
                    self.diagnostic(
                        index,
                        Some(declaration),
                        AntlrIssue::InvalidSpan,
                        span.original.as_ref(),
                    );
                }
                extent
            }
        };
        if subject.map_or_else(
            || self.parsed.declarations[&declaration].recovered,
            |s| s.recovered,
        ) {
            self.diagnostic(
                index,
                Some(declaration),
                AntlrIssue::Recovery {
                    node_index: declaration,
                },
                self.parsed.origins.get(&declaration),
            );
        }
        if !matches!(extent, ExtentObservation::Exact(_)) {
            self.diagnostic(
                index,
                Some(declaration),
                AntlrIssue::NonExact {
                    role: "extent",
                    node_index: declaration,
                },
                self.parsed.origins.get(&declaration),
            );
        }
        let declared_value = self.value(index, declaration, &occurrence.value);

        let type_node = subject.and_then(|s| s.own_type).or_else(|| {
            self.parsed
                .declarations
                .get(&declaration)
                .and_then(|d| d.r#type)
        });
        let declared_type = self.type_observation(type_node);
        let mut type_roles: Vec<AntlrTypedObservation<'doc, 'o>> = subject
            .map(|s| {
                s.roles
                    .iter()
                    .map(|t| AntlrTypedObservation {
                        role: t.role,
                        name_node_index: t.name,
                        observation: self.type_observation(Some(t.node)),
                    })
                    .collect()
            })
            .unwrap_or_default();
        if self.parsed.db2.is_none()
            && oracle_callable_label(&self.parsed.node(declaration).kind) == Some("Function")
            && type_node.is_some()
        {
            type_roles.push(AntlrTypedObservation {
                role: AntlrTypeRole::Return,
                name_node_index: None,
                observation: self.type_observation(type_node),
            });
        }

        if let Some(subject) = subject {
            for typed in &subject.roles {
                self.parsed.original.check()?;
                if !matches!(
                    self.parsed.origins.get(&typed.node),
                    Some(OriginalProjection::Exact(_))
                ) {
                    self.diagnostic(
                        index,
                        Some(declaration),
                        AntlrIssue::NonExact {
                            role: "type",
                            node_index: typed.node,
                        },
                        self.parsed.origins.get(&typed.node),
                    );
                }
            }
        }
        let oracle_declaration = if self.parsed.db2.is_none() {
            self.oracle_declaration(index, declaration)?
        } else {
            None
        };
        let contribution = AntlrContribution {
            occurrence_index: index,
            occurrence,
            extent,
            declared_value,
            declared_type,
            type_roles,
        };
        let data_key = occurrence.query.data_key().value().as_str();
        let key = (identity.uid().as_str().to_owned(), data_key);
        if let Some(&entity_index) = self.indices.get(&key) {
            let entity = &mut self.entities[entity_index];
            let next = entity.contributions.len();
            for (previous, other) in entity.contributions.iter().enumerate() {
                if let (ExtentObservation::Exact(a), ExtentObservation::Exact(b)) =
                    (&other.extent, &contribution.extent)
                    && (a.range().start() != b.range().start()
                        || a.range().end() != b.range().end())
                {
                    entity.conflicts.push(ContributionConflict {
                        field: ContributionConflictField::Extent,
                        first_contribution: previous,
                        second_contribution: next,
                    });
                }
                if other.declared_value.configured_value()
                    && contribution.declared_value.configured_value()
                    && !other.declared_value.agrees(&contribution.declared_value)
                {
                    entity.conflicts.push(ContributionConflict {
                        field: ContributionConflictField::Value,
                        first_contribution: previous,
                        second_contribution: next,
                    });
                }
                if matches!((&other.declared_type, &contribution.declared_type),
                    (AntlrTypeObservation::Present { raw: a, .. }, AntlrTypeObservation::Present { raw: b, .. }) if a.text() != b.text())
                    || (self.parsed.db2.is_some()
                        && (other.type_roles.len() != contribution.type_roles.len()
                            || other.type_roles.iter().zip(&contribution.type_roles).any(
                                |(a, b)| {
                                    a.role != b.role
                                        || a.name_node_index != b.name_node_index
                                        || !a.observation.agrees(&b.observation)
                                },
                            )))
                {
                    self.diagnostics.push(AntlrDiagnostic {
                        occurrence_index: index,
                        declaration_node_index: Some(declaration),
                        subject_node_index: self.parsed.db2.as_ref().map(|_| captured.node_index),
                        issue: AntlrIssue::TypeConflict {
                            first_contribution: previous,
                            second_contribution: next,
                        },
                        original: self.parsed.origins.get(&declaration),
                    });
                }
            }
            entity.contributions.push(contribution);
        } else {
            self.indices.insert(key, self.entities.len());
            self.entities.push(AntlrEntity {
                identity,
                data_key,
                name,
                declaration_node_index: declaration,
                name_node_index: if self.parsed.db2.is_none() {
                    qualified_name
                        .as_ref()
                        .and_then(|q| q.components.last())
                        .map_or(captured.node_index, |part| part.node_index)
                } else {
                    captured.node_index
                },
                role: subject.map_or(AntlrDeclarationRole::Declaration, |s| s.role),
                qualified_name,
                oracle_declaration,
                namespace,
                contributions: vec![contribution],
                conflicts: Vec::new(),
                diagnostic_indices: Vec::new(),
            });
        }
        Ok(())
    }
    fn associate_diagnostics(&mut self) -> Result<(), AntlrModelError> {
        if self.diagnostics.is_empty() {
            return Ok(());
        }
        let mut targets: HashMap<(usize, &str, &str), Vec<usize>> = HashMap::new();
        for (index, entity) in self.entities.iter().enumerate() {
            self.parsed.original.check()?;
            let query = entity.contributions[0].occurrence.query;
            targets
                .entry((
                    if self.parsed.db2.is_some() {
                        entity.name_node_index
                    } else {
                        entity.declaration_node_index
                    },
                    query.graph_label().value().as_str(),
                    entity.data_key,
                ))
                .or_default()
                .push(index);
        }
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            self.parsed.original.check()?;
            let occurrence = &self.parsed.raw.occurrences[diagnostic.occurrence_index];
            if self.parsed.db2.is_some()
                && diagnostic.subject_node_index.is_none()
                && let Some(declaration) = diagnostic.declaration_node_index
            {
                for entity in &mut self.entities {
                    self.parsed.original.check()?;
                    if entity.declaration_node_index == declaration
                        && entity.identity.label() == occurrence.query.graph_label().value()
                        && entity.data_key == occurrence.query.data_key().value()
                    {
                        entity.diagnostic_indices.push(index);
                    }
                }
                continue;
            }

            if let Some(declaration) = diagnostic.declaration_node_index
                && let Some(entities) = targets.get(&(
                    if self.parsed.db2.is_some() {
                        diagnostic.subject_node_index.unwrap_or(declaration)
                    } else {
                        declaration
                    },
                    occurrence.query.graph_label().value().as_str(),
                    occurrence.query.data_key().value().as_str(),
                ))
            {
                for &entity in entities {
                    self.entities[entity].diagnostic_indices.push(index);
                }
            }
        }
        Ok(())
    }
}

impl<'doc, 'profile: 'o, 'o, 'v, 'request> Assembly<'doc, 'profile, 'o, 'v, 'request> {
    fn type_observation(&self, node: Option<usize>) -> AntlrTypeObservation<'doc, 'o> {
        match node.and_then(|n| self.parsed.origins.get(&n).map(|p| (n, p))) {
            Some((node_index, OriginalProjection::Exact(raw))) => AntlrTypeObservation::Present {
                node_index,
                raw: *raw,
            },
            Some((_, origin)) => AntlrTypeObservation::NonExact(origin),
            None => AntlrTypeObservation::Missing,
        }
    }
    fn db2_name(
        &mut self,
        occurrence: usize,
        declaration: usize,
        name: usize,
        prepared: Option<SourceRange<'v>>,
        origin: Option<&'doc OriginalProjection<'o>>,
    ) -> Result<Option<AntlrQualifiedName<'o>>, AntlrModelError> {
        self.parsed.original.check()?;
        let Some(OriginalProjection::Exact(raw)) = origin else {
            self.diagnostic(
                occurrence,
                Some(declaration),
                AntlrIssue::NonExact {
                    role: "name",
                    node_index: name,
                },
                origin,
            );
            return Ok(None);
        };
        let decoded = match db2::decode_name(raw.text()) {
            Ok(value) => value,
            Err(cause) => {
                self.diagnostic(
                    occurrence,
                    Some(declaration),
                    match cause {
                        db2::NameError::Lex(e) => AntlrIssue::NameCodec(e),
                        db2::NameError::Syntax(e) => AntlrIssue::NameSyntax(e),
                        db2::NameError::Shape => AntlrIssue::NameKindMismatch,
                    },
                    origin,
                );
                return Ok(None);
            }
        };
        let Some(prepared) = prepared else {
            self.diagnostic(
                occurrence,
                Some(declaration),
                AntlrIssue::NameKindMismatch,
                origin,
            );
            return Ok(None);
        };
        match db2::decode_name(prepared.text()) {
            Ok(value) if value == decoded => {}
            Ok(_) => {
                self.diagnostic(
                    occurrence,
                    Some(declaration),
                    AntlrIssue::NameKindMismatch,
                    origin,
                );
                return Ok(None);
            }
            Err(cause) => {
                self.diagnostic(
                    occurrence,
                    Some(declaration),
                    match cause {
                        db2::NameError::Lex(e) => AntlrIssue::NameCodec(e),
                        db2::NameError::Syntax(e) => AntlrIssue::NameSyntax(e),
                        db2::NameError::Shape => AntlrIssue::NameKindMismatch,
                    },
                    origin,
                );
                return Ok(None);
            }
        }
        let mut ids = Vec::new();
        for (index, node) in self.parsed.raw.syntax.nodes.iter().enumerate() {
            self.parsed.original.check()?;
            if self.parsed.descendant(index, name)? {
                if node.error
                    || node.missing
                    || node.class == NodeClass::Error
                    || node.status != SpanStatus::Regular
                {
                    self.diagnostic(
                        occurrence,
                        Some(declaration),
                        AntlrIssue::Recovery { node_index: index },
                        self.parsed.origins.get(&index),
                    );
                    return Ok(None);
                }
                if node.class == NodeClass::Rule && node.kind == "id_" {
                    ids.push(index);
                }
            }
        }
        if ids.len() != decoded.len() {
            self.diagnostic(
                occurrence,
                Some(declaration),
                AntlrIssue::NameKindMismatch,
                origin,
            );
            return Ok(None);
        }
        let mut components = Vec::new();
        for (node_index, ident) in ids.into_iter().zip(decoded) {
            self.parsed.original.check()?;
            let Some(OriginalProjection::Exact(component)) = self.parsed.origins.get(&node_index)
            else {
                self.diagnostic(
                    occurrence,
                    Some(declaration),
                    AntlrIssue::NonExact {
                        role: "name component",
                        node_index,
                    },
                    self.parsed.origins.get(&node_index),
                );
                return Ok(None);
            };
            match db2::decode_name(component.text()) {
                Ok(parts) if parts.len() == 1 && parts[0] == ident => {}
                Ok(_) => {
                    self.diagnostic(
                        occurrence,
                        Some(declaration),
                        AntlrIssue::NameKindMismatch,
                        origin,
                    );
                    return Ok(None);
                }
                Err(cause) => {
                    self.diagnostic(
                        occurrence,
                        Some(declaration),
                        match cause {
                            db2::NameError::Lex(e) => AntlrIssue::NameCodec(e),
                            db2::NameError::Syntax(e) => AntlrIssue::NameSyntax(e),
                            db2::NameError::Shape => AntlrIssue::NameKindMismatch,
                        },
                        origin,
                    );
                    return Ok(None);
                }
            }
            components.push(AntlrQualifiedNameComponent {
                node_index,
                raw: *component,
                decoded: Arc::from(ident.value),
                quote_style: ident.quote_style,
            });
        }
        Ok(Some(AntlrQualifiedName { components }))
    }
    fn db2_owners(
        &mut self,
        occurrence_index: usize,
        occurrence: &'doc SemanticOccurrence<'profile, 'o, 'v>,
        declaration: usize,
    ) -> Result<Option<Vec<AntlrNamespace<'doc, 'o, 'request>>>, AntlrModelError> {
        let registry = self.parsed.db2.as_ref().expect("DB2 owner path");
        let mut selected: Vec<(usize, usize, &str)> = Vec::new();
        for &index in occurrence.contexts.iter().rev() {
            self.parsed.original.check()?;
            let context = &self.parsed.raw.contexts[index];
            let context_node = context.matched.node_index;
            let owner = registry.canonical(context_node);
            if owner == declaration || registry.unnamed_schema.contains(&owner) {
                continue;
            }
            let Some(label) = context.label else { continue };
            let Some(name) = registry.unique(owner) else {
                self.diagnostic(
                    occurrence_index,
                    Some(declaration),
                    AntlrIssue::UnsupportedOwner {
                        node_index: context_node,
                    },
                    context.matched.original.as_ref(),
                );
                return Ok(None);
            };
            if !self.parsed.descendant(declaration, owner)? {
                return Err(AntlrModelError::NativeContract(
                    "context is not a declaration ancestor",
                ));
            }
            if context.name_is_explicit {
                let CaptureBinding::Present(captured) = &context.name else {
                    self.diagnostic(
                        occurrence_index,
                        Some(declaration),
                        AntlrIssue::MissingParent,
                        None,
                    );
                    return Ok(None);
                };
                if captured.node_index != name
                    || !matches!(captured.original, Some(OriginalProjection::Exact(_)))
                {
                    self.diagnostic(
                        occurrence_index,
                        Some(declaration),
                        AntlrIssue::InvalidExplicitParent,
                        captured.original.as_ref(),
                    );
                    return Ok(None);
                }
            }
            if let Some((_, _, previous)) = selected.iter().find(|(n, _, _)| *n == owner) {
                if *previous != label {
                    self.diagnostic(
                        occurrence_index,
                        Some(declaration),
                        AntlrIssue::OwnerConflict,
                        context.matched.original.as_ref(),
                    );
                    return Ok(None);
                }
            } else {
                selected.push((owner, context_node, label));
            }
        }
        match &occurrence.parent {
            CaptureBinding::NotConfigured => {}
            CaptureBinding::Missing => {
                self.diagnostic(
                    occurrence_index,
                    Some(declaration),
                    AntlrIssue::MissingParent,
                    None,
                );
                return Ok(None);
            }
            CaptureBinding::Present(parent) => {
                let candidate = registry
                    .subjects
                    .get(&parent.node_index)
                    .map(|s| s.declaration)
                    .or_else(|| {
                        registry
                            .unique(parent.node_index)
                            .map(|n| registry.subjects[&n].declaration)
                    });
                let label = occurrence.query.parent_label().value().as_str();
                let Some(owner) = candidate else {
                    self.diagnostic(
                        occurrence_index,
                        Some(declaration),
                        AntlrIssue::InvalidExplicitParent,
                        parent.original.as_ref(),
                    );
                    return Ok(None);
                };
                if owner == declaration
                    || label.is_empty()
                    || !self.parsed.descendant(declaration, owner)?
                    || !matches!(parent.original, Some(OriginalProjection::Exact(_)))
                {
                    self.diagnostic(
                        occurrence_index,
                        Some(declaration),
                        AntlrIssue::InvalidExplicitParent,
                        parent.original.as_ref(),
                    );
                    return Ok(None);
                }
                if let Some((_, _, previous)) = selected.iter().find(|(n, _, _)| *n == owner) {
                    if *previous != label {
                        self.diagnostic(
                            occurrence_index,
                            Some(declaration),
                            AntlrIssue::OwnerConflict,
                            parent.original.as_ref(),
                        );
                        return Ok(None);
                    }
                } else {
                    selected.push((owner, parent.node_index, label));
                }
            }
        }
        selected.sort_by_key(|(owner, _, _)| *owner);
        let mut namespace: Vec<AntlrNamespace<'doc, 'o, 'request>> = Vec::new();
        for (owner, context_node_index, label) in selected {
            self.parsed.original.check()?;
            let subject =
                &registry.subjects[&registry.unique(owner).expect("selected native owner")];
            let prepared = self
                .parsed
                .node(subject.name)
                .span
                .as_ref()
                .map(|s| self.parsed.prepared.range(s.start, s.end))
                .transpose()?;
            let Some(qname) = self.db2_name(
                occurrence_index,
                declaration,
                subject.name,
                prepared,
                self.parsed.origins.get(&subject.name),
            )?
            else {
                return Ok(None);
            };
            let Some(OriginalProjection::Exact(anchor)) = self.parsed.origins.get(&subject.name)
            else {
                return Err(AntlrModelError::NativeContract(
                    "owner name lost exact origin",
                ));
            };
            let name = Arc::clone(
                &qname
                    .components
                    .last()
                    .expect("nonempty owner QName")
                    .decoded,
            );
            let extent = self.parsed.extent(subject.extent)?;
            if subject.recovered {
                self.diagnostic(
                    occurrence_index,
                    Some(declaration),
                    AntlrIssue::Recovery { node_index: owner },
                    self.parsed.origins.get(&owner),
                );
            }
            if !matches!(extent, ExtentObservation::Exact(_)) {
                self.diagnostic(
                    occurrence_index,
                    Some(declaration),
                    AntlrIssue::NonExact {
                        role: "owner",
                        node_index: owner,
                    },
                    self.parsed.origins.get(&owner),
                );
            }
            let preceding: Vec<_> = namespace.iter().map(|frame| &frame.identity).collect();
            let identity = self.parsed.identity.symbol(label, *anchor, &preceding)?;
            namespace.push(AntlrNamespace {
                identity,
                name,
                node_index: owner,
                context_node_index,
                qualified_name: Some(qname),
                extent,
            });
        }
        Ok(Some(namespace))
    }
}
