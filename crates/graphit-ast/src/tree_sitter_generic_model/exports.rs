use super::*;
use crate::profiles::ExportStrategy;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenericExport {
    Decision {
        exported: bool,
        evidence_node_indices: Vec<usize>,
    },
    NoDeclaration,
    DeclarationUnavailable {
        evidence: Vec<GenericDeclaration>,
    },
    Recovery {
        node_index: usize,
    },
    Ambiguous {
        entity_indices: Vec<usize>,
    },
}

impl ParsedGenericTreeSitterDocument<'_, '_, '_, '_, '_, '_> {
    pub(super) fn entity_export(
        &self,
        entity_index: usize,
        document: &GenericSemanticDocument<'_, '_, '_, '_, '_>,
    ) -> Result<GenericExport, GenericModelError> {
        let entity = &document.entities[entity_index];
        let policy = self.observations()?.profile.exports().value();
        let config = |name: &str| {
            policy
                .config()
                .value()
                .iter()
                .find(|(key, _)| key.value() == name)
                .map(|(_, value)| value.value().as_str())
        };
        let decision = |exported| GenericExport::Decision {
            exported,
            evidence_node_indices: Vec::new(),
        };
        match policy.strategy().value() {
            ExportStrategy::None => return Ok(decision(false)),
            ExportStrategy::CapitalizedName => {
                return Ok(decision(
                    self.upper_name
                        .as_ref()
                        .ok_or(GenericModelError::NativeContract(
                            "capitalized-name policy was not prepared",
                        ))?
                        .is_match(entity.name.display()),
                ));
            }
            ExportStrategy::NoPrefix => {
                return Ok(decision(!entity.name.display().starts_with(
                    config("prefix").filter(|s| !s.is_empty()).unwrap_or("_"),
                )));
            }
            ExportStrategy::ExportStatement => {
                let mut evidence = Vec::new();
                for exported in &document.entities {
                    self.original_context()?;
                    if exported.identity.label() != "Export" {
                        continue;
                    }
                    let targets = document.lexical_targets(
                        exported.name.display(),
                        &[],
                        exported.identity.context(),
                    );
                    if targets.contains(&entity_index) {
                        if targets.len() != 1 {
                            return Ok(GenericExport::Ambiguous {
                                entity_indices: targets,
                            });
                        }
                        evidence.extend(exported.contributions.iter().map(|c| c.name_node_index));
                    }
                }
                return Ok(GenericExport::Decision {
                    exported: !evidence.is_empty(),
                    evidence_node_indices: evidence,
                });
            }
            _ => (),
        }
        if entity.conflicts.iter().any(|issue| {
            matches!(
                issue,
                GenericEntityIssue::ConflictingDeclaration | GenericEntityIssue::InvalidDeclaration
            )
        }) {
            return Ok(GenericExport::DeclarationUnavailable {
                evidence: entity
                    .contributions
                    .iter()
                    .map(|c| c.declaration.clone())
                    .collect(),
            });
        }
        let mut declarations = Vec::new();
        for contribution in &entity.contributions {
            if let GenericDeclaration::Native { node_index, .. } = contribution.declaration
                && !declarations.contains(&node_index)
            {
                declarations.push(node_index);
            }
        }
        let [declaration] = declarations.as_slice() else {
            return Ok(GenericExport::NoDeclaration);
        };
        if self.subtree_recovered(*declaration)? {
            return Ok(GenericExport::Recovery {
                node_index: *declaration,
            });
        }
        // Only SDK leaf tokens in the declaration prefix can prove a modifier.
        // The subject boundary excludes the body, and native token kinds exclude
        // an identifier or literal whose text happens to spell the keyword.
        let observations = self.observations()?;
        let mut stack = vec![*declaration];
        let mut tokens = Vec::new();
        while let Some(index) = stack.pop() {
            self.original_context()?;
            let node = &observations.nodes[index];
            if node.range.end
                > observations.nodes[entity.contributions[0].name_node_index]
                    .range
                    .start
            {
                stack.extend(self.children[index].iter().copied());
                continue;
            }
            if observations
                .profile
                .comment_types()
                .value()
                .iter()
                .any(|kind| kind.value() == &node.kind)
            {
                continue;
            }
            if self.children[index].is_empty() {
                let text = self
                    .input
                    .prepared_span(
                        u64::try_from(node.range.start)
                            .map_err(|_| OriginError::ArithmeticOverflow)?,
                        u64::try_from(node.range.end)
                            .map_err(|_| OriginError::ArithmeticOverflow)?,
                    )?
                    .text();
                if !node.named || node.kind == text {
                    tokens.push((index, text));
                }
            } else {
                stack.extend(self.children[index].iter().copied());
            }
        }
        let mut evidence = Vec::new();
        let exported = match policy.strategy().value() {
            ExportStrategy::Modifier => {
                let keyword = config("keyword").filter(|s| !s.is_empty()).ok_or(
                    GenericModelError::InvalidPolicy {
                        field: "exports.config.keyword",
                        reason: "modifier requires a nonempty native keyword",
                    },
                )?;
                evidence.extend(
                    tokens
                        .iter()
                        .filter(|(_, text)| *text == keyword)
                        .map(|(index, _)| *index),
                );
                !evidence.is_empty()
            }
            ExportStrategy::NoModifier => {
                let keywords = policy
                    .config_list()
                    .value()
                    .iter()
                    .find(|(key, _)| key.value() == "keywords")
                    .map(|(_, value)| value.value());
                evidence.extend(
                    tokens
                        .iter()
                        .filter(|(_, text)| {
                            keywords.is_some_and(|keywords| {
                                keywords
                                    .iter()
                                    .any(|keyword| keyword.value().as_str() == *text)
                            })
                        })
                        .map(|(index, _)| *index),
                );
                evidence.is_empty()
            }
            ExportStrategy::NoStatic => {
                evidence.extend(
                    tokens
                        .iter()
                        .filter(|(_, text)| *text == "static")
                        .map(|(index, _)| *index),
                );
                evidence.is_empty()
            }
            _ => {
                return Err(GenericModelError::NativeContract(
                    "name export strategy entered declaration policy",
                ));
            }
        };
        Ok(GenericExport::Decision {
            exported,
            evidence_node_indices: evidence,
        })
    }
}
