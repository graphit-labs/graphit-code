//! Local callable evidence over retained Oracle syntax and the native lexical registry.
use super::*;
use AntlrOracleBindingIssue as B;
use AntlrOracleBindingState as S;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AntlrOracleCallReason {
    MissingCaller,
    OutsideBody,
    MissingNativeField,
    MissingEntity,
    NativeRecovery,
    NonExactOrigin,
    UnsupportedScope,
    UnsupportedReference,
    NameCodec,
    DatabaseCharacterSet,
    DeclarationVisibility,
    UnsupportedInvocation,
    UnsupportedTarget,
    MissingTargetRule,
    UnsupportedPolicy,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AntlrOracleCallIssue {
    pub reason: AntlrOracleCallReason,
    pub node_index: Option<usize>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AntlrOracleCallSourceState {
    Bound { entity_index: usize },
    Ambiguous { entity_indices: Vec<usize> },
    Unavailable { reason: AntlrOracleCallReason },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AntlrOracleCallTargetState {
    Resolved { entity_index: usize },
    NotFound,
    Ambiguous { entity_indices: Vec<usize> },
    Unresolved { reason: AntlrOracleCallReason },
}
pub struct AntlrOracleCallGroupOutcome<'doc, 'o> {
    pub group_index: usize,
    pub syntax: AntlrNativeSyntax<'doc, 'o>,
    pub target: AntlrOracleCallTargetState,
    pub issues: Vec<AntlrOracleCallIssue>,
}
pub struct AntlrOracleCallOutcome<'doc, 'o> {
    /// Ordinal in this document's oracle_calls, distinct from entity_index.
    pub call_index: usize,
    pub occurrence_index: usize,
    pub source: AntlrOracleCallSourceState,
    pub target: AntlrOracleCallTargetState,
    pub caller: Option<AntlrNativeSyntax<'doc, 'o>>,
    pub body: Option<AntlrNativeSyntax<'doc, 'o>>,
    pub evidence: Vec<AntlrNativeSyntax<'doc, 'o>>,
    pub binding_issues: Vec<AntlrOracleBindingIssue>,
    pub issues: Vec<AntlrOracleCallIssue>,
    pub group_outcomes: Vec<AntlrOracleCallGroupOutcome<'doc, 'o>>,
    /// Supplemental routine_name lookup. The original B1 binding is retained separately.
    pub call_lookup: Option<AntlrOracleLookup<'doc, 'o>>,
}
fn reason(issue: B) -> AntlrOracleCallReason {
    use AntlrOracleCallReason as R;
    match issue {
        B::MissingNativeField => R::MissingNativeField,
        B::UnsupportedReference => R::UnsupportedReference,
        B::UnsupportedScope | B::InvalidContext => R::UnsupportedScope,
        B::UnsupportedType => R::UnsupportedTarget,
        B::DatabaseCharacterSet => R::DatabaseCharacterSet,
        B::NameCodec => R::NameCodec,
        B::NonExactOrigin => R::NonExactOrigin,
        B::NativeRecovery => R::NativeRecovery,
        B::DeclarationVisibility => R::DeclarationVisibility,
        B::UnsupportedInvocation => R::UnsupportedInvocation,
    }
}
fn callable(kind: &str) -> bool {
    matches!(oracle_callable_label(kind), Some("Function" | "Procedure"))
}
fn native<'d, 'o>(
    p: &'d ParsedAntlrDocument<'_, 'o, '_, '_>,
    index: usize,
) -> AntlrNativeSyntax<'d, 'o> {
    AntlrNativeSyntax {
        node_index: index,
        node: p.node(index),
        original: p.origins.get(&index),
    }
}
fn unavailable(
    out: &mut AntlrOracleCallOutcome<'_, '_>,
    r: AntlrOracleCallReason,
    index: Option<usize>,
) {
    out.source = AntlrOracleCallSourceState::Unavailable { reason: r };
    out.issues.push(AntlrOracleCallIssue {
        reason: r,
        node_index: index,
    });
}
fn unresolved(
    out: &mut AntlrOracleCallOutcome<'_, '_>,
    r: AntlrOracleCallReason,
    index: Option<usize>,
) {
    out.target = AntlrOracleCallTargetState::Unresolved { reason: r };
    out.issues.push(AntlrOracleCallIssue {
        reason: r,
        node_index: index,
    });
}
// Entity diagnostics stay authoritative. Only the producer's own structurally
// nonexact extent/owner can be accepted after verifying its exact native origin.
fn entity_usable(
    p: &ParsedAntlrDocument<'_, '_, '_, '_>,
    entity: &AntlrEntity<'_, '_, '_, '_, '_>,
    diagnostics: &[AntlrDiagnostic<'_, '_>],
) -> Result<bool, AntlrModelError> {
    if !entity.conflicts().is_empty() {
        return Ok(false);
    }
    for &index in entity.diagnostic_indices() {
        p.original.check()?;
        let diagnostic = diagnostics
            .get(index)
            .ok_or(AntlrModelError::NativeContract(
                "Oracle CALLS foreign diagnostic index",
            ))?;
        let AntlrIssue::NonExact {
            role: "extent" | "owner",
            node_index,
        } = &diagnostic.issue
        else {
            return Ok(false);
        };
        let Some(origin @ OriginalProjection::Compound(_)) = diagnostic.original else {
            return Ok(false);
        };
        if !p
            .origins
            .get(node_index)
            .is_some_and(|native| std::ptr::eq(native, origin))
            || p.bindings.usable(p, *node_index).is_err()
        {
            return Ok(false);
        }
    }
    for contribution in entity.contributions() {
        p.original.check()?;
        if let ExtentObservation::NonExact(origin) = &contribution.extent
            && !p
                .origins
                .get(&entity.declaration_node_index())
                .is_some_and(|native| std::ptr::eq(native, *origin))
        {
            return Ok(false);
        }
    }
    Ok(true)
}
fn source<'d, 'o>(
    p: &'d ParsedAntlrDocument<'_, 'o, '_, '_>,
    entities: &[AntlrEntity<'_, '_, 'o, '_, '_>],
    application: &AntlrOracleApplication<'d, 'o>,
    diagnostics: &[AntlrDiagnostic<'_, 'o>],
    out: &mut AntlrOracleCallOutcome<'d, 'o>,
) -> Result<(), AntlrModelError> {
    use AntlrOracleCallReason as R;
    for boundary in &application.boundaries {
        p.original.check()?;
        if matches!(
            boundary.node.kind.as_str(),
            "parameter" | "default_value_part" | "variable_declaration"
        ) {
            unavailable(out, R::OutsideBody, Some(boundary.node_index));
            return Ok(());
        }
        if !callable(&boundary.node.kind) {
            continue;
        }
        out.caller = Some(*boundary);
        let bodies: Vec<_> = p
            .bindings
            .children(boundary.node_index)
            .iter()
            .copied()
            .filter(|&i| p.node(i).kind == "body")
            .collect();
        let [body] = bodies.as_slice() else {
            unavailable(out, R::OutsideBody, Some(boundary.node_index));
            return Ok(());
        };
        out.body = Some(native(p, *body));
        if !p.descendant(application.head.node_index, *body)? {
            unavailable(out, R::OutsideBody, Some(boundary.node_index));
            return Ok(());
        }
        for i in [application.head.node_index, *body, boundary.node_index] {
            p.original.check()?;
            if let Err(issue) = p.bindings.usable(p, i) {
                unavailable(out, reason(issue), Some(i));
                return Ok(());
            }
        }
        let indices: Vec<_> = entities
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.declaration_node_index() == boundary.node_index
                    && matches!(e.identity().label(), "Function" | "Procedure")
            })
            .map(|(i, _)| i)
            .collect();
        out.source = match indices.as_slice() {
            [] => AntlrOracleCallSourceState::Unavailable {
                reason: R::MissingEntity,
            },
            [i] if entity_usable(p, &entities[*i], diagnostics)? => {
                AntlrOracleCallSourceState::Bound { entity_index: *i }
            }
            [_] => AntlrOracleCallSourceState::Unavailable {
                reason: R::UnsupportedPolicy,
            },
            _ => AntlrOracleCallSourceState::Ambiguous {
                entity_indices: indices,
            },
        };
        return Ok(());
    }
    unavailable(out, R::MissingCaller, None);
    Ok(())
}
fn self_reference(
    p: &ParsedAntlrDocument<'_, '_, '_, '_>,
    lookup: &AntlrOracleLookup<'_, '_>,
    caller: usize,
) -> Result<bool, B> {
    let own_only = lookup.state == S::Indeterminate
        && !lookup.issues.is_empty()
        && lookup.issues.iter().all(|i| *i == B::DeclarationVisibility)
        && lookup.candidates.len() == 1
        && lookup.candidates[0].declaration.node_index == caller;
    if !(lookup.state == S::Missing && lookup.issues.is_empty() || own_only) {
        return Ok(false);
    }
    if !lookup.scope_continuity.iter().all(|c| c.verified)
        || !lookup
            .scope_continuity
            .iter()
            .any(|c| c.scope_node_index == caller)
    {
        return Err(B::NonExactOrigin);
    }
    let reference = lookup.reference.ok_or(B::MissingNativeField)?;
    let own_name = p
        .declarations
        .get(&caller)
        .and_then(|d| d.name)
        .ok_or(B::MissingNativeField)?;
    p.bindings
        .equivalent_nodes(p, reference.node_index, own_name)
}
fn target<'d, 'o>(
    p: &'d ParsedAntlrDocument<'_, 'o, '_, '_>,
    entities: &[AntlrEntity<'_, '_, 'o, '_, '_>],
    application: &AntlrOracleApplication<'d, 'o>,
    lookup: &AntlrOracleLookup<'d, 'o>,
    diagnostics: &[AntlrDiagnostic<'_, 'o>],
    out: &mut AntlrOracleCallOutcome<'d, 'o>,
) -> Result<(), AntlrModelError> {
    use AntlrOracleCallReason as R;
    if application.member.is_some()
        || application.assignment.as_ref().is_some_and(|a| {
            a.side == AntlrOracleAssignmentSide::Target && a.expressions.is_empty()
        })
    {
        unresolved(
            out,
            R::UnsupportedInvocation,
            Some(application.head.node_index),
        );
        return Ok(());
    }
    // B1 DeclarationVisibility remains intact. Self-recursion is a separate proof.
    if let Some(caller) = out.caller
        && let AntlrOracleCallSourceState::Bound { entity_index } = out.source
    {
        match self_reference(p, lookup, caller.node_index) {
            Ok(true) => {
                out.target = AntlrOracleCallTargetState::Resolved { entity_index };
                return Ok(());
            }
            Err(issue) => {
                unresolved(out, reason(issue), None);
                return Ok(());
            }
            Ok(false) => {}
        }
    }
    match lookup.state {
        S::Missing => out.target = AntlrOracleCallTargetState::NotFound,
        S::Indeterminate => {
            let r = lookup
                .issues
                .first()
                .copied()
                .map(reason)
                .unwrap_or(R::UnsupportedReference);
            // B1 issue enums do not provide a causal node: never invent one.
            unresolved(out, r, None);
        }
        S::Resolved | S::Ambiguous => {
            let mut indices = Vec::new();
            for candidate in &lookup.candidates {
                p.original.check()?;
                out.evidence.push(candidate.declaration);
                out.evidence.extend(candidate.name);
                out.evidence
                    .extend(candidate.scope_evidence.iter().copied());
                if !callable(&candidate.declaration.node.kind) {
                    unresolved(
                        out,
                        R::UnsupportedTarget,
                        Some(candidate.declaration.node_index),
                    );
                    return Ok(());
                }
                if let Err(issue) = p.bindings.usable(p, candidate.declaration.node_index) {
                    unresolved(out, reason(issue), Some(candidate.declaration.node_index));
                    return Ok(());
                }
                if candidate.entity_indices.is_empty() {
                    unresolved(
                        out,
                        R::MissingEntity,
                        Some(candidate.declaration.node_index),
                    );
                    return Ok(());
                }
                for &i in &candidate.entity_indices {
                    let e = entities.get(i).ok_or(AntlrModelError::NativeContract(
                        "Oracle CALLS foreign entity index",
                    ))?;
                    if e.declaration_node_index() != candidate.declaration.node_index {
                        return Err(AntlrModelError::NativeContract(
                            "Oracle CALLS candidate entity mismatch",
                        ));
                    }
                    if e.identity().label()
                        != oracle_callable_label(&candidate.declaration.node.kind).unwrap_or("")
                        || !entity_usable(p, e, diagnostics)?
                    {
                        unresolved(
                            out,
                            R::UnsupportedPolicy,
                            Some(candidate.declaration.node_index),
                        );
                        return Ok(());
                    }
                    indices.push(i);
                }
            }
            indices.sort_unstable();
            indices.dedup();
            out.target = match indices.as_slice() {
                [] => AntlrOracleCallTargetState::Unresolved {
                    reason: R::MissingEntity,
                },
                [i] if lookup.state == S::Resolved && lookup.candidates.len() == 1 => {
                    AntlrOracleCallTargetState::Resolved { entity_index: *i }
                }
                _ => AntlrOracleCallTargetState::Ambiguous {
                    entity_indices: indices,
                },
            };
        }
    }
    Ok(())
}
pub(super) fn resolve<'d, 'o>(
    p: &'d ParsedAntlrDocument<'_, 'o, '_, '_>,
    entities: &[AntlrEntity<'_, '_, 'o, '_, '_>],
    applications: &[AntlrOracleApplication<'d, 'o>],
    bindings: &[AntlrOracleApplicationBinding<'d, 'o>],
    diagnostics: &[AntlrDiagnostic<'_, 'o>],
) -> Result<Vec<AntlrOracleCallOutcome<'d, 'o>>, AntlrModelError> {
    p.original.check()?;
    let result = (|| {
        let mut calls = Vec::new();
        for application in applications {
            p.original.check()?;
            let binding = bindings
                .iter()
                .find(|b| b.occurrence_index == application.occurrence_index)
                .ok_or(AntlrModelError::NativeContract(
                    "Oracle CALLS application lacks binding",
                ))?;
            if binding.classification != AntlrOracleApplicationClass::Unresolved
                || matches!(
                    application.kind,
                    AntlrOracleApplicationKind::KeywordMethod
                        | AntlrOracleApplicationKind::CollectionMethodStatement
                )
            {
                continue;
            }
            let supplemental = p
                .bindings
                .call_lookup(p, entities, application.occurrence_index)?;
            let lookup = supplemental.as_ref().unwrap_or(&binding.receiver);
            // Only a proven noncallable stays with its existing producer. An
            // indeterminate name match must retain its unresolved cause.
            if matches!(lookup.state, S::Resolved | S::Ambiguous)
                && !lookup.candidates.is_empty()
                && lookup
                    .candidates
                    .iter()
                    .all(|c| !callable(&c.declaration.node.kind))
            {
                continue;
            }
            let mut out = AntlrOracleCallOutcome {
                call_index: calls.len(),
                occurrence_index: application.occurrence_index,
                source: AntlrOracleCallSourceState::Unavailable {
                    reason: AntlrOracleCallReason::MissingCaller,
                },
                target: AntlrOracleCallTargetState::NotFound,
                caller: None,
                body: None,
                evidence: std::iter::once(application.head)
                    .chain(application.boundaries.iter().copied())
                    .collect(),
                binding_issues: lookup.issues.clone(),
                issues: Vec::new(),
                group_outcomes: Vec::new(),
                call_lookup: None,
            };
            source(p, entities, application, diagnostics, &mut out)?;
            target(p, entities, application, lookup, diagnostics, &mut out)?;
            let occurrence = &p.raw.occurrences[application.occurrence_index];
            let rule = p
                .raw
                .profile
                .target_rules()
                .value()
                .iter()
                .find(|(key, _)| key.value() == occurrence.query.relation_type().value())
                .map(|(_, r)| r);
            match rule {
                None => unresolved(&mut out, AntlrOracleCallReason::MissingTargetRule, None),
                Some(rule) => {
                    let allowed = |index: usize| {
                        rule.value()
                            .labels()
                            .value()
                            .iter()
                            .any(|label| label.value() == entities[index].identity().label())
                    };
                    let incompatible = match &out.target {
                        AntlrOracleCallTargetState::Resolved { entity_index } => {
                            !allowed(*entity_index)
                        }
                        AntlrOracleCallTargetState::Ambiguous { entity_indices } => {
                            entity_indices.iter().any(|&i| !allowed(i))
                        }
                        _ => false,
                    };
                    if incompatible {
                        unresolved(&mut out, AntlrOracleCallReason::UnsupportedPolicy, None);
                    }
                }
            }
            if matches!(out.target, AntlrOracleCallTargetState::Resolved { .. }) {
                let capture = match &occurrence.name {
                    CaptureBinding::Present(capture) => Some(capture),
                    _ => None,
                };
                let anchor_valid =
                    capture
                        .zip(lookup.reference)
                        .is_some_and(|(capture, reference)| {
                            matches!(capture.original, Some(OriginalProjection::Exact(_)))
                                && p.bindings.equivalent_nodes(
                                    p,
                                    capture.node_index,
                                    reference.node_index,
                                ) == Ok(true)
                        });
                if !anchor_valid {
                    unresolved(&mut out, AntlrOracleCallReason::UnsupportedPolicy, None);
                }
            }
            for (group_index, group) in application.groups.iter().enumerate() {
                p.original.check()?;
                let (target, issues) = if group_index == 0 {
                    (out.target.clone(), Vec::new())
                } else {
                    (
                        AntlrOracleCallTargetState::Unresolved {
                            reason: AntlrOracleCallReason::UnsupportedInvocation,
                        },
                        vec![AntlrOracleCallIssue {
                            reason: AntlrOracleCallReason::UnsupportedInvocation,
                            node_index: Some(group.syntax.node_index),
                        }],
                    )
                };
                out.group_outcomes.push(AntlrOracleCallGroupOutcome {
                    group_index,
                    syntax: group.syntax,
                    target,
                    issues,
                });
            }
            out.evidence.sort_by_key(|e| e.node_index);
            out.evidence.dedup_by_key(|e| e.node_index);
            out.call_lookup = supplemental;
            calls.push(out);
        }
        Ok(calls)
    })();
    p.original.check()?;
    result
}
