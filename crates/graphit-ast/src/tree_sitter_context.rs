//! Native Tree-sitter context queries and one cursor traversal for lexical scopes.

use crate::{
    SourceRange,
    catalog::{EffectiveProfile, ProvenanceView},
    prepared_input::{OriginError, OriginalProjection, PreparedInput},
    tree_sitter_engine::{CompiledQuery, EngineError, GrammarLanguage, ParseControl},
    tree_sitter_profile::{
        PreparedTreeSitterProfile, TreeSitterProfileError, TreeSitterProfileMatches,
    },
};
use graphit_core::file_io::SnapshotContext;
use std::{collections::HashMap, error::Error, fmt, io};

#[derive(Debug)]
pub enum ContextError {
    Operation(io::Error),
    Profile(TreeSitterProfileError),
    Engine(EngineError),
    Origin(OriginError),
    UnknownContextKind {
        kind: String,
    },
    UnknownAnonymousKind {
        kind: String,
    },
    InvalidQuery {
        context_kind: String,
        message: String,
    },
    NativeContract(&'static str),
}
impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(e) => e.fmt(f),
            Self::Profile(e) => e.fmt(f),
            Self::Engine(e) => e.fmt(f),
            Self::Origin(e) => e.fmt(f),
            Self::UnknownContextKind { kind } => write!(f, "unknown native context kind {kind}"),
            Self::UnknownAnonymousKind { kind } => {
                write!(f, "unknown native anonymous-function kind {kind}")
            }
            Self::InvalidQuery {
                context_kind,
                message,
            } => write!(f, "context query for {context_kind}: {message}"),
            Self::NativeContract(message) => f.write_str(message),
        }
    }
}
impl Error for ContextError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(e) => Some(e),
            Self::Profile(e) => Some(e),
            Self::Engine(e) => Some(e),
            Self::Origin(e) => Some(e),
            _ => None,
        }
    }
}
impl From<io::Error> for ContextError {
    fn from(e: io::Error) -> Self {
        Self::Operation(e)
    }
}
impl From<TreeSitterProfileError> for ContextError {
    fn from(e: TreeSitterProfileError) -> Self {
        Self::Profile(e)
    }
}
impl From<EngineError> for ContextError {
    fn from(e: EngineError) -> Self {
        Self::Engine(e)
    }
}
impl From<OriginError> for ContextError {
    fn from(e: OriginError) -> Self {
        Self::Origin(e)
    }
}
const DEFAULT_CONTEXTS: [(&str, &str); 11] = [
    ("class_declaration", "Class"),
    ("class_definition", "Class"),
    ("interface_declaration", "Interface"),
    ("struct_declaration", "Struct"),
    ("trait_declaration", "Trait"),
    ("namespace_declaration", "Namespace"),
    ("enum_declaration", "Enum"),
    ("function_declaration", "Function"),
    ("function_definition", "Function"),
    ("method_declaration", "Method"),
    ("method_definition", "Method"),
];
const DEFAULT_ANONYMOUS: [&str; 3] = ["arrow_function", "function_expression", "function"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextPolicyOrigin {
    Default,
    Declared,
    Derived,
}
impl From<ProvenanceView<'_>> for ContextPolicyOrigin {
    fn from(p: ProvenanceView<'_>) -> Self {
        match p {
            ProvenanceView::Default => Self::Default,
            ProvenanceView::Declared(_) => Self::Declared,
            ProvenanceView::Derived(_) => Self::Derived,
        }
    }
}
#[derive(Clone, Debug)]
pub struct DefaultContextApplicability {
    pub kind: String,
    pub label: String,
    pub present: bool,
}
#[derive(Clone, Debug)]
pub struct DefaultAnonymousApplicability {
    pub kind: String,
    pub present: bool,
}
#[derive(Clone, Debug)]
pub struct ContextConfiguration {
    pub context_policy: ContextPolicyOrigin,
    pub anonymous_policy: ContextPolicyOrigin,
    pub applicable_context_types: Vec<(String, String)>,
    pub applicable_anon_types: Vec<String>,
    pub default_contexts: Vec<DefaultContextApplicability>,
    pub default_anonymous: Vec<DefaultAnonymousApplicability>,
}

pub struct PreparedTreeSitterContexts<'profile> {
    profile: PreparedTreeSitterProfile<'profile>,
    configuration: ContextConfiguration,
    queries: HashMap<String, CompiledQuery>,
}
impl<'profile> PreparedTreeSitterContexts<'profile> {
    pub fn prepared_profile(&self) -> &PreparedTreeSitterProfile<'profile> {
        &self.profile
    }
    pub fn prepare(
        profile: &'profile EffectiveProfile,
        language: GrammarLanguage,
        original: &SnapshotContext,
    ) -> Result<Self, ContextError> {
        original.check()?;
        let mut configuration = ContextConfiguration {
            context_policy: profile.context_types().provenance().into(),
            anonymous_policy: profile.anon_func_types().provenance().into(),
            applicable_context_types: Vec::new(),
            applicable_anon_types: Vec::new(),
            default_contexts: Vec::new(),
            default_anonymous: Vec::new(),
        };
        if configuration.context_policy == ContextPolicyOrigin::Default {
            for (kind, label) in DEFAULT_CONTEXTS {
                original.check()?;
                let present = language.context_kind_exists(kind, original)?;
                configuration
                    .default_contexts
                    .push(DefaultContextApplicability {
                        kind: kind.into(),
                        label: label.into(),
                        present,
                    });
                if present {
                    configuration
                        .applicable_context_types
                        .push((kind.into(), label.into()));
                }
            }
        } else {
            for (kind, label) in profile.context_types().value().iter() {
                original.check()?;
                if !language.context_kind_exists(kind.value(), original)? {
                    return Err(ContextError::UnknownContextKind {
                        kind: kind.value().clone(),
                    });
                }
                configuration
                    .applicable_context_types
                    .push((kind.value().clone(), label.value().clone()));
            }
        }
        if configuration.anonymous_policy == ContextPolicyOrigin::Default {
            for kind in DEFAULT_ANONYMOUS {
                original.check()?;
                let present = language.context_kind_exists(kind, original)?;
                configuration
                    .default_anonymous
                    .push(DefaultAnonymousApplicability {
                        kind: kind.into(),
                        present,
                    });
                if present {
                    configuration.applicable_anon_types.push(kind.into());
                }
            }
        } else {
            for kind in profile.anon_func_types().value().iter() {
                original.check()?;
                if !language.context_kind_exists(kind.value(), original)? {
                    return Err(ContextError::UnknownAnonymousKind {
                        kind: kind.value().clone(),
                    });
                }
                configuration
                    .applicable_anon_types
                    .push(kind.value().clone());
            }
        }

        let prepared = PreparedTreeSitterProfile::prepare(profile, language, original)?;
        let mut queries = HashMap::new();
        for (kind, pattern) in profile.context_name_queries().value().iter() {
            original.check()?;
            if !configuration
                .applicable_context_types
                .iter()
                .any(|(k, _)| k == kind.value())
            {
                return Err(ContextError::UnknownContextKind {
                    kind: kind.value().clone(),
                });
            }
            let query = prepared.compile_query(pattern.value(), original)?;
            if !query.required_single_captures(&["context", "name"]) {
                return Err(ContextError::InvalidQuery {
                    context_kind: kind.value().clone(),
                    message: "each native pattern must capture exactly one @context and one @name"
                        .into(),
                });
            }
            queries.insert(kind.value().clone(), query);
        }
        original.check()?;
        Ok(Self {
            profile: prepared,
            configuration,
            queries,
        })
    }
    pub fn collect<'o, 'v, 'request>(
        &self,
        input: &PreparedInput<'o, 'v, '_, 'request>,
    ) -> Result<ContextualTreeSitterDocument<'profile, 'o, 'v, 'request>, ContextError> {
        let original = input.original_context();
        original.check()?;
        let observations = self.profile.collect_matches(input)?;
        let mut node_indices = HashMap::new();
        let mut recovery = Vec::with_capacity(observations.nodes.len());
        for (index, node) in observations.nodes.iter().enumerate() {
            original.check()?;
            if node_indices.insert(node.node_id, index).is_some() {
                return Err(ContextError::NativeContract("duplicate SDK node identity"));
            }
            recovery.push(ContextRecovery {
                error: node.error,
                missing: node.missing,
                contains_error: node.error,
                contains_missing: node.missing,
            });
        }
        for index in (0..observations.nodes.len()).rev() {
            original.check()?;
            if let Some(parent) = observations.nodes[index].parent {
                if parent >= index {
                    return Err(ContextError::NativeContract(
                        "SDK node traversal is not parent-first",
                    ));
                }
                let child = recovery[index];
                recovery[parent].contains_error |= child.contains_error;
                recovery[parent].contains_missing |= child.contains_missing;
            }
        }
        // Match each prepared native query once. Pattern order supplies alternatives;
        // ties select the first source position, as Tree-sitter queries do.
        let mut names: HashMap<usize, (usize, usize)> = HashMap::new();
        for (kind, query) in &self.queries {
            let context_capture = query
                .capture_index_for_name("context")
                .ok_or(ContextError::NativeContract("prepared query lost @context"))?;
            let name_capture = query
                .capture_index_for_name("name")
                .ok_or(ContextError::NativeContract("prepared query lost @name"))?;
            observations.tree.visit_matches_with_context(
                query,
                ParseControl::default(),
                original,
                |matched| -> Result<(), ContextError> {
                    original.check()?;
                    let owner = matched
                        .captures()
                        .iter()
                        .find(|capture| capture.index == context_capture)
                        .ok_or(ContextError::NativeContract("native match lost @context"))?
                        .node;
                    let selected = matched
                        .captures()
                        .iter()
                        .find(|capture| capture.index == name_capture)
                        .ok_or(ContextError::NativeContract("native match lost @name"))?
                        .node;
                    let owner_index =
                        *node_indices
                            .get(&owner.id())
                            .ok_or(ContextError::NativeContract(
                                "context capture outside retained tree",
                            ))?;
                    let name_index =
                        *node_indices
                            .get(&selected.id())
                            .ok_or(ContextError::NativeContract(
                                "name capture outside retained tree",
                            ))?;
                    if owner.kind() != kind
                        || selected.start_byte() < owner.start_byte()
                        || selected.end_byte() > owner.end_byte()
                    {
                        return Err(ContextError::InvalidQuery {
                            context_kind: kind.clone(),
                            message: "@name must belong to the configured @context kind".into(),
                        });
                    }
                    let candidate = (matched.pattern_index, name_index);
                    if names
                        .get(&owner_index)
                        .is_none_or(|prior| candidate < *prior)
                    {
                        names.insert(owner_index, candidate);
                    }
                    Ok(())
                },
            )?;
        }
        let mut capture_scopes: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut scopes = Vec::new();
        for (query_index, group) in observations.groups.iter().enumerate() {
            for (match_index, matched) in group.matches.iter().enumerate() {
                for (capture_index, capture) in matched.captures.iter().enumerate() {
                    original.check()?;
                    let capture_node_index = *node_indices.get(&capture.native.node_id).ok_or(
                        ContextError::NativeContract("capture outside retained SDK tree"),
                    )?;
                    capture_scopes
                        .entry(capture_node_index)
                        .or_default()
                        .push(scopes.len());
                    scopes.push(CaptureContextScope {
                        query_index,
                        match_index,
                        capture_index,
                        capture_node_index,
                        contexts: Vec::new(),
                        excluded_self: Vec::new(),
                        unresolved: Vec::new(),
                    });
                }
            }
        }
        // A bit per property is enough to skip subtrees with no captures. The
        // existing SDK preorder/parent indices supply the bottom-up propagation;
        // no second children tree or per-capture ancestor search is needed.
        const CAPTURE_SUBTREE: u8 = 1;
        const CAPTURE_DESCENDANT: u8 = 2;
        let mut needed = vec![0_u8; observations.nodes.len()];
        for &index in capture_scopes.keys() {
            needed[index] |= CAPTURE_SUBTREE;
        }
        for index in (0..needed.len()).rev() {
            original.check()?;
            if needed[index] & CAPTURE_SUBTREE != 0
                && let Some(parent) = observations.nodes[index].parent
            {
                needed[parent] |= CAPTURE_SUBTREE | CAPTURE_DESCENDANT;
            }
        }
        let mut assembly = Assembly {
            input,
            observations: &observations,
            prepared: self,
            recovery,
            contexts: Vec::new(),
            diagnostics: Vec::new(),
        };
        let mut cursor = observations.tree.root_node().walk();
        let mut active = Vec::new();
        let mut stack_lengths = Vec::new();
        loop {
            original.check()?;
            let node = cursor.node();
            let index = node_indices[&node.id()];
            // A declaration's own name is excluded from its enclosing contexts.
            if let Some(slots) = capture_scopes.get(&index) {
                for &slot in slots {
                    let mut skip_owner = None;
                    for &context_index in active.iter().rev() {
                        let context: &NativeContextObservation<'_, '_> =
                            &assembly.contexts[context_index];
                        if skip_owner == Some(context.trigger.node_index) {
                            continue;
                        }
                        match &context.name {
                            ContextName::Missing { .. } => {
                                scopes[slot].unresolved.push(context_index)
                            }
                            ContextName::Found { selected, .. } if selected.node_index == index => {
                                scopes[slot].excluded_self.push(context_index)
                            }
                            ContextName::Found { .. } => scopes[slot].contexts.push(context_index),
                        }
                        if context.source == ContextSource::Anonymous
                            && matches!(context.name, ContextName::Found { .. })
                        {
                            skip_owner = context.owner.as_ref().map(|owner| owner.node_index);
                        }
                    }
                }
            }
            let saved = active.len();
            if needed[index] & CAPTURE_DESCENDANT != 0 {
                let configured = assembly.configured(node, index, &node_indices, &names)?;
                let configured_found = configured.is_some_and(|i| {
                    matches!(assembly.contexts[i].name, ContextName::Found { .. })
                });
                if !configured_found
                    && let Some(context_index) = assembly.anonymous(node, index, &node_indices)?
                {
                    active.push(context_index);
                }
                if let Some(context_index) = configured {
                    active.push(context_index);
                }
                if cursor.goto_first_child() {
                    stack_lengths.push(saved);
                    continue;
                }
            }
            active.truncate(saved);
            loop {
                original.check()?;
                if cursor.goto_next_sibling() {
                    break;
                }
                if !cursor.goto_parent() {
                    drop(cursor);
                    let contexts = assembly.contexts;
                    let diagnostics = assembly.diagnostics;
                    return Ok(ContextualTreeSitterDocument {
                        observations,
                        original,
                        configuration: self.configuration.clone(),
                        contexts,
                        scopes,
                        diagnostics,
                    });
                }
                active.truncate(stack_lengths.pop().ok_or(ContextError::NativeContract(
                    "unbalanced native context cursor",
                ))?);
            }
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextRecovery {
    pub error: bool,
    pub missing: bool,
    pub contains_error: bool,
    pub contains_missing: bool,
}
pub struct NativeNodeSource<'o, 'v> {
    pub node_index: usize,
    pub prepared: SourceRange<'v>,
    pub original: OriginalProjection<'o>,
    pub recovery: ContextRecovery,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ContextSource {
    Configured,
    Anonymous,
}
#[derive(Debug)]
pub enum ContextMissingReason {
    NoDefaultNameField,
    NoQueryMatch,
    AnonymousParentNotVariableDeclarator,
    AnonymousNameFieldMissing,
}
pub enum ContextName<'o, 'v> {
    Found {
        selected: NativeNodeSource<'o, 'v>,
        pattern_index: Option<usize>,
    },
    Missing {
        reason: ContextMissingReason,
    },
}
pub struct NativeContextObservation<'o, 'v> {
    pub trigger: NativeNodeSource<'o, 'v>,
    pub owner: Option<NativeNodeSource<'o, 'v>>,
    pub label: String,
    pub source: ContextSource,
    pub name: ContextName<'o, 'v>,
}
#[derive(Debug)]
pub struct CaptureContextScope {
    pub query_index: usize,
    pub match_index: usize,
    pub capture_index: usize,
    pub capture_node_index: usize,
    pub contexts: Vec<usize>,
    pub excluded_self: Vec<usize>,
    pub unresolved: Vec<usize>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextIssue {
    MissingName,
    RecoveredName,
    RecoveredOwner,
    NonExactName,
    NonExactOwner,
}
#[derive(Debug)]
pub struct ContextDiagnostic {
    pub context_index: usize,
    pub issue: ContextIssue,
}
pub struct ContextualTreeSitterDocument<'profile, 'o, 'v, 'request> {
    observations: TreeSitterProfileMatches<'profile, 'o, 'v>,
    original: &'request SnapshotContext,
    configuration: ContextConfiguration,
    contexts: Vec<NativeContextObservation<'o, 'v>>,
    scopes: Vec<CaptureContextScope>,
    diagnostics: Vec<ContextDiagnostic>,
}
impl<'profile, 'o, 'v> ContextualTreeSitterDocument<'profile, 'o, 'v, '_> {
    pub(crate) fn into_parts(
        self,
    ) -> (
        TreeSitterProfileMatches<'profile, 'o, 'v>,
        Vec<NativeContextObservation<'o, 'v>>,
    ) {
        (self.observations, self.contexts)
    }

    pub fn observations(
        &self,
    ) -> Result<&TreeSitterProfileMatches<'profile, 'o, 'v>, ContextError> {
        self.original.check()?;
        Ok(&self.observations)
    }
    pub fn contexts(&self) -> Result<&[NativeContextObservation<'o, 'v>], ContextError> {
        self.original.check()?;
        Ok(&self.contexts)
    }
    pub fn scopes(&self) -> Result<&[CaptureContextScope], ContextError> {
        self.original.check()?;
        Ok(&self.scopes)
    }
    pub fn diagnostics(&self) -> Result<&[ContextDiagnostic], ContextError> {
        self.original.check()?;
        Ok(&self.diagnostics)
    }
    pub fn configuration(&self) -> Result<&ContextConfiguration, ContextError> {
        self.original.check()?;
        Ok(&self.configuration)
    }
}

struct Assembly<'a, 'profile, 'o, 'v, 'p, 'request> {
    input: &'a PreparedInput<'o, 'v, 'p, 'request>,
    observations: &'a TreeSitterProfileMatches<'profile, 'o, 'v>,
    prepared: &'a PreparedTreeSitterContexts<'profile>,
    recovery: Vec<ContextRecovery>,
    contexts: Vec<NativeContextObservation<'o, 'v>>,
    diagnostics: Vec<ContextDiagnostic>,
}
impl<'o, 'v> Assembly<'_, '_, 'o, 'v, '_, '_> {
    fn node_source(&self, index: usize) -> Result<NativeNodeSource<'o, 'v>, ContextError> {
        self.input.original_context().check()?;
        let node = &self.observations.nodes[index];
        let start = u64::try_from(node.range.start)
            .map_err(|_| ContextError::NativeContract("SDK start cannot fit u64"))?;
        let end = u64::try_from(node.range.end)
            .map_err(|_| ContextError::NativeContract("SDK end cannot fit u64"))?;
        Ok(NativeNodeSource {
            node_index: index,
            prepared: self.input.prepared_span(start, end)?,
            original: self.input.project_prepared(start, end)?,
            recovery: self.recovery[index],
        })
    }
    fn configured(
        &mut self,
        node: tree_sitter::Node<'_>,
        index: usize,
        indices: &HashMap<usize, usize>,
        names: &HashMap<usize, (usize, usize)>,
    ) -> Result<Option<usize>, ContextError> {
        let Some((_, label)) = self
            .prepared
            .configuration
            .applicable_context_types
            .iter()
            .find(|(kind, _)| kind == node.kind())
        else {
            return Ok(None);
        };
        let name = if self.prepared.queries.contains_key(node.kind()) {
            match names.get(&index) {
                Some(&(pattern, name)) => ContextName::Found {
                    selected: self.node_source(name)?,
                    pattern_index: Some(pattern),
                },
                None => ContextName::Missing {
                    reason: ContextMissingReason::NoQueryMatch,
                },
            }
        } else {
            match node.child_by_field_name("name") {
                Some(name) => ContextName::Found {
                    selected: self.node_source(indices[&name.id()])?,
                    pattern_index: None,
                },
                None => ContextName::Missing {
                    reason: ContextMissingReason::NoDefaultNameField,
                },
            }
        };
        self.add(
            index,
            Some(index),
            label.clone(),
            ContextSource::Configured,
            name,
        )
        .map(Some)
    }
    fn anonymous(
        &mut self,
        node: tree_sitter::Node<'_>,
        index: usize,
        indices: &HashMap<usize, usize>,
    ) -> Result<Option<usize>, ContextError> {
        if !self
            .prepared
            .configuration
            .applicable_anon_types
            .iter()
            .any(|kind| kind == node.kind())
        {
            return Ok(None);
        }
        let owner = node
            .parent()
            .filter(|parent| parent.kind() == "variable_declarator");
        let name = match owner {
            Some(owner) => match owner.child_by_field_name("name") {
                Some(name) => ContextName::Found {
                    selected: self.node_source(indices[&name.id()])?,
                    pattern_index: None,
                },
                None => ContextName::Missing {
                    reason: ContextMissingReason::AnonymousNameFieldMissing,
                },
            },
            None => ContextName::Missing {
                reason: ContextMissingReason::AnonymousParentNotVariableDeclarator,
            },
        };
        self.add(
            index,
            owner.map(|n| indices[&n.id()]),
            "Function".into(),
            ContextSource::Anonymous,
            name,
        )
        .map(Some)
    }
    fn add(
        &mut self,
        index: usize,
        owner_index: Option<usize>,
        label: String,
        source: ContextSource,
        name: ContextName<'o, 'v>,
    ) -> Result<usize, ContextError> {
        let trigger = self.node_source(index)?;
        let owner = owner_index
            .map(|index| self.node_source(index))
            .transpose()?;
        let context_index = self.contexts.len();
        let mut issues = Vec::new();
        match &name {
            ContextName::Missing { .. } => issues.push(ContextIssue::MissingName),
            ContextName::Found { selected, .. } => {
                if selected.recovery.contains_error || selected.recovery.contains_missing {
                    issues.push(ContextIssue::RecoveredName)
                }
                if !matches!(selected.original, OriginalProjection::Exact(_)) {
                    issues.push(ContextIssue::NonExactName)
                }
            }
        }
        if let Some(owner) = &owner {
            if owner.recovery.contains_error || owner.recovery.contains_missing {
                issues.push(ContextIssue::RecoveredOwner)
            }
            if !matches!(owner.original, OriginalProjection::Exact(_)) {
                issues.push(ContextIssue::NonExactOwner)
            }
        }
        for issue in issues {
            self.input.original_context().check()?;
            self.diagnostics.push(ContextDiagnostic {
                context_index,
                issue,
            });
        }
        self.contexts.push(NativeContextObservation {
            trigger,
            owner,
            label,
            source,
            name,
        });
        Ok(context_index)
    }
}
