//! Local Oracle binding over native declarations, not published-entity guesses.
use super::*;
use crate::prepared_input::OriginPiece;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AntlrOracleBindingState {
    Resolved,
    Ambiguous,
    Missing,
    Indeterminate,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AntlrOracleBindingIssue {
    MissingNativeField,
    UnsupportedReference,
    UnsupportedScope,
    UnsupportedType,
    DatabaseCharacterSet,
    NameCodec,
    NonExactOrigin,
    NativeRecovery,
    DeclarationVisibility,
    InvalidContext,
    UnsupportedInvocation,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AntlrOracleBoundKind {
    Value,
    Parameter,
    Type,
    Other,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AntlrOracleApplicationClass {
    Unresolved,
    CollectionIndex,
    CollectionMethod,
    CollectionConstructor,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AntlrOracleCollectionMethodKind {
    Count,
    First,
    Last,
    Limit,
    Next,
    Exists,
    Delete,
    Extend,
    Trim,
}
pub struct AntlrOracleMemberContinuity<'doc, 'o> {
    pub syntax: AntlrNativeSyntax<'doc, 'o>,
    pub verified: bool,
}
pub struct AntlrOracleBoundDeclaration<'doc, 'o> {
    pub declaration: AntlrNativeSyntax<'doc, 'o>,
    pub name: Option<AntlrNativeSyntax<'doc, 'o>>,
    pub scope: AntlrNativeSyntax<'doc, 'o>,
    pub scope_evidence: Vec<AntlrNativeSyntax<'doc, 'o>>,
    pub kind: AntlrOracleBoundKind,
    pub decoded_name: Option<Arc<str>>,
    pub quote_style: Option<char>,
    pub entity_indices: Vec<usize>,
}
pub struct AntlrOracleScopeContinuity<'doc, 'o> {
    pub scope_node_index: usize,
    pub reference_node_index: usize,
    pub prepared_start: usize,
    pub prepared_end: usize,
    pub original: &'doc OriginalProjection<'o>,
    pub verified: bool,
}
struct Continuity<'o> {
    prepared_start: usize,
    prepared_end: usize,
    original: OriginalProjection<'o>,
    verified: bool,
}
fn verified_continuity(projection: &OriginalProjection<'_>, text: &str) -> bool {
    match projection {
        OriginalProjection::Exact(raw) => raw.start() != raw.end() && raw.text() == text,
        OriginalProjection::Compound(pieces) => {
            let mut remaining = text;
            let mut previous_end = None;
            for piece in pieces {
                let OriginPiece::Original(raw) = piece else {
                    return false;
                };
                if raw.start() == raw.end() || previous_end.is_some_and(|end| end != raw.start()) {
                    return false;
                }
                let Some(rest) = remaining.strip_prefix(raw.text()) else {
                    return false;
                };
                remaining = rest;
                previous_end = Some(raw.end());
            }
            previous_end.is_some() && remaining.is_empty()
        }
        _ => false,
    }
}
pub struct AntlrOracleLookup<'doc, 'o> {
    pub state: AntlrOracleBindingState,
    pub reference: Option<AntlrNativeSyntax<'doc, 'o>>,
    pub decoded_name: Option<Arc<str>>,
    pub quote_style: Option<char>,
    pub candidates: Vec<AntlrOracleBoundDeclaration<'doc, 'o>>,
    pub scope_continuity: Vec<AntlrOracleScopeContinuity<'doc, 'o>>,
    pub issues: Vec<AntlrOracleBindingIssue>,
}
pub struct AntlrOracleApplicationBinding<'doc, 'o> {
    pub occurrence_index: usize,
    pub receiver: AntlrOracleLookup<'doc, 'o>,
    pub declared_type: Option<AntlrOracleLookup<'doc, 'o>>,
    pub classification: AntlrOracleApplicationClass,
    pub collection_type: Option<AntlrOracleTypeKind>,
    pub collection_method: Option<AntlrOracleCollectionMethodKind>,
    pub member_continuity: Option<AntlrOracleMemberContinuity<'doc, 'o>>,
    pub type_evidence: Vec<AntlrNativeSyntax<'doc, 'o>>,
    pub issues: Vec<AntlrOracleBindingIssue>,
}
#[derive(Clone)]
pub(super) struct Name {
    pub(super) decoded: Arc<str>,
    pub(super) quote: Option<char>,
}
#[derive(Clone, Copy)]
struct Reference {
    name: Option<usize>,
    issue: Option<AntlrOracleBindingIssue>,
}
struct Candidate {
    declaration: usize,
    name: Option<usize>,
    scope: usize,
    kind: AntlrOracleBoundKind,
    recovered: bool,
    context_issue: Option<AntlrOracleBindingIssue>,
}
pub(super) struct Registry<'o> {
    children: Vec<Vec<usize>>,
    candidates: Vec<Candidate>,
    recovered: Vec<bool>,
    references: HashMap<usize, Reference>,
    call_references: HashMap<usize, Reference>,
    evidence: HashMap<usize, Vec<usize>>,
    continuity: HashMap<(usize, usize), Continuity<'o>>,
}
fn scope(kind: &str) -> bool {
    matches!(
        kind,
        "anonymous_block"
            | "block"
            | "function_body"
            | "procedure_body"
            | "create_function_body"
            | "create_procedure_body"
            | "function_spec"
            | "procedure_spec"
    )
}
fn barrier(kind: &str) -> bool {
    matches!(
        kind,
        "select_statement"
            | "data_manipulation_language_statements"
            | "loop_statement"
            | "forall_statement"
            | "selection_directive"
            | "default_value_part"
            | "create_package"
            | "create_package_body"
            | "create_trigger"
            | "trigger_block"
            | "tps_block"
            | "create_type"
            | "create_type_body"
            | "cursor_declaration"
    )
}
fn native<'d, 'o>(
    p: &'d ParsedAntlrDocument<'_, 'o, '_, '_>,
    index: usize,
) -> AntlrNativeSyntax<'d, 'o> {
    AntlrNativeSyntax {
        node_index: index,
        node: &p.raw.syntax.nodes[index],
        original: p.origins.get(&index),
    }
}
pub(super) fn word(text: &str) -> Result<Name, AntlrOracleBindingIssue> {
    let tokens = Tokenizer::new(&GenericDialect, text)
        .with_unescape(true)
        .tokenize()
        .map_err(|_| AntlrOracleBindingIssue::NameCodec)?;
    let [Token::Word(word)] = tokens.as_slice() else {
        return Err(AntlrOracleBindingIssue::NameCodec);
    };
    if word.value.is_empty() || !matches!(word.quote_style, None | Some('"')) {
        return Err(AntlrOracleBindingIssue::NameCodec);
    }
    Ok(Name {
        decoded: Arc::from(word.value.as_str()),
        quote: word.quote_style,
    })
}
fn exact<'o>(
    p: &ParsedAntlrDocument<'_, 'o, '_, '_>,
    index: usize,
) -> Result<SourceRange<'o>, AntlrOracleBindingIssue> {
    let n = &p.raw.syntax.nodes[index];
    if n.error || n.missing || n.class == NodeClass::Error || n.status != SpanStatus::Regular {
        return Err(AntlrOracleBindingIssue::NativeRecovery);
    }
    match p.origins.get(&index) {
        Some(OriginalProjection::Exact(raw)) => {
            if raw.start() == raw.end() {
                return Err(AntlrOracleBindingIssue::MissingNativeField);
            }
            let span = n
                .span
                .as_ref()
                .ok_or(AntlrOracleBindingIssue::NativeRecovery)?;
            let prepared = p
                .prepared
                .range(span.start, span.end)
                .map_err(|_| AntlrOracleBindingIssue::NonExactOrigin)?;
            if raw.text() != prepared.text() {
                return Err(AntlrOracleBindingIssue::NonExactOrigin);
            }
            Ok(*raw)
        }
        _ => Err(AntlrOracleBindingIssue::NonExactOrigin),
    }
}
fn name(
    p: &ParsedAntlrDocument<'_, '_, '_, '_>,
    index: usize,
) -> Result<Name, AntlrOracleBindingIssue> {
    let raw = exact(p, index)?;
    let original = word(raw.text())?;
    let span = p.raw.syntax.nodes[index]
        .span
        .as_ref()
        .ok_or(AntlrOracleBindingIssue::NativeRecovery)?;
    let prepared = p
        .prepared
        .range(span.start, span.end)
        .map_err(|_| AntlrOracleBindingIssue::NonExactOrigin)?;
    let observed = word(prepared.text())?;
    if original.decoded != observed.decoded || original.quote != observed.quote {
        return Err(AntlrOracleBindingIssue::NonExactOrigin);
    }
    Ok(original)
}
fn equivalent(a: &Name, b: &Name) -> Option<bool> {
    if a.quote.is_some() && b.quote.is_some() {
        return Some(a.decoded == b.decoded);
    }
    if a.quote.is_none() && b.quote.is_none() && a.decoded == b.decoded {
        return Some(true);
    }
    let key = |n: &Name| {
        if n.quote.is_some() {
            Some(n.decoded.to_string())
        } else if n.decoded.is_ascii() {
            Some(n.decoded.to_ascii_uppercase())
        } else {
            None
        }
    };
    Some(key(a)? == key(b)?)
}
impl<'o> Registry<'o> {
    pub(super) fn collect(
        raw: &SemanticDocument<'_, 'o, '_>,
        declarations: &HashMap<usize, Declaration>,
        applications: &[oracle_applications::ApplicationNodes],
        input: &PreparedInput<'o, '_, '_, '_>,
        original: &SnapshotContext,
    ) -> Result<Self, AntlrModelError> {
        let nodes = &raw.syntax.nodes;
        let mut registry = Self {
            children: vec![Vec::new(); nodes.len()],
            candidates: Vec::new(),
            recovered: Vec::new(),
            references: HashMap::new(),
            call_references: HashMap::new(),
            evidence: HashMap::new(),
            continuity: HashMap::new(),
        };
        if raw.profile.grammar().value() != "antlr-plsql" {
            return Ok(registry);
        }
        let mut recovered: Vec<_> = nodes
            .iter()
            .map(|n| {
                n.error
                    || n.missing
                    || n.class == NodeClass::Error
                    || matches!(n.status, SpanStatus::Unindexed(_))
            })
            .collect();
        for (i, node) in nodes.iter().enumerate() {
            original.check()?;
            if let Some(parent) = node.parent {
                registry.children[parent].push(i);
            }
        }
        for i in (0..nodes.len()).rev() {
            original.check()?;
            if recovered[i]
                && let Some(parent) = nodes[i].parent
            {
                recovered[parent] = true;
            }
        }
        registry.recovered = recovered;
        for (i, n) in nodes.iter().enumerate() {
            original.check()?;
            if n.class != NodeClass::Rule {
                continue;
            }
            if scope(&n.kind) {
                let mut evidence = Vec::new();
                for &child in &registry.children[i] {
                    if nodes[child].class == NodeClass::Terminal
                        && matches!(
                            nodes[child].kind.as_str(),
                            "DECLARE"
                                | "FUNCTION"
                                | "PROCEDURE"
                                | "IS"
                                | "AS"
                                | "LEFT_PAREN"
                                | "RIGHT_PAREN"
                                | "BEGIN"
                                | "END"
                        )
                    {
                        evidence.push(child);
                    }
                    if nodes[child].kind == "body" {
                        evidence.extend(registry.children[child].iter().copied().filter(|&b| {
                            nodes[b].class == NodeClass::Terminal
                                && matches!(nodes[b].kind.as_str(), "BEGIN" | "END")
                        }));
                    }
                }
                if let Some(header) = declarations.get(&i).and_then(|d| d.name) {
                    evidence.push(header);
                }
                evidence.sort_unstable();
                registry.evidence.insert(i, evidence);
            }
            let kind = match n.kind.as_str() {
                "variable_declaration" => AntlrOracleBoundKind::Value,
                "parameter" => AntlrOracleBoundKind::Parameter,
                "type_declaration" => AntlrOracleBoundKind::Type,
                k if oracle_callable_label(k).is_some() => AntlrOracleBoundKind::Other,
                "cursor_declaration" | "exception_declaration" | "subtype_declaration" => {
                    AntlrOracleBoundKind::Other
                }
                _ => continue,
            };
            let mut parent = n.parent;
            let mut enclosing = None;
            let mut context_issue = None;
            while let Some(p) = parent {
                original.check()?;
                if nodes[p].kind == "selection_directive" {
                    context_issue = Some(AntlrOracleBindingIssue::UnsupportedScope);
                    parent = nodes[p].parent;
                    continue;
                }
                if scope(&nodes[p].kind) || barrier(&nodes[p].kind) {
                    enclosing = Some(p);
                    break;
                }
                parent = nodes[p].parent;
            }
            let Some(enclosing) = enclosing else {
                continue;
            };
            let canonical = declarations.get(&i).and_then(|d| d.name).or_else(|| {
                registry.children[i]
                    .iter()
                    .copied()
                    .find(|&c| nodes[c].kind == "identifier")
            });
            registry.candidates.push(Candidate {
                declaration: i,
                name: canonical,
                scope: enclosing,
                kind,
                recovered: registry.recovered[i],
                context_issue,
            });
        }
        let member_references: HashMap<_, _> = applications
            .iter()
            .filter_map(|application| application.member_reference())
            .collect();
        for (i, occurrence) in raw.occurrences.iter().enumerate() {
            original.check()?;
            if *occurrence.query.r#type().value() != QueryKind::Relation
                || occurrence.query.relation_type().value() != "CALLS"
            {
                continue;
            }
            let matched = occurrence.matched.node_index;
            let reference = if let Some(receiver) = member_references.get(&i) {
                (*receiver)
                    .map(|index| registry.member_receiver_reference(nodes, index))
                    .unwrap_or(Reference {
                        name: None,
                        issue: Some(AntlrOracleBindingIssue::UnsupportedReference),
                    })
            } else if nodes[matched].kind == "general_element_part" {
                registry.part_reference(nodes, matched, true, original)?
            } else if nodes[matched].class == NodeClass::Terminal
                && matches!(nodes[matched].kind.as_str(), "DELETE" | "EXTEND" | "TRIM")
                && nodes[matched]
                    .parent
                    .is_some_and(|p| nodes[p].kind == "collection_method_call")
            {
                let method = nodes[matched].parent.expect("checked collection parent");
                let receiver = registry.children[method]
                    .iter()
                    .copied()
                    .find(|&c| nodes[c].kind == "expression");
                receiver
                    .map(|r| registry.expression_reference(nodes, r, original))
                    .transpose()?
                    .unwrap_or(Reference {
                        name: None,
                        issue: Some(AntlrOracleBindingIssue::UnsupportedReference),
                    })
            } else {
                Reference {
                    name: None,
                    issue: Some(AntlrOracleBindingIssue::UnsupportedReference),
                }
            };
            registry.references.insert(i, reference);
            if nodes[matched].kind == "routine_name" {
                registry
                    .call_references
                    .insert(i, registry.routine_reference(nodes, matched));
            }
        }
        let mut references: Vec<_> = registry
            .references
            .values()
            .filter_map(|r| r.name)
            .collect();
        references.extend(registry.call_references.values().filter_map(|r| r.name));
        references.extend(
            registry
                .candidates
                .iter()
                .filter_map(|c| declarations.get(&c.declaration))
                .filter_map(|d| registry.type_reference(nodes, d.r#type).name),
        );
        references.sort_unstable();
        references.dedup();
        for reference in references {
            let mut ancestor = Some(reference);
            while let Some(scope) = ancestor {
                original.check()?;
                if let Some(first) = registry.evidence.get(&scope).and_then(|e| e.first())
                    && let (Some(start), Some(end)) =
                        (nodes[*first].span.as_ref(), nodes[reference].span.as_ref())
                    && start.start < end.end
                {
                    let projection = input.project_prepared(
                        u64::try_from(start.start).map_err(|_| {
                            AntlrModelError::NativeContract("scope prefix start overflow")
                        })?,
                        u64::try_from(end.end).map_err(|_| {
                            AntlrModelError::NativeContract("scope prefix end overflow")
                        })?,
                    )?;
                    let text = input.prepared_span(
                        u64::try_from(start.start).map_err(|_| {
                            AntlrModelError::NativeContract("scope prefix start overflow")
                        })?,
                        u64::try_from(end.end).map_err(|_| {
                            AntlrModelError::NativeContract("scope prefix end overflow")
                        })?,
                    )?;
                    let verified = verified_continuity(&projection, text.text());
                    registry.continuity.insert(
                        (scope, reference),
                        Continuity {
                            prepared_start: start.start,
                            prepared_end: end.end,
                            original: projection,
                            verified,
                        },
                    );
                }
                ancestor = nodes[scope].parent;
            }
        }
        Ok(registry)
    }
    // The CALLS consumer supplements routine_name without changing B1 reference semantics.
    fn routine_reference(&self, nodes: &[SyntaxNode], head: usize) -> Reference {
        let unsupported = Reference {
            name: None,
            issue: Some(AntlrOracleBindingIssue::UnsupportedReference),
        };
        let [identifier] = self.children[head].as_slice() else {
            return unsupported;
        };
        if nodes[*identifier].kind != "identifier" {
            return unsupported;
        }
        let [id] = self.children[*identifier].as_slice() else {
            return unsupported;
        };
        if nodes[*id].kind != "id_expression" {
            return unsupported;
        }
        let Some(statement) = nodes[head].parent else {
            return unsupported;
        };
        if nodes[statement].kind != "call_statement"
            || self.children[statement]
                .iter()
                .filter(|&&i| nodes[i].kind == "routine_name")
                .count()
                != 1
        {
            return unsupported;
        }
        Reference {
            name: Some(*id),
            issue: None,
        }
    }
    pub(super) fn call_lookup<'d>(
        &'d self,
        p: &'d ParsedAntlrDocument<'_, 'o, '_, '_>,
        entities: &[AntlrEntity<'_, '_, 'o, '_, '_>],
        occurrence_index: usize,
    ) -> Result<Option<AntlrOracleLookup<'d, 'o>>, AntlrModelError> {
        self.call_references
            .get(&occurrence_index)
            .copied()
            .map(|reference| self.lookup(p, entities, reference))
            .transpose()
    }
    pub(super) fn children(&self, index: usize) -> &[usize] {
        &self.children[index]
    }
    pub(super) fn equivalent_nodes(
        &self,
        p: &ParsedAntlrDocument<'_, 'o, '_, '_>,
        left: usize,
        right: usize,
    ) -> Result<bool, AntlrOracleBindingIssue> {
        equivalent(&name(p, left)?, &name(p, right)?)
            .ok_or(AntlrOracleBindingIssue::DatabaseCharacterSet)
    }
    /// A member carrier establishes this exact native prefix as the stop boundary.
    /// The ordinary reference path keeps rejecting enclosing member chains.
    fn member_receiver_reference(&self, nodes: &[SyntaxNode], prefix: usize) -> Reference {
        let unsupported = Reference {
            name: None,
            issue: Some(AntlrOracleBindingIssue::UnsupportedReference),
        };
        if nodes[prefix].class != NodeClass::Rule
            || nodes[prefix].kind != "general_element"
            || self.children[prefix].len() != 1
        {
            return unsupported;
        }
        let part = self.children[prefix][0];
        if nodes[part].class != NodeClass::Rule
            || nodes[part].kind != "general_element_part"
            || self.children[part].len() != 1
        {
            return unsupported;
        }
        let id = self.children[part][0];
        if nodes[id].class != NodeClass::Rule || nodes[id].kind != "id_expression" {
            return unsupported;
        }
        Reference {
            name: Some(id),
            issue: None,
        }
    }
    pub(super) fn usable(
        &self,
        p: &ParsedAntlrDocument<'_, '_, '_, '_>,
        index: usize,
    ) -> Result<(), AntlrOracleBindingIssue> {
        let node = &p.raw.syntax.nodes[index];
        if self.recovered.get(index).copied().unwrap_or(true) || node.status != SpanStatus::Regular
        {
            return Err(AntlrOracleBindingIssue::NativeRecovery);
        }
        let span = node
            .span
            .as_ref()
            .ok_or(AntlrOracleBindingIssue::MissingNativeField)?;
        if span.start == span.end {
            return Err(AntlrOracleBindingIssue::MissingNativeField);
        }
        let original = p
            .origins
            .get(&index)
            .ok_or(AntlrOracleBindingIssue::NonExactOrigin)?;
        let prepared = p
            .prepared
            .range(span.start, span.end)
            .map_err(|_| AntlrOracleBindingIssue::NonExactOrigin)?;
        if !verified_continuity(original, prepared.text()) {
            return Err(AntlrOracleBindingIssue::NonExactOrigin);
        }
        Ok(())
    }
    fn following_member(
        &self,
        p: &ParsedAntlrDocument<'_, '_, '_, '_>,
        mut index: usize,
    ) -> Result<bool, AntlrModelError> {
        let nodes = &p.raw.syntax.nodes;
        while let Some(parent) = nodes[index].parent {
            p.original.check()?;
            if nodes[parent].kind == "expression" {
                return Ok(false);
            }
            // A recursive extension starts with this complete prefix. A plain
            // parenthesized wrapper starts with LEFT_PAREN and is not an extension.
            if nodes[parent].kind == "general_element"
                && self.children[parent].first() == Some(&index)
                && self.children[parent].len() > 1
            {
                return Ok(true);
            }
            index = parent;
        }
        Ok(false)
    }
    fn accessor(
        &self,
        p: &ParsedAntlrDocument<'_, '_, '_, '_>,
        application: &AntlrOracleApplication<'_, '_>,
        member: &AntlrOracleMemberApplication<'_, '_>,
    ) -> Result<AntlrOracleCollectionMethodKind, AntlrOracleBindingIssue> {
        use AntlrOracleCollectionMethodKind as K;
        let receiver = member
            .receiver
            .as_ref()
            .ok_or(AntlrOracleBindingIssue::UnsupportedReference)?;
        let member_name = member
            .name
            .as_ref()
            .ok_or(AntlrOracleBindingIssue::MissingNativeField)?;
        let separator = member
            .separator
            .as_ref()
            .ok_or(AntlrOracleBindingIssue::MissingNativeField)?;
        for index in [
            member.syntax.node_index,
            receiver.node_index,
            separator.node_index,
        ] {
            self.usable(p, index)?;
        }
        let decoded = name(p, member_name.node_index)?;
        if decoded.quote.is_some() {
            return Err(AntlrOracleBindingIssue::UnsupportedReference);
        }
        let nodes = &p.raw.syntax.nodes;
        let mut token = member_name.node_index;
        if nodes[token].kind == "id_expression" {
            for expected in [
                "id_expression",
                "regular_id",
                "non_reserved_keywords_pre12c",
            ] {
                if nodes[token].class != NodeClass::Rule
                    || nodes[token].kind != expected
                    || self.children[token].len() != 1
                {
                    return Err(AntlrOracleBindingIssue::UnsupportedReference);
                }
                token = self.children[token][0];
            }
        }
        if nodes[token].class != NodeClass::Terminal {
            return Err(AntlrOracleBindingIssue::UnsupportedReference);
        }
        let kind = match nodes[token].kind.as_str() {
            "COUNT" => K::Count,
            "FIRST" => K::First,
            "LAST" => K::Last,
            "LIMIT" => K::Limit,
            "NEXT" => K::Next,
            "EXISTS" => K::Exists,
            _ => return Err(AntlrOracleBindingIssue::UnsupportedReference),
        };
        if !decoded.decoded.eq_ignore_ascii_case(&nodes[token].kind) {
            return Err(AntlrOracleBindingIssue::NameCodec);
        }
        if application.kind == AntlrOracleApplicationKind::GeneralElementPart
            && self.children[application.head.node_index]
                .iter()
                .any(|&index| {
                    !matches!(
                        nodes[index].kind.as_str(),
                        "id_expression" | "function_argument"
                    )
                })
        {
            return Err(AntlrOracleBindingIssue::UnsupportedReference);
        }
        if application.assignment.as_ref().is_some_and(|assignment| {
            assignment.side == AntlrOracleAssignmentSide::Target
                && assignment.expressions.is_empty()
        }) {
            return Err(AntlrOracleBindingIssue::InvalidContext);
        }
        if matches!(kind, K::Count | K::First | K::Last | K::Limit) {
            return if application.groups.is_empty() {
                Ok(kind)
            } else {
                Err(AntlrOracleBindingIssue::UnsupportedInvocation)
            };
        }
        let [group] = application.groups.as_slice() else {
            return Err(AntlrOracleBindingIssue::UnsupportedInvocation);
        };
        let [argument] = group.arguments.as_slice() else {
            return Err(AntlrOracleBindingIssue::UnsupportedInvocation);
        };
        if group.keep_clause.is_some() || argument.name.is_some() {
            return Err(AntlrOracleBindingIssue::UnsupportedInvocation);
        }
        let AntlrOracleExpression::Present { node_index, .. } = &argument.expression else {
            return Err(AntlrOracleBindingIssue::UnsupportedInvocation);
        };
        for index in [
            group.syntax.node_index,
            argument.syntax.node_index,
            *node_index,
        ] {
            self.usable(p, index)?;
        }
        Ok(kind)
    }
    fn part_reference(
        &self,
        nodes: &[SyntaxNode],
        part: usize,
        arguments: bool,
        original: &SnapshotContext,
    ) -> Result<Reference, AntlrModelError> {
        let children = &self.children[part];
        let id = children
            .iter()
            .copied()
            .find(|&c| nodes[c].kind == "id_expression");
        let valid = id.is_some()
            && children.iter().all(|&c| {
                nodes[c].kind == "id_expression"
                    || (arguments && nodes[c].kind == "function_argument")
            });
        let mut parent = nodes[part].parent;
        let mut simple = true;
        while let Some(p) = parent {
            original.check()?;
            if nodes[p].kind != "general_element" {
                break;
            }
            if self.children[p].len() != 1 {
                simple = false;
                break;
            }
            parent = nodes[p].parent;
        }
        Ok(Reference {
            name: id,
            issue: (!(valid && simple)).then_some(AntlrOracleBindingIssue::UnsupportedReference),
        })
    }
    fn expression_reference(
        &self,
        nodes: &[SyntaxNode],
        mut index: usize,
        original: &SnapshotContext,
    ) -> Result<Reference, AntlrModelError> {
        loop {
            original.check()?;
            if nodes[index].error || nodes[index].missing || nodes[index].class == NodeClass::Error
            {
                return Ok(Reference {
                    name: None,
                    issue: Some(AntlrOracleBindingIssue::NativeRecovery),
                });
            }
            if nodes[index].kind == "general_element_part" {
                return self.part_reference(nodes, index, false, original);
            }
            if self.children[index].len() != 1 || nodes[index].class != NodeClass::Rule {
                return Ok(Reference {
                    name: None,
                    issue: Some(AntlrOracleBindingIssue::UnsupportedReference),
                });
            }
            index = self.children[index][0];
        }
    }
    fn type_reference(&self, nodes: &[SyntaxNode], spec: Option<usize>) -> Reference {
        let unsupported = Reference {
            name: spec,
            issue: Some(AntlrOracleBindingIssue::UnsupportedType),
        };
        let Some(spec) = spec else {
            return unsupported;
        };
        if nodes[spec].kind != "type_spec" || self.children[spec].len() != 1 {
            return unsupported;
        }
        let type_name = self.children[spec][0];
        if nodes[type_name].kind != "type_name" || self.children[type_name].len() != 1 {
            return unsupported;
        }
        let id = self.children[type_name][0];
        if nodes[id].kind != "id_expression" {
            return unsupported;
        }
        Reference {
            name: Some(id),
            issue: None,
        }
    }
    pub(super) fn required_origins(
        &self,
        nodes: &[SyntaxNode],
        declarations: &HashMap<usize, Declaration>,
    ) -> Vec<usize> {
        let mut indices = Vec::new();
        for c in &self.candidates {
            indices.extend([c.declaration, c.scope]);
            indices.extend(c.name);
            if let Some(d) = declarations.get(&c.declaration) {
                indices.extend(d.r#type);
                indices.extend(self.type_reference(nodes, d.r#type).name);
            }
        }
        for reference in self
            .references
            .values()
            .chain(self.call_references.values())
        {
            indices.extend(reference.name);
        }
        for (&scope, markers) in &self.evidence {
            indices.push(scope);
            indices.extend(markers);
        }
        indices.sort_unstable();
        indices.dedup();
        indices
    }
    fn scope_issue(
        &self,
        p: &ParsedAntlrDocument<'_, '_, '_, '_>,
        index: usize,
    ) -> Option<AntlrOracleBindingIssue> {
        let n = &p.raw.syntax.nodes[index];
        if n.error || n.missing || n.class == NodeClass::Error || n.status != SpanStatus::Regular {
            return Some(AntlrOracleBindingIssue::NativeRecovery);
        }
        let Some(evidence) = self.evidence.get(&index) else {
            return Some(AntlrOracleBindingIssue::UnsupportedScope);
        };
        let mut begin = false;
        let mut end = false;
        let mut previous_end = None;
        for &marker in evidence {
            let raw = match exact(p, marker) {
                Ok(raw) => raw,
                Err(issue) => return Some(issue),
            };
            let n = &p.raw.syntax.nodes[marker];
            let Some(span) = n.span.as_ref() else {
                return Some(AntlrOracleBindingIssue::NativeRecovery);
            };
            let Ok(prepared) = p.prepared.range(span.start, span.end) else {
                return Some(AntlrOracleBindingIssue::NonExactOrigin);
            };
            if raw.text() != prepared.text() {
                return Some(AntlrOracleBindingIssue::NonExactOrigin);
            }
            if previous_end.is_some_and(|end| raw.start() < end) {
                return Some(AntlrOracleBindingIssue::NonExactOrigin);
            }
            previous_end = Some(raw.end());
            begin |= n.kind == "BEGIN";
            end |= n.kind == "END";
        }
        if !begin || !end {
            Some(AntlrOracleBindingIssue::UnsupportedScope)
        } else {
            None
        }
    }
    fn scope_contains(
        &self,
        p: &ParsedAntlrDocument<'_, '_, '_, '_>,
        scope: usize,
        anchor: usize,
    ) -> bool {
        let Some(evidence) = self.evidence.get(&scope) else {
            return false;
        };
        let Some((&first, &last)) = evidence.first().zip(evidence.last()) else {
            return false;
        };
        match (exact(p, first), exact(p, last), exact(p, anchor)) {
            (Ok(first), Ok(last), Ok(anchor)) => {
                first.start() <= anchor.start() && anchor.end() <= last.end()
            }
            _ => false,
        }
    }
    fn lookup<'d>(
        &'d self,
        p: &'d ParsedAntlrDocument<'_, 'o, '_, '_>,
        entities: &[AntlrEntity<'_, '_, 'o, '_, '_>],
        reference: Reference,
    ) -> Result<AntlrOracleLookup<'d, 'o>, AntlrModelError> {
        let mut result = AntlrOracleLookup {
            state: AntlrOracleBindingState::Indeterminate,
            reference: reference.name.map(|n| native(p, n)),
            decoded_name: None,
            quote_style: None,
            candidates: Vec::new(),
            scope_continuity: Vec::new(),
            issues: Vec::new(),
        };
        if let Some(issue) = reference.issue {
            result.issues.push(issue);
            return Ok(result);
        }
        let Some(index) = reference.name else {
            result
                .issues
                .push(AntlrOracleBindingIssue::UnsupportedReference);
            return Ok(result);
        };
        let key = match name(p, index) {
            Ok(key) => key,
            Err(issue) => {
                result.issues.push(issue);
                return Ok(result);
            }
        };
        result.decoded_name = Some(Arc::clone(&key.decoded));
        result.quote_style = key.quote;
        let mut context = Some(index);
        while let Some(a) = context {
            p.original.check()?;
            if matches!(
                p.raw.syntax.nodes[a].kind.as_str(),
                "selection_directive" | "default_value_part" | "select_statement"
            ) {
                result
                    .issues
                    .push(AntlrOracleBindingIssue::UnsupportedScope);
                return Ok(result);
            }
            context = p.raw.syntax.nodes[a].parent;
        }
        let mut ancestor = Some(index);
        while let Some(s) = ancestor {
            p.original.check()?;
            let n = &p.raw.syntax.nodes[s];
            if barrier(&n.kind) {
                result
                    .issues
                    .push(AntlrOracleBindingIssue::UnsupportedScope);
                return Ok(result);
            }
            if scope(&n.kind) {
                let Some(continuity) = self.continuity.get(&(s, index)) else {
                    result.issues.push(AntlrOracleBindingIssue::NonExactOrigin);
                    return Ok(result);
                };
                result.scope_continuity.push(AntlrOracleScopeContinuity {
                    scope_node_index: s,
                    reference_node_index: index,
                    prepared_start: continuity.prepared_start,
                    prepared_end: continuity.prepared_end,
                    original: &continuity.original,
                    verified: continuity.verified,
                });
                if !continuity.verified {
                    result.issues.push(AntlrOracleBindingIssue::NonExactOrigin);
                    return Ok(result);
                }
                if let Some(issue) = self.scope_issue(p, s) {
                    result.issues.push(issue);
                    return Ok(result);
                }
                if !self.scope_contains(p, s, index) {
                    result.issues.push(AntlrOracleBindingIssue::NonExactOrigin);
                    return Ok(result);
                }
                for c in self
                    .candidates
                    .iter()
                    .filter(|c| c.scope == s && c.declaration < index)
                {
                    p.original.check()?;
                    let candidate_name = c.name.map(|i| name(p, i)).transpose();
                    let (decoded, mut issue) = match candidate_name {
                        Ok(Some(decoded)) => (Some(decoded), None),
                        Ok(None) => (None, Some(AntlrOracleBindingIssue::NameCodec)),
                        Err(issue) => (None, Some(issue)),
                    };
                    if let Some(decoded) = &decoded {
                        match equivalent(&key, decoded) {
                            Some(false) => continue,
                            None => issue = Some(AntlrOracleBindingIssue::DatabaseCharacterSet),
                            Some(true) => {}
                        }
                    }
                    if let Some(context_issue) = c.context_issue {
                        issue = Some(context_issue);
                    }
                    if c.name.is_some_and(|name| !self.scope_contains(p, s, name)) {
                        issue = Some(AntlrOracleBindingIssue::NonExactOrigin);
                    }
                    if c.recovered {
                        issue = Some(AntlrOracleBindingIssue::NativeRecovery);
                    }
                    let mut path = Some(index);
                    while let Some(a) = path {
                        p.original.check()?;
                        if a == c.declaration {
                            issue = Some(AntlrOracleBindingIssue::DeclarationVisibility);
                            break;
                        }
                        path = p.raw.syntax.nodes[a].parent;
                    }
                    if let (Some(reference_raw), Some(name_raw)) = (
                        result.reference.and_then(|r| r.original).and_then(|o| {
                            if let OriginalProjection::Exact(r) = o {
                                Some(r)
                            } else {
                                None
                            }
                        }),
                        c.name.and_then(|i| p.origins.get(&i)).and_then(|o| {
                            if let OriginalProjection::Exact(r) = o {
                                Some(r)
                            } else {
                                None
                            }
                        }),
                    ) && name_raw.start() >= reference_raw.start()
                    {
                        issue = Some(AntlrOracleBindingIssue::DeclarationVisibility);
                    }
                    if let Some(issue) = issue {
                        result.issues.push(issue);
                    }
                    result.candidates.push(AntlrOracleBoundDeclaration {
                        declaration: native(p, c.declaration),
                        name: c.name.map(|i| native(p, i)),
                        scope: native(p, s),
                        scope_evidence: self
                            .evidence
                            .get(&s)
                            .into_iter()
                            .flatten()
                            .map(|&i| native(p, i))
                            .collect(),
                        kind: c.kind,
                        decoded_name: decoded.as_ref().map(|d| Arc::clone(&d.decoded)),
                        quote_style: decoded.and_then(|d| d.quote),
                        entity_indices: entities
                            .iter()
                            .enumerate()
                            .filter_map(|(i, e)| {
                                (e.declaration_node_index == c.declaration).then_some(i)
                            })
                            .collect(),
                    });
                }
                if !result.candidates.is_empty() {
                    result.state = if !result.issues.is_empty() {
                        AntlrOracleBindingState::Indeterminate
                    } else if result.candidates.len() == 1 {
                        AntlrOracleBindingState::Resolved
                    } else {
                        AntlrOracleBindingState::Ambiguous
                    };
                    return Ok(result);
                }
            }
            ancestor = n.parent;
        }
        result.state = AntlrOracleBindingState::Missing;
        Ok(result)
    }
    pub(super) fn bind<'d>(
        &'d self,
        p: &'d ParsedAntlrDocument<'_, 'o, '_, '_>,
        entities: &[AntlrEntity<'_, '_, 'o, '_, '_>],
        application: &AntlrOracleApplication<'d, 'o>,
    ) -> Result<AntlrOracleApplicationBinding<'d, 'o>, AntlrModelError> {
        p.original.check()?;
        let reference = self
            .references
            .get(&application.occurrence_index)
            .copied()
            .unwrap_or(Reference {
                name: None,
                issue: Some(AntlrOracleBindingIssue::UnsupportedReference),
            });
        let receiver = self.lookup(p, entities, reference)?;
        let mut issues = Vec::new();
        let member_continuity = application.member.as_ref().map(|member| {
            let proof = self.usable(p, member.syntax.node_index);
            if let Err(issue) = proof {
                issues.push(issue);
            }
            AntlrOracleMemberContinuity {
                syntax: member.syntax,
                verified: proof.is_ok(),
            }
        });
        let mut result = AntlrOracleApplicationBinding {
            occurrence_index: application.occurrence_index,
            receiver,
            declared_type: None,
            classification: AntlrOracleApplicationClass::Unresolved,
            collection_type: None,
            collection_method: None,
            member_continuity,
            type_evidence: Vec::new(),
            issues,
        };
        if result.receiver.state != AntlrOracleBindingState::Resolved {
            return Ok(result);
        }
        let candidate = &result.receiver.candidates[0];
        let mut type_declaration = None;
        if matches!(
            candidate.kind,
            AntlrOracleBoundKind::Value | AntlrOracleBoundKind::Parameter
        ) {
            let spec = p
                .declarations
                .get(&candidate.declaration.node_index)
                .and_then(|d| d.r#type);
            let type_reference = self.type_reference(&p.raw.syntax.nodes, spec);
            let mut lookup = self.lookup(p, entities, type_reference)?;
            if let Some(spec) = spec
                && let Err(issue) = exact(p, spec)
            {
                lookup.state = AntlrOracleBindingState::Indeterminate;
                lookup.issues.push(issue);
            }
            if lookup.state == AntlrOracleBindingState::Resolved
                && lookup.candidates[0].kind == AntlrOracleBoundKind::Type
            {
                type_declaration = Some(lookup.candidates[0].declaration.node_index);
            }
            result.declared_type = Some(lookup);
        } else if candidate.kind == AntlrOracleBoundKind::Type {
            type_declaration = Some(candidate.declaration.node_index);
        }
        let Some(type_declaration) = type_declaration else {
            return Ok(result);
        };
        let Some(detail) = p
            .declarations
            .get(&type_declaration)
            .and_then(|d| d.oracle_detail.as_ref())
        else {
            result
                .issues
                .push(AntlrOracleBindingIssue::MissingNativeField);
            return Ok(result);
        };
        for index in detail.indices() {
            p.original.check()?;
            result.type_evidence.push(native(p, index));
            if let Err(issue) = exact(p, index) {
                result.issues.push(issue);
            }
        }
        for issue in detail.field_issues(p)? {
            result.issues.push(match issue {
                oracle_declarations::DeclarationFieldIssue::Recovery(_) => {
                    AntlrOracleBindingIssue::NativeRecovery
                }
                oracle_declarations::DeclarationFieldIssue::NonExact(_) => {
                    AntlrOracleBindingIssue::NonExactOrigin
                }
                _ => AntlrOracleBindingIssue::MissingNativeField,
            });
        }
        if !result.issues.is_empty() {
            return Ok(result);
        }
        let oracle_declarations::DeclarationNodes::Type { kind, .. } = detail else {
            result
                .issues
                .push(AntlrOracleBindingIssue::MissingNativeField);
            return Ok(result);
        };
        if !matches!(
            kind,
            AntlrOracleTypeKind::NestedTable
                | AntlrOracleTypeKind::Varray
                | AntlrOracleTypeKind::AssociativeArray
        ) {
            return Ok(result);
        }
        result.collection_type = Some(*kind);
        if let Some(member) = &application.member {
            if !matches!(
                candidate.kind,
                AntlrOracleBoundKind::Value | AntlrOracleBoundKind::Parameter
            ) {
                result.issues.push(AntlrOracleBindingIssue::InvalidContext);
            } else if self.following_member(p, member.syntax.node_index)? {
                result
                    .issues
                    .push(AntlrOracleBindingIssue::UnsupportedReference);
            } else {
                match self.accessor(p, application, member) {
                    Ok(kind) => {
                        result.collection_method = Some(kind);
                        result.classification = AntlrOracleApplicationClass::CollectionMethod;
                    }
                    Err(issue) => result.issues.push(issue),
                }
            }
        } else if candidate.kind == AntlrOracleBoundKind::Type {
            if application.assignment.as_ref().is_some_and(|a| {
                a.side == AntlrOracleAssignmentSide::Target && a.expressions.is_empty()
            }) {
                result.issues.push(AntlrOracleBindingIssue::InvalidContext);
            } else if application.groups.len() == 1
                && matches!(
                    kind,
                    AntlrOracleTypeKind::NestedTable | AntlrOracleTypeKind::Varray
                )
            {
                result.classification = AntlrOracleApplicationClass::CollectionConstructor;
            }
        } else if application.collection_statement.is_some() {
            result.classification = AntlrOracleApplicationClass::CollectionMethod;
            result.collection_method = match application.head.node.kind.as_str() {
                "DELETE" => Some(AntlrOracleCollectionMethodKind::Delete),
                "EXTEND" => Some(AntlrOracleCollectionMethodKind::Extend),
                "TRIM" => Some(AntlrOracleCollectionMethodKind::Trim),
                _ => None,
            };
        } else if application.kind == AntlrOracleApplicationKind::GeneralElementPart
            && application.groups.len() == 1
        {
            result.classification = AntlrOracleApplicationClass::CollectionIndex;
        }
        p.original.check()?;
        Ok(result)
    }
}
