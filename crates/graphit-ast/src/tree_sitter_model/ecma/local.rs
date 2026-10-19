use super::*;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Role {
    Callable,
    Variable,
    Parameter,
    Import,
    Class,
    Type,
    BlockFunction,
}
#[derive(Clone, Copy)]
struct Binding {
    declaration: usize,
    subject: usize,
    scope: usize,
    role: Role,
    lexical: bool,
    exact: bool,
}
struct Environment {
    names: HashMap<String, Vec<Binding>>,
    opaque: Vec<(usize, usize)>,
    legacy_uncertainty: HashMap<String, Vec<(usize, usize)>>,
}

impl<'doc, 'profile: 'o, 'o, 'v, 'request> Assembly<'doc, 'profile, 'o, 'v, 'request> {
    fn ecma_scope(&self, mut index: usize, function: bool) -> Result<usize, TreeSitterModelError> {
        loop {
            self.parsed.original.check()?;
            let n = self.parsed.node(index)?;
            if n.kind == "program"
                || (function && is_callable_node(n))
                || (!function
                    && matches!(
                        n.kind.as_str(),
                        "statement_block"
                            | "for_statement"
                            | "for_in_statement"
                            | "switch_body"
                            | "catch_clause"
                    ))
            {
                return Ok(index);
            }
            index = n.parent.ok_or(TreeSitterModelError::NativeContract(
                "ECMA binding lacks native scope",
            ))?;
        }
    }

    fn ecma_word(&self, index: usize) -> Result<Option<String>, TreeSitterModelError> {
        let origin = self.parsed.origin(index)?;
        let Some(raw) = exact_range(&origin.original) else {
            return Ok(None);
        };
        let dialect = if self
            .parsed
            .context
            .observations()?
            .profile
            .language()
            .value()
            == "javascript"
        {
            EcmaNameDialect::JavaScript
        } else {
            EcmaNameDialect::TypeScript
        };
        self.parsed.original.check()?;
        let name = decode_ecma_identifier_name(raw.text(), dialect).ok();
        self.parsed.original.check()?;
        Ok(name)
    }

    fn ecma_pattern(
        &self,
        root: usize,
        names: &mut Vec<usize>,
    ) -> Result<bool, TreeSitterModelError> {
        self.parsed.original.check()?;
        let node = self.parsed.node(root)?;
        match node.kind.as_str() {
            "identifier"
            | "type_identifier"
            | "property_identifier"
            | "shorthand_property_identifier_pattern" => {
                names.push(root);
                Ok(true)
            }
            "pair_pattern" => match self.parsed.field(root, "value")? {
                Some(i) => self.ecma_pattern(i, names),
                None => Ok(false),
            },
            "assignment_pattern" | "object_assignment_pattern" => {
                match self.parsed.field(root, "left")? {
                    Some(i) => self.ecma_pattern(i, names),
                    None => Ok(false),
                }
            }
            "required_parameter" | "optional_parameter" => {
                match self.parsed.field(root, "pattern")? {
                    Some(i) => self.ecma_pattern(i, names),
                    None => Ok(false),
                }
            }
            "rest_pattern" | "object_pattern" | "array_pattern" => {
                for &i in &self.parsed.children[root] {
                    self.parsed.original.check()?;
                    if self.parsed.node(i)?.named
                        && self.parsed.node(i)?.kind != "comment"
                        && !self.ecma_pattern(i, names)?
                    {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn ecma_add_binding(
        &self,
        env: &mut Environment,
        declaration: usize,
        pattern: usize,
        scope: usize,
        role: Role,
        lexical: bool,
    ) -> Result<(), TreeSitterModelError> {
        let mut names = Vec::new();
        if !self.ecma_pattern(pattern, &mut names)? {
            env.opaque.push((scope, pattern));
            return Ok(());
        }
        for subject in names {
            self.parsed.original.check()?;
            let Some(name) = self.ecma_word(subject)? else {
                env.opaque.push((scope, subject));
                continue;
            };
            let exact = exact_range(&self.parsed.origin(declaration)?.original).is_some()
                && !recovered(self.parsed.origin(declaration)?.recovery)
                && !recovered(self.parsed.origin(subject)?.recovery);
            env.names.entry(name).or_default().push(Binding {
                declaration,
                subject,
                scope,
                role,
                lexical,
                exact,
            });
        }
        Ok(())
    }

    fn ecma_environment(&self) -> Result<Environment, TreeSitterModelError> {
        let nodes = &self.parsed.context.observations()?.nodes;
        let mut env = Environment {
            names: HashMap::new(),
            opaque: Vec::new(),
            legacy_uncertainty: HashMap::new(),
        };
        for (index, node) in nodes.iter().enumerate() {
            self.parsed.original.check()?;
            match node.kind.as_str() {
                "function_declaration"
                | "generator_function_declaration"
                | "class_declaration"
                | "interface_declaration"
                | "type_alias_declaration"
                | "enum_declaration" => {
                    let Some(name) = self.parsed.field(index, "name")? else {
                        continue;
                    };
                    let parent = node.parent.ok_or(TreeSitterModelError::NativeContract(
                        "ECMA declaration lacks parent",
                    ))?;
                    let scope = self.ecma_scope(parent, false)?;
                    let role = match node.kind.as_str() {
                        "class_declaration" => Role::Class,
                        "interface_declaration" | "type_alias_declaration" => Role::Type,
                        "enum_declaration" => Role::Class,
                        _ if !((parent == scope
                            || (nodes[parent].kind == "export_statement"
                                && self.parsed.field(parent, "declaration")? == Some(index)
                                && nodes[parent].parent == Some(scope)))
                            && (nodes[scope].kind == "program"
                                || (nodes[scope].kind == "statement_block"
                                    && nodes[scope].parent.is_some_and(|p| {
                                        is_callable_node(&nodes[p])
                                            && nodes[scope].field.as_deref() == Some("body")
                                    })))) =>
                        {
                            Role::BlockFunction
                        }
                        _ => Role::Callable,
                    };
                    if role == Role::BlockFunction {
                        if let Some(word) = self.ecma_word(name)? {
                            env.legacy_uncertainty
                                .entry(word)
                                .or_default()
                                .push((self.ecma_scope(parent, true)?, index));
                        } else {
                            env.opaque.push((self.ecma_scope(parent, true)?, name));
                        }
                    }
                    self.ecma_add_binding(
                        &mut env,
                        index,
                        name,
                        scope,
                        role,
                        matches!(role, Role::Class | Role::Type),
                    )?;
                }
                "variable_declarator" => {
                    let Some(name) = self.parsed.field(index, "name")? else {
                        continue;
                    };
                    let parent = node.parent.ok_or(TreeSitterModelError::NativeContract(
                        "ECMA variable lacks parent",
                    ))?;
                    let lexical = nodes[parent].kind == "lexical_declaration";
                    if !lexical && nodes[parent].kind != "variable_declaration" {
                        env.opaque.push((self.ecma_scope(parent, false)?, name));
                        continue;
                    }
                    let scope = self.ecma_scope(parent, !lexical)?;
                    let scope = if !lexical && is_callable_node(&nodes[scope]) {
                        self.parsed.field(scope, "body")?.ok_or(
                            TreeSitterModelError::NativeContract("ECMA var callable lacks body"),
                        )?
                    } else {
                        scope
                    };
                    self.ecma_add_binding(&mut env, index, name, scope, Role::Variable, lexical)?;
                }
                "catch_clause" => {
                    if let Some(parameter) = self.parsed.field(index, "parameter")? {
                        self.ecma_add_binding(
                            &mut env,
                            index,
                            parameter,
                            index,
                            Role::Parameter,
                            false,
                        )?;
                    }
                }
                "formal_parameters" => {
                    let Some(callable) = node.parent else {
                        continue;
                    };
                    if !is_callable_node(&nodes[callable]) {
                        continue;
                    }
                    for &pattern in &self.parsed.children[index] {
                        if nodes[pattern].named && nodes[pattern].kind != "comment" {
                            self.ecma_add_binding(
                                &mut env,
                                callable,
                                pattern,
                                callable,
                                Role::Parameter,
                                false,
                            )?;
                        }
                    }
                }
                "arrow_function" => {
                    if let Some(parameter) = self.parsed.field(index, "parameter")? {
                        self.ecma_add_binding(
                            &mut env,
                            index,
                            parameter,
                            index,
                            Role::Parameter,
                            false,
                        )?;
                    }
                }
                "function_expression" | "generator_function" => {
                    if let Some(name) = self.parsed.field(index, "name")? {
                        self.ecma_add_binding(&mut env, index, name, index, Role::Callable, false)?;
                    }
                }
                "import_specifier" | "namespace_import" | "import_clause" => {
                    let mut import = index;
                    while nodes[import].kind != "import_statement" {
                        self.parsed.original.check()?;
                        import =
                            nodes[import]
                                .parent
                                .ok_or(TreeSitterModelError::NativeContract(
                                    "native import binding lacks statement",
                                ))?;
                    }
                    let scope = self.ecma_scope(import, false)?;
                    if node.kind == "import_specifier" {
                        let name = self
                            .parsed
                            .field(index, "alias")?
                            .or(self.parsed.field(index, "name")?);
                        if let Some(name) = name {
                            self.ecma_add_binding(
                                &mut env,
                                import,
                                name,
                                scope,
                                Role::Import,
                                false,
                            )?;
                        } else {
                            env.opaque.push((scope, index));
                        }
                    } else {
                        for &child in &self.parsed.children[index] {
                            if nodes[child].kind == "identifier" {
                                self.ecma_add_binding(
                                    &mut env,
                                    import,
                                    child,
                                    scope,
                                    Role::Import,
                                    false,
                                )?;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(env)
    }

    fn ecma_visible<'a>(
        &self,
        env: &'a Environment,
        name: &str,
        subject: usize,
        type_space: bool,
    ) -> Result<Result<Vec<&'a Binding>, usize>, TreeSitterModelError> {
        let mut cursor = Some(subject);
        while let Some(scope) = cursor {
            self.parsed.original.check()?;
            if let Some(&(_, opaque)) = env.opaque.iter().find(|&&(s, _)| s == scope) {
                return Ok(Err(opaque));
            }
            let bindings = env
                .names
                .get(name)
                .into_iter()
                .flatten()
                .filter(|b| {
                    b.scope == scope
                        && ((type_space
                            && matches!(b.role, Role::Type | Role::Class | Role::Import))
                            || (!type_space && b.role != Role::Type))
                })
                .collect::<Vec<_>>();
            if !bindings.is_empty() {
                return Ok(Ok(bindings));
            }
            if !type_space
                && let Some(&(_, declaration)) = env
                    .legacy_uncertainty
                    .get(name)
                    .into_iter()
                    .flatten()
                    .find(|&&(s, _)| s == scope)
            {
                return Ok(Err(declaration));
            }
            cursor = self.parsed.node(scope)?.parent;
        }
        Ok(Ok(Vec::new()))
    }

    fn ecma_function_scope(&self, mut index: usize) -> Result<Option<usize>, TreeSitterModelError> {
        loop {
            self.parsed.original.check()?;
            let node = self.parsed.node(index)?;
            if is_callable_node(node) {
                return Ok(Some(index));
            }
            let Some(parent) = node.parent else {
                return Ok(None);
            };
            index = parent;
        }
    }

    fn ecma_static(&self, index: usize) -> Result<bool, TreeSitterModelError> {
        for &child in &self.parsed.children[index] {
            self.parsed.original.check()?;
            let node = self.parsed.node(child)?;
            if !node.named && node.kind == "static" {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn ecma_this_class(
        &self,
        mut index: usize,
    ) -> Result<Option<(usize, bool)>, TreeSitterModelError> {
        loop {
            self.parsed.original.check()?;
            let node = self.parsed.node(index)?;
            if matches!(
                node.kind.as_str(),
                "function_declaration"
                    | "generator_function_declaration"
                    | "function_expression"
                    | "generator_function"
            ) {
                return Ok(None);
            }
            if matches!(
                node.kind.as_str(),
                "method_definition" | "field_definition" | "public_field_definition"
            ) {
                let Some(body) = node.parent else {
                    return Ok(None);
                };
                let Some(class) = self.parsed.node(body)?.parent else {
                    return Ok(None);
                };
                if self.parsed.node(body)?.kind == "class_body"
                    && self.parsed.node(class)?.kind == "class_declaration"
                    && self.parsed.field(class, "body")? == Some(body)
                {
                    return Ok(Some((class, self.ecma_static(index)?)));
                }
                return Ok(None);
            }
            let Some(parent) = node.parent else {
                return Ok(None);
            };
            index = parent;
        }
    }

    fn ecma_target(
        &self,
        env: &Environment,
        relation: &TsRelationObservation<'_, '_, '_, '_>,
        subject: usize,
        evidence: &mut Vec<usize>,
    ) -> Result<TsLocalTargetState, TreeSitterModelError> {
        let unresolved = |reason| TsLocalTargetState::Unresolved { reason };
        let Some(rule) = relation.target_rule else {
            return Ok(unresolved(TsLocalUnresolvedReason::MissingTargetRule));
        };
        let query = relation.query.value();
        if !rule.value().fallback().value().is_empty()
            || !query.target_fallback().value().is_empty()
            || !query.qualifier_capture().value().is_empty()
            || !query.parent_capture().value().is_empty()
        {
            return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedPolicy));
        }
        let origin = self.parsed.origin(subject)?;
        if recovered(origin.recovery) {
            return Ok(unresolved(TsLocalUnresolvedReason::Recovery {
                node_index: subject,
            }));
        }
        let Some(raw) = exact_range(&origin.original) else {
            return Ok(unresolved(TsLocalUnresolvedReason::NonExact {
                node_index: subject,
            }));
        };
        if self
            .name_reject
            .get(&relation.query_index)
            .is_some_and(|r| r.is_match(raw.text()))
        {
            return Ok(unresolved(TsLocalUnresolvedReason::ExcludedName));
        }
        let node = self.parsed.node(subject)?;
        if !matches!(
            node.kind.as_str(),
            "identifier" | "type_identifier" | "property_identifier"
        ) {
            return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                node_index: subject,
            }));
        }
        let Some(name) = self.ecma_word(subject)? else {
            return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                node_index: subject,
            }));
        };
        let labels = rule.value().labels().value();
        let kind = query.relation_type().value().as_str();
        let mut candidates = Vec::new();
        if let Some(member) = node.parent
            && self.parsed.node(member)?.kind == "member_expression"
            && self.parsed.field(member, "property")? == Some(subject)
        {
            if !matches!(kind, "CALLS" | "READS_FIELD" | "WRITES_FIELD") {
                return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                    node_index: member,
                }));
            }
            if kind == "READS_FIELD"
                && let Some(assignment) = self.parsed.node(member)?.parent
                && self.parsed.node(assignment)?.kind == "assignment_expression"
                && self.parsed.field(assignment, "left")? == Some(member)
            {
                return Ok(unresolved(TsLocalUnresolvedReason::NotReadAccess {
                    node_index: member,
                }));
            }
            let Some(object) = self.parsed.field(member, "object")? else {
                return Ok(unresolved(TsLocalUnresolvedReason::UnknownReceiver {
                    node_index: member,
                }));
            };
            evidence.push(object);
            if self.parsed.node(object)?.kind != "this" {
                if self.parsed.node(object)?.kind == "identifier"
                    && let Some(receiver) = self.ecma_word(object)?
                    && let Ok(bindings) = self.ecma_visible(env, &receiver, subject, false)?
                    && let Some(binding) =
                        bindings.iter().find(|b| b.role == Role::Import && b.exact)
                {
                    evidence.extend([binding.declaration, binding.subject]);
                    return Ok(unresolved(TsLocalUnresolvedReason::ExternalImport {
                        declaration_node_index: binding.declaration,
                    }));
                }
                return Ok(unresolved(TsLocalUnresolvedReason::DynamicReceiver {
                    node_index: object,
                }));
            }
            if !self
                .parsed
                .context
                .observations()?
                .profile
                .self_keywords()
                .value()
                .iter()
                .any(|s| s.value() == "this.")
            {
                return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedPolicy));
            }
            let Some((class, is_static)) = self.ecma_this_class(member)? else {
                return Ok(unresolved(TsLocalUnresolvedReason::DynamicReceiver {
                    node_index: object,
                }));
            };
            evidence.push(class);
            for (i, entity) in self.entities.iter().enumerate() {
                self.parsed.original.check()?;
                if entity.name() != name
                    || !labels.iter().any(|l| l.value() == entity.identity.label())
                {
                    continue;
                }
                for c in &entity.contributions {
                    if c.derived_value.is_some() || c.container_node_index != Some(class) {
                        continue;
                    }
                    let native_kind = self.parsed.node(c.declaration_node_index)?.kind.as_str();
                    let valid_kind = if kind == "CALLS" {
                        native_kind == "method_definition"
                    } else {
                        matches!(native_kind, "field_definition" | "public_field_definition")
                    };
                    if valid_kind
                        && self.ecma_static(c.declaration_node_index)? == is_static
                        && entity.diagnostic_indices.is_empty()
                        && entity.conflicts.is_empty()
                        && entity.model_conflicts.is_empty()
                    {
                        candidates.push(i);
                        evidence.push(c.declaration_node_index);
                    }
                }
            }
        } else {
            if !matches!(
                kind,
                "CALLS" | "INSTANTIATES" | "INHERITS" | "IMPLEMENTS" | "DECORATOR"
            ) {
                return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                    node_index: subject,
                }));
            }
            let native_role = match (kind, node.parent) {
                ("CALLS", Some(parent)) => {
                    self.parsed.node(parent)?.kind == "call_expression"
                        && self.parsed.field(parent, "function")? == Some(subject)
                }
                ("INSTANTIATES", Some(parent)) => {
                    self.parsed.node(parent)?.kind == "new_expression"
                        && self.parsed.field(parent, "constructor")? == Some(subject)
                }
                ("INHERITS", Some(parent)) => {
                    self.parsed.node(parent)?.kind == "class_heritage"
                        || (self.parsed.node(parent)?.kind == "extends_clause"
                            && self.parsed.field(parent, "value")? == Some(subject))
                }
                ("IMPLEMENTS", Some(parent)) => {
                    self.parsed.node(parent)?.kind == "implements_clause"
                }
                ("DECORATOR", Some(parent)) => {
                    self.parsed.node(parent)?.kind == "decorator"
                        || (self.parsed.node(parent)?.kind == "call_expression"
                            && self.parsed.field(parent, "function")? == Some(subject)
                            && match self.parsed.node(parent)?.parent {
                                Some(p) => self.parsed.node(p)?.kind == "decorator",
                                None => false,
                            })
                }
                _ => false,
            };
            if !native_role {
                return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                    node_index: subject,
                }));
            }
            let type_space = node.kind == "type_identifier" || kind == "IMPLEMENTS";
            let visible = match self.ecma_visible(env, &name, subject, type_space)? {
                Ok(bindings) => bindings,
                Err(node_index) => {
                    evidence.push(node_index);
                    return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                        node_index,
                    }));
                }
            };
            for binding in visible {
                evidence.extend([binding.declaration, binding.subject, binding.scope]);
                if !binding.exact {
                    return Ok(unresolved(TsLocalUnresolvedReason::Recovery {
                        node_index: binding.declaration,
                    }));
                }
                if binding.role == Role::Import {
                    return Ok(unresolved(TsLocalUnresolvedReason::ExternalImport {
                        declaration_node_index: binding.declaration,
                    }));
                }
                if binding.role == Role::BlockFunction {
                    return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                        node_index: binding.declaration,
                    }));
                }
                let same_execution_scope = self.ecma_function_scope(subject)?
                    == self.ecma_function_scope(binding.declaration)?;
                if !type_space
                    && binding.lexical
                    && same_execution_scope
                    && node.range.start < self.parsed.node(binding.declaration)?.range.end
                {
                    return Ok(unresolved(TsLocalUnresolvedReason::TemporalDeadZone {
                        declaration_node_index: binding.declaration,
                    }));
                }
                if binding.role == Role::Parameter
                    || (binding.role == Role::Variable
                        && (self.ecma_callable_kind(binding.declaration)?.is_none()
                            || (same_execution_scope
                                && node.range.start
                                    < self.parsed.node(binding.declaration)?.range.end)))
                {
                    return Ok(unresolved(TsLocalUnresolvedReason::Shadowed {
                        declaration_node_index: binding.declaration,
                    }));
                }
                let mut found = false;
                for (i, entity) in self.entities.iter().enumerate() {
                    self.parsed.original.check()?;
                    if entity.name() == name
                        && labels.iter().any(|l| l.value() == entity.identity.label())
                        && same_range(
                            entity.name_raw,
                            exact_range(&self.parsed.origin(binding.subject)?.original).ok_or(
                                TreeSitterModelError::NativeContract("exact binding lost name"),
                            )?,
                        )
                        && entity.contributions.iter().any(|c| {
                            c.derived_value.is_none()
                                && c.declaration_node_index == binding.declaration
                        })
                        && entity.diagnostic_indices.is_empty()
                        && entity.conflicts.is_empty()
                        && entity.model_conflicts.is_empty()
                    {
                        candidates.push(i);
                        found = true;
                    }
                }
                if !found && matches!(binding.role, Role::Variable | Role::Callable) {
                    return Ok(unresolved(TsLocalUnresolvedReason::Shadowed {
                        declaration_node_index: binding.declaration,
                    }));
                }
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        Ok(match candidates.as_slice() {
            [] => TsLocalTargetState::NotFound,
            [entity_index] => TsLocalTargetState::Resolved {
                entity_index: *entity_index,
            },
            _ => TsLocalTargetState::Ambiguous {
                entity_indices: candidates,
            },
        })
    }

    pub(in crate::tree_sitter_model) fn ecma_local_relations(
        &self,
        relations: &[TsRelationObservation<'doc, 'profile, 'o, 'v>],
    ) -> Result<Vec<TsLocalRelationResolution>, TreeSitterModelError> {
        self.parsed.original.check()?;
        let env = self.ecma_environment()?;
        let mut results = Vec::new();
        for (relation_index, relation) in relations.iter().enumerate() {
            for (source_index, source) in relation.sources.iter().enumerate() {
                self.parsed.original.check()?;
                let mut evidence = vec![source.node_index];
                let target = match &source.state {
                    TsRelationSourceState::MissingContainer => TsLocalTargetState::Unresolved {
                        reason: TsLocalUnresolvedReason::MissingSource,
                    },
                    TsRelationSourceState::UnavailableContainer {
                        container_node_index,
                    } => TsLocalTargetState::Unresolved {
                        reason: if matches!(
                            self.parsed.node(*container_node_index)?.kind.as_str(),
                            "arrow_function" | "function_expression" | "generator_function"
                        ) {
                            TsLocalUnresolvedReason::AnonymousSource {
                                node_index: *container_node_index,
                            }
                        } else {
                            TsLocalUnresolvedReason::UnavailableSource {
                                node_index: *container_node_index,
                            }
                        },
                    },
                    TsRelationSourceState::Bound {
                        container_node_index,
                        entity_indices,
                    } => {
                        evidence.push(*container_node_index);
                        if recovered(self.parsed.origin(*container_node_index)?.recovery)
                            || entity_indices.iter().any(|&i| {
                                !self.entities[i].diagnostic_indices.is_empty()
                                    || !self.entities[i].conflicts.is_empty()
                                    || !self.entities[i].model_conflicts.is_empty()
                            })
                        {
                            TsLocalTargetState::Unresolved {
                                reason: TsLocalUnresolvedReason::UnavailableSource {
                                    node_index: *container_node_index,
                                },
                            }
                        } else {
                            self.ecma_target(&env, relation, source.node_index, &mut evidence)?
                        }
                    }
                };
                results.push(TsLocalRelationResolution {
                    relation_index,
                    source_index,
                    capture_index: source.capture_index,
                    evidence_node_indices: evidence,
                    target,
                });
            }
        }
        self.parsed.original.check()?;
        Ok(results)
    }
}
