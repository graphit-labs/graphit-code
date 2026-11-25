use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenericDecisionRule {
    NodeKind,
    Operator,
    HeadName,
    HeadPairs,
    HeadSubjectPairs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenericComplexityDecision {
    pub node_index: usize,
    pub rule: GenericDecisionRule,
    pub weight: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenericComplexity {
    NoDeclaration,
    ContextUnavailable {
        issues: Vec<GenericEntityIssue>,
    },
    DeclarationUnavailable {
        evidence: Vec<GenericDeclaration>,
    },
    Recovery {
        node_index: usize,
    },
    Score {
        value: usize,
        declaration_node_index: usize,
        decisions: Vec<GenericComplexityDecision>,
    },
}

impl ParsedGenericTreeSitterDocument<'_, '_, '_, '_, '_, '_> {
    pub fn complexity(
        &self,
        declaration: Option<usize>,
    ) -> Result<GenericComplexity, GenericModelError> {
        self.original_context()?;
        let contexts = match declaration {
            Some(declaration) => match self.declaration_contexts(declaration)? {
                Ok(contexts) => contexts,
                Err(issues) => return Ok(GenericComplexity::ContextUnavailable { issues }),
            },
            None => Vec::new(),
        };
        self.complexity_with_contexts(declaration, &contexts)
    }

    pub(super) fn complexity_with_contexts(
        &self,
        declaration: Option<usize>,
        owned_contexts: &[usize],
    ) -> Result<GenericComplexity, GenericModelError> {
        let observations = self.observations()?;
        let Some(declaration) = declaration else {
            return Ok(GenericComplexity::NoDeclaration);
        };
        if declaration >= observations.nodes.len() {
            return Err(GenericModelError::NativeContract(
                "foreign metric declaration",
            ));
        }
        let profile = observations.profile;
        let policy = profile.complexity().value();
        let boundaries = self.declaration_boundaries()?;
        let mut stack = vec![declaration];
        let mut decisions = Vec::new();
        let mut value = 1usize;
        while let Some(index) = stack.pop() {
            self.original_context()?;
            let node = &observations.nodes[index];
            if index != declaration
                && (boundaries.contains(&index)
                    || (!owned_contexts.contains(&index)
                        && profile
                            .anon_func_types()
                            .value()
                            .iter()
                            .any(|kind| kind.value() == &node.kind)))
            {
                continue;
            }
            if node.error || node.missing {
                return Ok(GenericComplexity::Recovery { node_index: index });
            }
            let own_text = || {
                self.input
                    .prepared_span(
                        u64::try_from(node.range.start)
                            .map_err(|_| OriginError::ArithmeticOverflow)?,
                        u64::try_from(node.range.end)
                            .map_err(|_| OriginError::ArithmeticOverflow)?,
                    )
                    .map(|range| range.text())
            };
            let operator_match = if self.children[index].is_empty() {
                let text = own_text()?;
                policy
                    .operators()
                    .value()
                    .iter()
                    .any(|operator| operator.value().as_str() == text)
            } else {
                false
            };
            let decision = if policy
                .node_types()
                .value()
                .iter()
                .any(|kind| kind.value() == &node.kind)
            {
                Some((GenericDecisionRule::NodeKind, 1))
            } else if operator_match {
                Some((GenericDecisionRule::Operator, 1))
            } else if let Some(heads) = policy.head_calls().value()
                && heads.node_type().value() == &node.kind
            {
                let named: Vec<_> = self.children[index]
                    .iter()
                    .copied()
                    .filter(|&child| observations.nodes[child].named)
                    .collect();
                if let Some(&first) = named.first() {
                    let head = &observations.nodes[first];
                    let text = self
                        .input
                        .prepared_span(
                            u64::try_from(head.range.start)
                                .map_err(|_| OriginError::ArithmeticOverflow)?,
                            u64::try_from(head.range.end)
                                .map_err(|_| OriginError::ArithmeticOverflow)?,
                        )?
                        .text();
                    let arguments = named.len() - 1;
                    if heads
                        .names()
                        .value()
                        .iter()
                        .any(|name| name.value().as_str() == text)
                    {
                        Some((GenericDecisionRule::HeadName, 1))
                    } else if heads
                        .pair_names()
                        .value()
                        .iter()
                        .any(|name| name.value().as_str() == text)
                    {
                        Some((GenericDecisionRule::HeadPairs, arguments / 2))
                    } else if heads
                        .subject_pair_names()
                        .value()
                        .iter()
                        .any(|name| name.value().as_str() == text)
                    {
                        Some((
                            GenericDecisionRule::HeadSubjectPairs,
                            arguments.saturating_sub(1) / 2,
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            if let Some((rule, weight)) = decision {
                value = value
                    .checked_add(weight)
                    .ok_or(GenericModelError::NativeContract("complexity overflow"))?;
                decisions.push(GenericComplexityDecision {
                    node_index: index,
                    rule,
                    weight,
                });
            }
            stack.extend(self.children[index].iter().rev());
        }
        self.original_context()?;
        Ok(GenericComplexity::Score {
            value,
            declaration_node_index: declaration,
            decisions,
        })
    }
}
