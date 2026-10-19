use super::*;
use sqlparser::{
    ast::Ident,
    dialect::GenericDialect,
    parser::{Parser, ParserError},
    tokenizer::{Token, Tokenizer, TokenizerError},
};
use std::collections::{HashMap, HashSet};

pub(crate) enum NameError {
    Lex(TokenizerError),
    Syntax(ParserError),
    Shape,
}
pub(crate) fn decode_name(text: &str) -> Result<Vec<Ident>, NameError> {
    let tokens = Tokenizer::new(&GenericDialect, text)
        .with_unescape(true)
        .tokenize_with_location()
        .map_err(NameError::Lex)?;
    let mut parser = Parser::new(&GenericDialect).with_tokens_with_locations(tokens);
    let parts = parser
        .parse_multipart_identifier()
        .map_err(NameError::Syntax)?;
    if parts.is_empty()
        || parts.len() > 3
        || parser.peek_token().token != Token::EOF
        || parts
            .iter()
            .any(|p| p.value.is_empty() || !matches!(p.quote_style, None | Some('"')))
    {
        return Err(NameError::Shape);
    }
    Ok(parts)
}

pub(super) struct TypedNode {
    pub role: AntlrTypeRole,
    pub node: usize,
    pub name: Option<usize>,
}
pub(super) struct Subject {
    pub name: usize,
    pub declaration: usize,
    pub extent: usize,
    pub own_type: Option<usize>,
    pub roles: Vec<TypedNode>,
    pub role: AntlrDeclarationRole,
    pub recovered: bool,
    pub owner_only: bool,
}
pub(super) struct Registry {
    pub subjects: HashMap<usize, Subject>,
    pub declarations: HashMap<usize, Vec<usize>>,
    pub aliases: HashMap<usize, usize>,
    pub unnamed_schema: HashSet<usize>,
}
impl Registry {
    pub fn canonical(&self, node: usize) -> usize {
        self.aliases.get(&node).copied().unwrap_or(node)
    }
    pub fn unique(&self, node: usize) -> Option<usize> {
        let s = self.declarations.get(&self.canonical(node))?;
        (s.len() == 1).then(|| s[0])
    }
    pub fn selected(&self, matched: usize, name: usize) -> Option<usize> {
        if let Some(s) = self.subjects.get(&name)
            && !s.owner_only
            && (matched == name || self.canonical(matched) == s.declaration)
        {
            return Some(name);
        }
        self.unique(matched)
            .filter(|name| !self.subjects[name].owner_only)
    }
    pub fn build(
        raw: &SemanticDocument<'_, '_, '_>,
        control: &SnapshotContext,
    ) -> Result<Self, AntlrModelError> {
        let nodes = &raw.syntax.nodes;
        let direct = |parent: usize, kind: &str| {
            nodes
                .iter()
                .enumerate()
                .filter(move |(_, n)| {
                    n.parent == Some(parent) && n.class == NodeClass::Rule && n.kind == kind
                })
                .map(|(i, _)| i)
                .collect::<Vec<_>>()
        };
        let mut out = Self {
            subjects: HashMap::new(),
            declarations: HashMap::new(),
            aliases: HashMap::new(),
            unnamed_schema: HashSet::new(),
        };
        for (index, n) in nodes.iter().enumerate() {
            control.check()?;
            if n.parent.is_some_and(|p| p >= index) {
                return Err(AntlrModelError::NativeContract(
                    "parent is not a native ancestor",
                ));
            }
            if n.class != NodeClass::Rule {
                continue;
            }
            let (name_kind, type_kind, role) = match n.kind.as_str() {
                "create_function_external_scalar_statement"
                | "create_function_external_table_statement"
                | "create_function_old_db_external_function_statement"
                | "create_function_sourced_or_template_statement"
                | "create_function_sql_scalar_table_or_row_statement"
                | "create_function_aggregate_interface_statement" => {
                    ("function_name", None, AntlrDeclarationRole::Declaration)
                }
                "create_procedure_external_statement"
                | "create_procedure_sourced_statement"
                | "create_procedure_sql_statement" => {
                    ("procedure_name", None, AntlrDeclarationRole::Declaration)
                }
                "method_signature" | "method_specification" => {
                    ("method_name", None, AntlrDeclarationRole::Declaration)
                }
                "create_type_array_statement" => (
                    "type_name",
                    Some("data_type"),
                    AntlrDeclarationRole::Declaration,
                ),
                "create_type_cursor_statement"
                | "create_type_row_statement"
                | "create_type_structured_statement" => {
                    ("type_name", None, AntlrDeclarationRole::Declaration)
                }
                "create_type_distinct_statement" => (
                    "distinct_type_name",
                    Some("source_data_type"),
                    AntlrDeclarationRole::Declaration,
                ),
                "alter_module_statement" => {
                    ("module_name", None, AntlrDeclarationRole::Declaration)
                }
                "module_type_definition" => (
                    "unqualified_type_name",
                    if !direct(index, "data_type").is_empty() {
                        Some("data_type")
                    } else if !direct(index, "source_data_type").is_empty() {
                        Some("source_data_type")
                    } else {
                        None
                    },
                    AntlrDeclarationRole::Declaration,
                ),
                "module_variable_definition" => (
                    "variable_name",
                    Some("data_type_1"),
                    AntlrDeclarationRole::Declaration,
                ),
                "create_table_statement" => ("table_name", None, AntlrDeclarationRole::Declaration),
                "create_view_statement" => ("view_name", None, AntlrDeclarationRole::Declaration),
                "create_index_statement" => ("index_name", None, AntlrDeclarationRole::Declaration),
                "create_trigger_statement" => {
                    ("trigger_name", None, AntlrDeclarationRole::Declaration)
                }
                "create_sequence_statement" => (
                    "sequence_name",
                    Some("data_type"),
                    AntlrDeclarationRole::Declaration,
                ),
                "create_schema_statement" => {
                    ("schema_name", None, AntlrDeclarationRole::Declaration)
                }
                "create_tablespace_statement" => {
                    ("tablespace_name", None, AntlrDeclarationRole::Declaration)
                }
                "column_definition" => (
                    "column_name",
                    Some("data_type"),
                    AntlrDeclarationRole::Declaration,
                ),
                "index_col_opts_item" => (
                    "column_name",
                    None,
                    AntlrDeclarationRole::IndexKeySpecification,
                ),
                "declare_variable_statement" => (
                    "variable_name",
                    Some("data_type"),
                    AntlrDeclarationRole::Declaration,
                ),
                "agg_fn_param_decl" => (
                    "parameter_name",
                    Some("aggregate_builtin_type"),
                    AntlrDeclarationRole::Declaration,
                ),
                "ext_scalar_param_decl"
                | "ext_table_param_decl"
                | "param_decl_3"
                | "proc_ext_param"
                | "proc_parameter_list_item" => (
                    "parameter_name",
                    Some("data_type"),
                    AntlrDeclarationRole::Declaration,
                ),
                "sourced_function_param_decl" => (
                    "parameter_name",
                    Some("sourced_function_parameter_type"),
                    AntlrDeclarationRole::Declaration,
                ),
                "param_decl_2" => (
                    "parameter_name",
                    Some("data_type_1"),
                    AntlrDeclarationRole::Declaration,
                ),
                "method_signature_parameter_decl" => (
                    "parameter_name",
                    Some("method_parameter_type"),
                    AntlrDeclarationRole::Declaration,
                ),
                "method_spec_parameter_decl" => (
                    "parameter_name",
                    Some("method_spec_parameter_type"),
                    AntlrDeclarationRole::Declaration,
                ),
                "declared_alias_name" => ("alias_name", None, AntlrDeclarationRole::Declaration),
                _ => continue,
            };
            let declaration = match n.kind.as_str() {
                "method_signature" => n
                    .parent
                    .filter(|p| nodes[*p].kind == "create_method_statement")
                    .ok_or(AntlrModelError::NativeContract(
                        "method signature without native statement",
                    ))?,
                "declared_alias_name" => n
                    .parent
                    .and_then(|p| nodes[p].parent)
                    .filter(|p| nodes[*p].kind == "create_alias_statement")
                    .ok_or(AntlrModelError::NativeContract(
                        "declared alias without native statement",
                    ))?,
                _ => index,
            };
            if index != declaration {
                out.aliases.insert(index, declaration);
            }
            let names = direct(index, name_kind);
            if n.kind != "declare_variable_statement" && names.len() > 1 {
                return Err(AntlrModelError::NativeContract(
                    "multiple canonical declaration names",
                ));
            }
            if names.is_empty() && n.kind == "create_schema_statement" {
                out.unnamed_schema.insert(index);
            }
            let mut type_nodes = type_kind.map(|k| direct(index, k)).unwrap_or_default();
            if matches!(
                n.kind.as_str(),
                "ext_scalar_param_decl" | "ext_table_param_decl" | "param_decl_3"
            ) {
                type_nodes.extend(direct(index, "method_reference_type"));
            }
            if matches!(
                n.kind.as_str(),
                "ext_scalar_param_decl" | "ext_table_param_decl" | "proc_ext_param"
            ) {
                type_nodes.extend(direct(index, "procedure_external_xml_type"));
            }
            if type_nodes.len() > 1 {
                return Err(AntlrModelError::NativeContract(
                    "multiple declared type slots",
                ));
            }
            let own_type = type_nodes.first().copied().or_else(|| {
                (n.kind == "create_sequence_statement")
                    .then(|| {
                        nodes
                            .iter()
                            .enumerate()
                            .find(|(_, t)| {
                                t.parent == Some(index)
                                    && t.class == NodeClass::Terminal
                                    && t.kind == "INTEGER"
                            })
                            .map(|(i, _)| i)
                    })
                    .flatten()
            });
            let keys = out.declarations.entry(declaration).or_default();
            for name in names {
                keys.push(name);
                let mut roles = return_roles(nodes, index)?;
                if let Some(node) = own_type {
                    roles.push(TypedNode {
                        role: AntlrTypeRole::Declared,
                        node,
                        name: None,
                    });
                }
                roles.sort_by_key(|t| t.node);
                out.subjects.insert(
                    name,
                    Subject {
                        name,
                        declaration,
                        extent: declaration,
                        own_type,
                        roles,
                        role,
                        recovered: false,
                        owner_only: n.kind == "alter_module_statement",
                    },
                );
            }
        }
        for (index, n) in nodes.iter().enumerate() {
            control.check()?;
            if matches!(
                n.kind.as_str(),
                "create_function_statement" | "create_procedure_statement"
            ) {
                let branches = out
                    .declarations
                    .keys()
                    .copied()
                    .filter(|d| nodes[*d].parent == Some(index))
                    .collect::<Vec<_>>();
                if branches.len() == 1 {
                    out.aliases.insert(index, branches[0]);
                }
            }
        }
        for subject in out.subjects.values_mut() {
            control.check()?;
            let extent_span = nodes[subject.extent].span.as_ref();

            for (i, n) in nodes.iter().enumerate() {
                control.check()?;
                // A nullable option list has no consumed tokens. Retain its
                // reversed raw boundary without classifying it as recovery.
                let empty_options = n.kind == "agg_fn_option_list"
                    && n.class == NodeClass::Rule
                    && n.status
                        == SpanStatus::Unindexed(
                            super::super::UnindexedReason::ReversedRuleBoundary,
                        )
                    && !nodes.iter().any(|child| child.parent == Some(i));
                if !(n.error
                    || n.missing
                    || n.class == NodeClass::Error
                    || (matches!(n.status, SpanStatus::Unindexed(_)) && !empty_options))
                {
                    continue;
                }
                let mut ancestor = Some(i);
                while let Some(parent) = ancestor {
                    control.check()?;
                    if parent == subject.declaration {
                        subject.recovered = true;
                        break;
                    }
                    ancestor = nodes[parent].parent;
                }
            }
            for diagnostic in &raw.syntax.diagnostics {
                control.check()?;
                if diagnostic
                    .span
                    .as_ref()
                    .zip(extent_span)
                    .is_none_or(|(d, e)| d.start <= e.end && d.end >= e.start)
                {
                    subject.recovered = true;
                }
            }
        }
        Ok(out)
    }
    pub fn required_origins(
        &self,
        nodes: &[SyntaxNode],
        control: &SnapshotContext,
    ) -> Result<Vec<usize>, AntlrModelError> {
        let mut selected = HashSet::new();
        for s in self.subjects.values() {
            control.check()?;
            selected.extend([s.name, s.declaration, s.extent]);
            selected.extend(s.own_type);
            selected.extend(s.roles.iter().map(|t| t.node));
            selected.extend(s.roles.iter().filter_map(|t| t.name));
            for (i, _) in nodes.iter().enumerate().filter(|(_, n)| n.kind == "id_") {
                control.check()?;
                let mut a = Some(i);
                while let Some(p) = a {
                    control.check()?;
                    if p == s.name {
                        selected.insert(i);
                        break;
                    }
                    a = nodes[p].parent;
                }
            }
        }
        let mut selected = selected.into_iter().collect::<Vec<_>>();
        selected.sort_unstable();
        Ok(selected)
    }
}

fn children(nodes: &[SyntaxNode], parent: usize, kind: &str) -> Vec<usize> {
    nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.parent == Some(parent) && n.class == NodeClass::Rule && n.kind == kind)
        .map(|(i, _)| i)
        .collect()
}
fn return_roles(nodes: &[SyntaxNode], index: usize) -> Result<Vec<TypedNode>, AntlrModelError> {
    let mut out = Vec::new();
    let mut add = |parent, kind, role| {
        for node in children(nodes, parent, kind) {
            out.push(TypedNode {
                node,
                role,
                name: None,
            });
        }
    };
    match nodes[index].kind.as_str() {
        "create_function_aggregate_interface_statement" => {
            for clause in children(nodes, index, "aggregate_return_clause") {
                let cast = !children(nodes, clause, "aggregate_cast_target_type").is_empty();
                add(
                    clause,
                    "aggregate_builtin_type",
                    if cast {
                        AntlrTypeRole::CastSource
                    } else {
                        AntlrTypeRole::Return
                    },
                );
                add(
                    clause,
                    "aggregate_cast_target_type",
                    AntlrTypeRole::CastTarget,
                );
            }
        }
        "create_function_external_scalar_statement" => {
            let ordinary = children(nodes, index, "external_scalar_ordinary_return_type").len();
            let sources = children(nodes, index, "external_scalar_cast_source_type").len();
            let targets = children(nodes, index, "data_type_3").len();
            if ordinary > 1
                || sources > 1
                || targets > 1
                || (ordinary > 0 && (sources > 0 || targets > 0))
                || (sources > 0 && targets == 0)
            {
                return Err(AntlrModelError::NativeContract(
                    "external scalar return slots do not match grammar",
                ));
            }
            add(
                index,
                "external_scalar_ordinary_return_type",
                AntlrTypeRole::Return,
            );
            add(
                index,
                "external_scalar_cast_source_type",
                AntlrTypeRole::CastSource,
            );
            add(index, "data_type_3", AntlrTypeRole::CastTarget);
        }
        "create_function_external_table_statement"
        | "create_function_old_db_external_function_statement" => {
            let names = children(nodes, index, "column_name");
            let type_kind = if nodes[index].kind == "create_function_external_table_statement" {
                "external_table_return_type"
            } else {
                "oledb_return_type"
            };
            let types = children(nodes, index, type_kind);
            if names.len() != types.len() {
                return Err(AntlrModelError::NativeContract(
                    "return columns do not pair with types",
                ));
            }
            for (name, node) in names.into_iter().zip(types) {
                out.push(TypedNode {
                    role: AntlrTypeRole::ReturnColumn,
                    node,
                    name: Some(name),
                });
            }
        }
        "create_function_sourced_or_template_statement" => {
            for options in children(nodes, index, "fn_return_opts") {
                for clause in children(nodes, options, "fn_return_opts_item") {
                    let sourced = children(nodes, clause, "sourced_function_return_type");
                    let template = children(nodes, clause, "template_function_return_type");
                    if sourced.len() + template.len() > 1 {
                        return Err(AntlrModelError::NativeContract(
                            "sourced or template return slots do not match grammar",
                        ));
                    }
                    add(
                        clause,
                        "sourced_function_return_type",
                        AntlrTypeRole::Return,
                    );
                    add(
                        clause,
                        "template_function_return_type",
                        AntlrTypeRole::Return,
                    );
                }
            }
        }
        "create_function_sql_scalar_table_or_row_statement" => {
            for kind in [
                "data_type_1",
                "row_type_name",
                "anchored_row_data_type",
                "array_type_name",
            ] {
                add(index, kind, AntlrTypeRole::Return);
            }
            for kind in [
                "sql_function_return_row_columns",
                "sql_function_return_table_columns",
            ] {
                for columns in children(nodes, index, kind) {
                    for column in children(nodes, columns, "sql_function_return_column") {
                        let names = children(nodes, column, "column_name");
                        let types = children(nodes, column, "sql_function_return_data_type");
                        if names.len() != 1 || types.len() != 1 {
                            return Err(AntlrModelError::NativeContract(
                                "SQL return column slot mismatch",
                            ));
                        }
                        out.push(TypedNode {
                            role: AntlrTypeRole::ReturnColumn,
                            node: types[0],
                            name: Some(names[0]),
                        });
                    }
                }
            }
        }
        "method_signature" | "method_specification" => {
            let clauses = if nodes[index].kind == "method_signature" {
                vec![index]
            } else {
                children(nodes, index, "method_specification_return_clause")
            };
            for clause in clauses {
                add(clause, "method_ordinary_return_type", AntlrTypeRole::Return);
                add(clause, "method_cast_source_type", AntlrTypeRole::CastSource);
                add(clause, "method_cast_target_type", AntlrTypeRole::CastTarget);
            }
        }
        _ => {}
    }
    Ok(out)
}
