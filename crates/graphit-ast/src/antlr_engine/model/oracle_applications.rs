//! Native application syntax. These observations never establish a resolved call.
use super::{
    AntlrModelError, AntlrOracleExpression, NodeClass, OriginalProjection, QueryKind,
    SemanticDocument, SnapshotContext, SyntaxNode,
};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AntlrOracleApplicationKind {
    GeneralElementPart,
    KeywordMethod,
    RoutineName,
    CollectionMethodStatement,
}

/// A real syntax node and its independently projected Original, if projectable.
#[derive(Clone, Copy)]
pub struct AntlrNativeSyntax<'doc, 'o> {
    pub node_index: usize,
    pub node: &'doc SyntaxNode,
    pub original: Option<&'doc OriginalProjection<'o>>,
}

pub struct AntlrOracleArgument<'doc, 'o> {
    pub syntax: AntlrNativeSyntax<'doc, 'o>,
    pub name: Option<AntlrNativeSyntax<'doc, 'o>>,
    pub expression: AntlrOracleExpression<'doc, 'o>,
}
pub struct AntlrOracleArgumentGroup<'doc, 'o> {
    pub syntax: AntlrNativeSyntax<'doc, 'o>,
    pub arguments: Vec<AntlrOracleArgument<'doc, 'o>>,
    pub keep_clause: Option<AntlrNativeSyntax<'doc, 'o>>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AntlrOracleAssignmentSide {
    Target,
    Value,
}
pub struct AntlrOracleAssignmentContext<'doc, 'o> {
    pub statement: AntlrNativeSyntax<'doc, 'o>,
    pub branch: AntlrNativeSyntax<'doc, 'o>,
    pub side: AntlrOracleAssignmentSide,
    /// Expression ancestors between the head and assignment, nearest first.
    pub expressions: Vec<AntlrNativeSyntax<'doc, 'o>>,
}
pub struct AntlrOracleCollectionStatement<'doc, 'o> {
    pub syntax: AntlrNativeSyntax<'doc, 'o>,
    pub receiver: AntlrOracleExpression<'doc, 'o>,
    pub indices: Vec<AntlrNativeSyntax<'doc, 'o>>,
    pub delimiters: Vec<AntlrNativeSyntax<'doc, 'o>>,
}
pub struct AntlrOracleMemberApplication<'doc, 'o> {
    pub syntax: AntlrNativeSyntax<'doc, 'o>,
    /// Present only when one native node describes the complete receiver.
    pub receiver: Option<AntlrNativeSyntax<'doc, 'o>>,
    pub name: Option<AntlrNativeSyntax<'doc, 'o>>,
    pub separator: Option<AntlrNativeSyntax<'doc, 'o>>,
    pub delimiters: Vec<AntlrNativeSyntax<'doc, 'o>>,
}
pub struct AntlrOracleApplication<'doc, 'o> {
    pub occurrence_index: usize,
    pub kind: AntlrOracleApplicationKind,
    pub head: AntlrNativeSyntax<'doc, 'o>,
    pub groups: Vec<AntlrOracleArgumentGroup<'doc, 'o>>,
    /// Native ancestors, nearest first. This is not a caller or execution grant.
    pub boundaries: Vec<AntlrNativeSyntax<'doc, 'o>>,
    pub collection_statement: Option<AntlrOracleCollectionStatement<'doc, 'o>>,
    pub member: Option<AntlrOracleMemberApplication<'doc, 'o>>,
    pub assignment: Option<AntlrOracleAssignmentContext<'doc, 'o>>,
}

struct ArgumentNodes {
    node: usize,
    name: Option<usize>,
    expression: Option<usize>,
}
struct GroupNodes {
    node: usize,
    arguments: Vec<ArgumentNodes>,
    keep_clause: Option<usize>,
}
struct CollectionNodes {
    node: usize,
    receiver: Option<usize>,
    indices: Vec<usize>,
    delimiters: Vec<usize>,
}
struct MemberNodes {
    syntax: usize,
    receiver: Option<usize>,
    name: Option<usize>,
    separator: Option<usize>,
    delimiters: Vec<usize>,
}
struct AssignmentNodes {
    statement: usize,
    branch: usize,
    side: AntlrOracleAssignmentSide,
    expressions: Vec<usize>,
}
pub(super) struct ApplicationNodes {
    occurrence_index: usize,
    kind: AntlrOracleApplicationKind,
    head: usize,
    groups: Vec<GroupNodes>,
    boundaries: Vec<usize>,
    collection: Option<CollectionNodes>,
    member: Option<MemberNodes>,
    assignment: Option<AssignmentNodes>,
}
impl ApplicationNodes {
    pub(super) fn member_reference(&self) -> Option<(usize, Option<usize>)> {
        self.member
            .as_ref()
            .map(|member| (self.occurrence_index, member.receiver))
    }
    pub(super) fn indices(&self) -> Vec<usize> {
        let mut indices = vec![self.head];
        indices.extend(&self.boundaries);
        if let Some(collection) = &self.collection {
            indices.push(collection.node);
            indices.extend(collection.receiver);
            indices.extend(&collection.indices);
            indices.extend(&collection.delimiters);
        }
        if let Some(member) = &self.member {
            indices.push(member.syntax);
            indices.extend(member.receiver);
            indices.extend(member.name);
            indices.extend(member.separator);
            indices.extend(&member.delimiters);
        }
        if let Some(assignment) = &self.assignment {
            indices.extend([assignment.statement, assignment.branch]);
            indices.extend(&assignment.expressions);
        }
        for group in &self.groups {
            indices.push(group.node);
            indices.extend(group.keep_clause);
            for argument in &group.arguments {
                indices.push(argument.node);
                indices.extend(argument.name);
                indices.extend(argument.expression);
            }
        }
        indices
    }
}

fn is_boundary(kind: &str) -> bool {
    matches!(
        kind,
        "create_function_body"
            | "function_body"
            | "function_spec"
            | "create_procedure_body"
            | "procedure_body"
            | "procedure_spec"
            | "create_package"
            | "create_package_body"
            | "parameter"
            | "default_value_part"
            | "variable_declaration"
            | "body"
            | "block"
            | "anonymous_block"
            | "trigger_block"
            | "tps_block"
            | "create_trigger"
            | "execute_immediate"
    )
}

fn assignment_context(
    head: usize,
    nodes: &[SyntaxNode],
    original: &SnapshotContext,
) -> Result<Option<AssignmentNodes>, AntlrModelError> {
    let mut branch = head;
    let mut expressions = Vec::new();
    loop {
        original.check()?;
        if nodes[branch].class == NodeClass::Rule && nodes[branch].kind == "expression" {
            expressions.push(branch);
        }
        let Some(parent) = nodes[branch].parent else {
            return Ok(None);
        };
        if nodes[parent].kind == "assignment_statement" {
            let side = match nodes[branch].kind.as_str() {
                "general_element" | "bind_variable" => AntlrOracleAssignmentSide::Target,
                "expression" => AntlrOracleAssignmentSide::Value,
                _ => {
                    return Err(AntlrModelError::NativeContract(
                        "assignment has unsupported native branch",
                    ));
                }
            };
            return Ok(Some(AssignmentNodes {
                statement: parent,
                branch,
                side,
                expressions,
            }));
        }
        branch = parent;
    }
}
fn accessor_part(nodes: &[SyntaxNode], matched: usize) -> Option<usize> {
    if nodes[matched].class != NodeClass::Terminal
        || !matches!(
            nodes[matched].kind.as_str(),
            "COUNT" | "FIRST" | "LAST" | "LIMIT" | "NEXT"
        )
    {
        return None;
    }
    let mut index = matched;
    for expected in [
        "non_reserved_keywords_pre12c",
        "regular_id",
        "id_expression",
        "general_element_part",
    ] {
        index = nodes[index].parent?;
        if nodes[index].class != NodeClass::Rule || nodes[index].kind != expected {
            return None;
        }
    }
    Some(index)
}
fn member_nodes(
    head: usize,
    kind: AntlrOracleApplicationKind,
    groups: &[GroupNodes],
    nodes: &[SyntaxNode],
    children: &[Vec<usize>],
    positions: &[usize],
) -> Option<MemberNodes> {
    if !matches!(
        kind,
        AntlrOracleApplicationKind::GeneralElementPart | AntlrOracleApplicationKind::KeywordMethod
    ) {
        return None;
    }
    let parent = nodes[head].parent?;
    let parts = &children[parent];
    if nodes[parent].kind != "general_element"
        || !parts.iter().any(|&index| {
            nodes[index].class == NodeClass::Rule && nodes[index].kind == "general_element"
        })
    {
        return None;
    }
    let position = positions[head];
    let separator = position.checked_sub(1).and_then(|index| {
        let index = parts[index];
        (nodes[index].class == NodeClass::Terminal && nodes[index].kind == "PERIOD")
            .then_some(index)
    });
    let expected_len = if kind == AntlrOracleApplicationKind::KeywordMethod {
        3 + groups.len()
    } else {
        3
    };
    let simple = parts.len() == expected_len
        && position == 2
        && nodes[parts[0]].class == NodeClass::Rule
        && nodes[parts[0]].kind == "general_element"
        && separator.is_some()
        && (kind != AntlrOracleApplicationKind::KeywordMethod
            || parts[3..]
                .iter()
                .copied()
                .eq(groups.iter().map(|group| group.node)));
    let name = if kind == AntlrOracleApplicationKind::KeywordMethod {
        children[head]
            .iter()
            .copied()
            .find(|&index| nodes[index].class == NodeClass::Terminal)
    } else {
        children[head].iter().copied().find(|&index| {
            nodes[index].class == NodeClass::Rule && nodes[index].kind == "id_expression"
        })
    };
    let delimiters = groups
        .iter()
        .flat_map(|group| children[group.node].iter().copied())
        .filter(|&index| {
            nodes[index].class == NodeClass::Terminal
                && matches!(
                    nodes[index].kind.as_str(),
                    "LEFT_PAREN" | "RIGHT_PAREN" | "COMMA"
                )
        })
        .collect();
    Some(MemberNodes {
        syntax: parent,
        receiver: simple.then_some(parts[0]),
        name,
        separator,
        delimiters,
    })
}
pub(super) fn collect(
    raw: &SemanticDocument<'_, '_, '_>,
    original: &SnapshotContext,
) -> Result<Vec<ApplicationNodes>, AntlrModelError> {
    if raw.profile.grammar().value() != "antlr-plsql" {
        return Ok(Vec::new());
    }
    let nodes = &raw.syntax.nodes;
    let mut children = vec![Vec::new(); nodes.len()];
    let mut positions = vec![0; nodes.len()];
    for (index, node) in nodes.iter().enumerate() {
        original.check()?;
        if let Some(parent) = node.parent {
            let siblings = children
                .get_mut(parent)
                .ok_or(AntlrModelError::NativeContract(
                    "application node has foreign native parent",
                ))?;
            positions[index] = siblings.len();
            siblings.push(index);
        }
    }
    let child = |parent: usize, kind: &str| {
        children[parent]
            .iter()
            .copied()
            .find(|&i| nodes[i].class == NodeClass::Rule && nodes[i].kind == kind)
    };
    let mut applications = Vec::new();
    for (occurrence_index, occurrence) in raw.occurrences.iter().enumerate() {
        original.check()?;
        if *occurrence.query.r#type().value() != QueryKind::Relation
            || occurrence.query.relation_type().value() != "CALLS"
        {
            continue;
        }
        let matched = occurrence.matched.node_index;
        let collection = if nodes[matched].class == NodeClass::Terminal
            && matches!(nodes[matched].kind.as_str(), "DELETE" | "EXTEND" | "TRIM")
            && nodes[matched]
                .parent
                .is_some_and(|p| nodes[p].kind == "collection_method_call")
        {
            let node = nodes[matched]
                .parent
                .expect("checked native collection parent");
            let expressions: Vec<_> = children[node]
                .iter()
                .copied()
                .filter(|&i| nodes[i].class == NodeClass::Rule && nodes[i].kind == "expression")
                .collect();
            let delimiters = children[node]
                .iter()
                .copied()
                .filter(|&i| {
                    nodes[i].class == NodeClass::Terminal
                        && matches!(
                            nodes[i].kind.as_str(),
                            "LEFT_PAREN" | "RIGHT_PAREN" | "COMMA"
                        )
                })
                .collect();
            Some(CollectionNodes {
                node,
                receiver: expressions.first().copied(),
                indices: expressions.into_iter().skip(1).collect(),
                delimiters,
            })
        } else {
            None
        };
        let head = if let Some(part) = accessor_part(nodes, matched) {
            part
        } else if nodes[matched].class == NodeClass::Terminal
            && nodes[matched]
                .parent
                .is_some_and(|p| nodes[p].kind == "keyword_as_method")
        {
            nodes[matched]
                .parent
                .expect("checked native keyword parent")
        } else {
            matched
        };
        if collection.is_none() && nodes[head].class != NodeClass::Rule {
            continue;
        }
        let kind = if collection.is_some() {
            AntlrOracleApplicationKind::CollectionMethodStatement
        } else {
            match nodes[head].kind.as_str() {
                "general_element_part" => AntlrOracleApplicationKind::GeneralElementPart,
                "keyword_as_method" => AntlrOracleApplicationKind::KeywordMethod,
                "routine_name" => AntlrOracleApplicationKind::RoutineName,
                _ => continue,
            }
        };
        let group_indices: Vec<usize> = if collection.is_some() {
            Vec::new()
        } else if kind == AntlrOracleApplicationKind::GeneralElementPart {
            children[head]
                .iter()
                .copied()
                .filter(|&i| nodes[i].kind == "function_argument")
                .collect()
        } else if let Some(parent) = nodes[head].parent {
            let expected = if kind == AntlrOracleApplicationKind::KeywordMethod {
                "general_element"
            } else {
                "call_statement"
            };
            if nodes[parent].kind != expected {
                return Err(AntlrModelError::NativeContract(
                    "application head has unexpected native parent",
                ));
            }
            let following = children[parent]
                .iter()
                .copied()
                .skip_while(|&i| i != head)
                .skip(1)
                .take_while(|&i| nodes[i].kind == "function_argument");
            if kind == AntlrOracleApplicationKind::RoutineName {
                following.take(1).collect()
            } else {
                following.collect()
            }
        } else {
            return Err(AntlrModelError::NativeContract(
                "application head has no native parent",
            ));
        };
        let mut groups = Vec::new();
        for node in group_indices {
            original.check()?;
            let mut arguments = Vec::new();
            for &argument in &children[node] {
                original.check()?;
                if nodes[argument].class == NodeClass::Rule && nodes[argument].kind == "argument" {
                    arguments.push(ArgumentNodes {
                        node: argument,
                        name: child(argument, "identifier"),
                        expression: child(argument, "expression"),
                    });
                }
            }
            groups.push(GroupNodes {
                node,
                arguments,
                keep_clause: child(node, "keep_clause"),
            });
        }
        let mut boundaries = Vec::new();
        let mut ancestor = nodes[head].parent;
        while let Some(index) = ancestor {
            original.check()?;
            if nodes[index].class == NodeClass::Rule && is_boundary(&nodes[index].kind) {
                boundaries.push(index);
            }
            ancestor = nodes[index].parent;
        }
        let member = member_nodes(head, kind, &groups, nodes, &children, &positions);
        let assignment = assignment_context(head, nodes, original)?;
        applications.push(ApplicationNodes {
            occurrence_index,
            kind,
            head,
            groups,
            boundaries,
            collection,
            member,
            assignment,
        });
    }
    Ok(applications)
}

pub(super) fn observe<'doc, 'o>(
    application: &ApplicationNodes,
    nodes: &'doc [SyntaxNode],
    origins: &'doc HashMap<usize, OriginalProjection<'o>>,
) -> AntlrOracleApplication<'doc, 'o> {
    let syntax = |index| AntlrNativeSyntax {
        node_index: index,
        node: &nodes[index],
        original: origins.get(&index),
    };
    AntlrOracleApplication {
        occurrence_index: application.occurrence_index,
        kind: application.kind,
        collection_statement: application.collection.as_ref().map(|collection| {
            AntlrOracleCollectionStatement {
                syntax: syntax(collection.node),
                receiver: match collection.receiver {
                    Some(index) => AntlrOracleExpression::Present {
                        node_index: index,
                        node: &nodes[index],
                        original: origins.get(&index),
                    },
                    None => AntlrOracleExpression::Missing {
                        parent_node_index: collection.node,
                        role: "collection receiver",
                    },
                },
                indices: collection.indices.iter().copied().map(syntax).collect(),
                delimiters: collection.delimiters.iter().copied().map(syntax).collect(),
            }
        }),
        member: application
            .member
            .as_ref()
            .map(|member| AntlrOracleMemberApplication {
                syntax: syntax(member.syntax),
                receiver: member.receiver.map(syntax),
                name: member.name.map(syntax),
                separator: member.separator.map(syntax),
                delimiters: member.delimiters.iter().copied().map(syntax).collect(),
            }),
        assignment: application.assignment.as_ref().map(|assignment| {
            AntlrOracleAssignmentContext {
                statement: syntax(assignment.statement),
                branch: syntax(assignment.branch),
                side: assignment.side,
                expressions: assignment.expressions.iter().copied().map(syntax).collect(),
            }
        }),
        head: syntax(application.head),
        groups: application
            .groups
            .iter()
            .map(|group| AntlrOracleArgumentGroup {
                syntax: syntax(group.node),
                arguments: group
                    .arguments
                    .iter()
                    .map(|argument| AntlrOracleArgument {
                        syntax: syntax(argument.node),
                        name: argument.name.map(syntax),
                        expression: match argument.expression {
                            Some(index) => AntlrOracleExpression::Present {
                                node_index: index,
                                node: &nodes[index],
                                original: origins.get(&index),
                            },
                            None => AntlrOracleExpression::Missing {
                                parent_node_index: argument.node,
                                role: "argument expression",
                            },
                        },
                    })
                    .collect(),
                keep_clause: group.keep_clause.map(syntax),
            })
            .collect(),
        boundaries: application.boundaries.iter().copied().map(syntax).collect(),
    }
}
