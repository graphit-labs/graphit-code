use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AntlrOracleTypeKind {
    NestedTable,
    AssociativeArray,
    Varray,
    Record,
    RefCursor,
}

pub struct AntlrOracleKeyword<'doc, 'o> {
    pub node_index: usize,
    pub node: &'doc SyntaxNode,
    pub original: Option<&'doc OriginalProjection<'o>>,
}
pub enum AntlrOracleExpression<'doc, 'o> {
    Missing {
        parent_node_index: usize,
        role: &'static str,
    },
    Present {
        node_index: usize,
        node: &'doc SyntaxNode,
        original: Option<&'doc OriginalProjection<'o>>,
    },
}
pub enum AntlrOracleDeclaration<'doc, 'o> {
    Parameter {
        modes: Vec<AntlrOracleKeyword<'doc, 'o>>,
        default_value: Option<AntlrOracleExpression<'doc, 'o>>,
    },
    Type(Box<AntlrOracleTypeDeclaration<'doc, 'o>>),
}

pub struct AntlrOracleTypeDeclaration<'doc, 'o> {
    pub kind: AntlrOracleTypeKind,
    pub definition: AntlrTypeObservation<'doc, 'o>,
    pub element_type: Option<AntlrTypeObservation<'doc, 'o>>,
    pub index_type: Option<AntlrTypeObservation<'doc, 'o>>,
    pub bound: Option<AntlrOracleExpression<'doc, 'o>>,
    pub modifiers: Vec<AntlrOracleKeyword<'doc, 'o>>,
}

#[derive(Clone)]
pub(super) struct ExpressionNodes {
    parent: usize,
    expression: Option<usize>,
    role: &'static str,
}
#[derive(Clone)]
pub(super) enum DeclarationNodes {
    Parameter {
        modes: Vec<usize>,
        default_value: Option<ExpressionNodes>,
    },
    Type {
        kind: AntlrOracleTypeKind,
        definition: usize,
        element_type: Option<usize>,
        index_parent: Option<usize>,
        index_type: Option<usize>,
        bound: Option<ExpressionNodes>,
        modifiers: Vec<usize>,
    },
    MissingType {
        declaration: usize,
    },
}
fn children(nodes: &[SyntaxNode], parent: usize) -> impl Iterator<Item = (usize, &SyntaxNode)> {
    nodes
        .iter()
        .enumerate()
        .filter(move |(_, n)| n.parent == Some(parent))
}
fn child(nodes: &[SyntaxNode], parent: usize, kind: &str) -> Option<usize> {
    children(nodes, parent)
        .find_map(|(i, n)| (n.class == NodeClass::Rule && n.kind == kind).then_some(i))
}
pub(super) fn collect(
    nodes: &[SyntaxNode],
    declaration: usize,
    original: &SnapshotContext,
) -> Result<Option<DeclarationNodes>, AntlrModelError> {
    original.check()?;
    Ok(match nodes[declaration].kind.as_str() {
        "parameter" => {
            let modes = children(nodes, declaration)
                .filter_map(|(i, n)| {
                    (n.class == NodeClass::Terminal
                        && matches!(n.kind.as_str(), "IN" | "OUT" | "INOUT" | "NOCOPY"))
                    .then_some(i)
                })
                .collect();
            let default_value =
                child(nodes, declaration, "default_value_part").map(|parent| ExpressionNodes {
                    parent,
                    expression: child(nodes, parent, "expression"),
                    role: "parameter default",
                });
            Some(DeclarationNodes::Parameter {
                modes,
                default_value,
            })
        }
        "type_declaration" => {
            let definition = children(nodes, declaration).find_map(|(i, n)| {
                (n.class == NodeClass::Rule
                    && matches!(
                        n.kind.as_str(),
                        "table_type_def"
                            | "varray_type_def"
                            | "record_type_def"
                            | "ref_cursor_type_def"
                    ))
                .then_some(i)
            });
            if let Some(definition) = definition {
                let index_parent = child(nodes, definition, "table_indexed_by_part");
                let kind = match nodes[definition].kind.as_str() {
                    "table_type_def" if index_parent.is_some() => {
                        AntlrOracleTypeKind::AssociativeArray
                    }
                    "table_type_def" => AntlrOracleTypeKind::NestedTable,
                    "varray_type_def" => AntlrOracleTypeKind::Varray,
                    "record_type_def" => AntlrOracleTypeKind::Record,
                    "ref_cursor_type_def" => AntlrOracleTypeKind::RefCursor,
                    _ => {
                        return Err(AntlrModelError::NativeContract(
                            "noncanonical Oracle type body",
                        ));
                    }
                };
                let element_type = matches!(
                    kind,
                    AntlrOracleTypeKind::NestedTable
                        | AntlrOracleTypeKind::AssociativeArray
                        | AntlrOracleTypeKind::Varray
                )
                .then(|| child(nodes, definition, "type_spec"))
                .flatten();
                let index_type = index_parent.and_then(|p| child(nodes, p, "type_spec"));
                let bound = (kind == AntlrOracleTypeKind::Varray).then(|| ExpressionNodes {
                    parent: definition,
                    expression: child(nodes, definition, "expression"),
                    role: "varray bound",
                });
                let mut modifiers: Vec<_> = children(nodes, definition)
                    .chain(index_parent.into_iter().flat_map(|p| children(nodes, p)))
                    .filter_map(|(i, n)| {
                        (n.class == NodeClass::Terminal
                            && matches!(
                                n.kind.as_str(),
                                "TABLE"
                                    | "OF"
                                    | "NOT"
                                    | "NULL_"
                                    | "VARRAY"
                                    | "VARYING"
                                    | "ARRAY"
                                    | "RECORD"
                                    | "REF"
                                    | "CURSOR"
                                    | "RETURN"
                                    | "INDEX"
                                    | "INDEXED"
                                    | "BY"
                            ))
                        .then_some(i)
                    })
                    .collect();
                modifiers.sort_unstable();
                Some(DeclarationNodes::Type {
                    kind,
                    definition,
                    element_type,
                    index_parent,
                    index_type,
                    bound,
                    modifiers,
                })
            } else {
                Some(DeclarationNodes::MissingType { declaration })
            }
        }
        _ => None,
    })
}

pub(super) enum DeclarationFieldIssue {
    Empty(usize),
    Recovery(usize),
    NonExact(usize),
    Missing { role: &'static str, parent: usize },
    MissingExpression,
}
impl DeclarationNodes {
    pub(super) fn field_issues(
        &self,
        parsed: &ParsedAntlrDocument<'_, '_, '_, '_>,
    ) -> Result<Vec<DeclarationFieldIssue>, AntlrModelError> {
        let mut issues = Vec::new();
        for index in self.indices() {
            parsed.original.check()?;
            let node = &parsed.raw.syntax.nodes[index];
            if node
                .span
                .as_ref()
                .is_some_and(|span| span.start == span.end)
            {
                issues.push(DeclarationFieldIssue::Empty(index));
            }
            if node.error || node.missing || node.status != SpanStatus::Regular {
                issues.push(DeclarationFieldIssue::Recovery(index));
            }
            match parsed.origins.get(&index) {
                Some(OriginalProjection::Exact(_)) => {}
                Some(_) => issues.push(DeclarationFieldIssue::NonExact(index)),
                None => issues.push(DeclarationFieldIssue::Missing {
                    role: "Oracle declaration field",
                    parent: index,
                }),
            }
        }
        match self {
            Self::Type {
                definition,
                kind,
                element_type,
                index_parent,
                index_type,
                bound,
                ..
            } => {
                if matches!(
                    kind,
                    AntlrOracleTypeKind::NestedTable
                        | AntlrOracleTypeKind::AssociativeArray
                        | AntlrOracleTypeKind::Varray
                ) && element_type.is_none()
                    || index_parent.is_some() && index_type.is_none()
                {
                    issues.push(DeclarationFieldIssue::Missing {
                        role: "collection type",
                        parent: *definition,
                    });
                }
                if let Some(bound) = bound
                    && bound.expression.is_none()
                {
                    issues.push(DeclarationFieldIssue::MissingExpression);
                }
            }
            Self::Parameter {
                default_value: Some(value),
                ..
            } if value.expression.is_none() => {
                issues.push(DeclarationFieldIssue::MissingExpression)
            }
            Self::MissingType { declaration } => issues.push(DeclarationFieldIssue::Missing {
                role: "type definition",
                parent: *declaration,
            }),
            _ => {}
        }
        Ok(issues)
    }
}

impl DeclarationNodes {
    pub(super) fn indices(&self) -> Vec<usize> {
        let mut indices = match self {
            Self::Parameter {
                modes,
                default_value,
            } => {
                let mut values = modes.clone();
                if let Some(e) = default_value {
                    values.extend([Some(e.parent), e.expression].into_iter().flatten());
                }
                values
            }
            Self::Type {
                definition,
                element_type,
                index_parent,
                index_type,
                bound,
                modifiers,
                ..
            } => {
                let mut values = modifiers.clone();
                values.extend(
                    [Some(*definition), *element_type, *index_parent, *index_type]
                        .into_iter()
                        .flatten(),
                );
                if let Some(e) = bound {
                    values.extend([Some(e.parent), e.expression].into_iter().flatten());
                }
                values
            }
            Self::MissingType { declaration } => vec![*declaration],
        };
        indices.sort_unstable();
        indices.dedup();
        indices
    }
}
impl<'doc, 'profile: 'o, 'o, 'v, 'request> Assembly<'doc, 'profile, 'o, 'v, 'request> {
    fn oracle_keyword(&self, index: usize) -> AntlrOracleKeyword<'doc, 'o> {
        AntlrOracleKeyword {
            node_index: index,
            node: self.parsed.node(index),
            original: self.parsed.origins.get(&index),
        }
    }
    fn oracle_expression(
        &mut self,
        occurrence: usize,
        declaration: usize,
        nodes: ExpressionNodes,
    ) -> AntlrOracleExpression<'doc, 'o> {
        match nodes.expression {
            Some(node_index) => AntlrOracleExpression::Present {
                node_index,
                node: self.parsed.node(node_index),
                original: self.parsed.origins.get(&node_index),
            },
            None => {
                self.diagnostic(
                    occurrence,
                    Some(declaration),
                    AntlrIssue::MissingNativeField {
                        role: nodes.role,
                        parent_node_index: nodes.parent,
                    },
                    self.parsed.origins.get(&nodes.parent),
                );
                AntlrOracleExpression::Missing {
                    parent_node_index: nodes.parent,
                    role: nodes.role,
                }
            }
        }
    }
    pub(super) fn oracle_declaration(
        &mut self,
        occurrence: usize,
        declaration: usize,
    ) -> Result<Option<AntlrOracleDeclaration<'doc, 'o>>, AntlrModelError> {
        let Some(nodes) = self.parsed.declarations[&declaration].oracle_detail.clone() else {
            return Ok(None);
        };
        for issue in nodes.field_issues(self.parsed)? {
            let (issue, parent) = match issue {
                DeclarationFieldIssue::Empty(index) => (
                    AntlrIssue::MissingNativeField {
                        role: "empty Oracle declaration field",
                        parent_node_index: index,
                    },
                    index,
                ),
                DeclarationFieldIssue::Recovery(index) => {
                    (AntlrIssue::Recovery { node_index: index }, index)
                }
                DeclarationFieldIssue::NonExact(index) => (
                    AntlrIssue::NonExact {
                        role: "Oracle declaration field",
                        node_index: index,
                    },
                    index,
                ),
                DeclarationFieldIssue::Missing { role, parent } => (
                    AntlrIssue::MissingNativeField {
                        role,
                        parent_node_index: parent,
                    },
                    parent,
                ),
                // oracle_expression retains its existing field-specific diagnostic.
                DeclarationFieldIssue::MissingExpression => continue,
            };
            self.diagnostic(
                occurrence,
                Some(declaration),
                issue,
                self.parsed.origins.get(&parent),
            );
        }
        Ok(match nodes {
            DeclarationNodes::Parameter {
                modes,
                default_value,
            } => Some(AntlrOracleDeclaration::Parameter {
                modes: modes.into_iter().map(|i| self.oracle_keyword(i)).collect(),
                default_value: default_value
                    .map(|n| self.oracle_expression(occurrence, declaration, n)),
            }),
            DeclarationNodes::Type {
                kind,
                definition,
                element_type,
                index_parent,
                index_type,
                bound,
                modifiers,
            } => {
                let collection = matches!(
                    kind,
                    AntlrOracleTypeKind::NestedTable
                        | AntlrOracleTypeKind::AssociativeArray
                        | AntlrOracleTypeKind::Varray
                );
                Some(AntlrOracleDeclaration::Type(Box::new(
                    AntlrOracleTypeDeclaration {
                        kind,
                        definition: self.type_observation(Some(definition)),
                        element_type: collection.then(|| self.type_observation(element_type)),
                        index_type: index_parent.map(|_| self.type_observation(index_type)),
                        bound: bound.map(|n| self.oracle_expression(occurrence, declaration, n)),
                        modifiers: modifiers
                            .into_iter()
                            .map(|i| self.oracle_keyword(i))
                            .collect(),
                    },
                )))
            }
            DeclarationNodes::MissingType { .. } => None,
        })
    }
}
