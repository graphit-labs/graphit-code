use super::*;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Role {
    Callable,
    Class,
    Variable,
    Parameter,
    Import,
}
#[derive(Clone, Copy)]
struct Binding {
    scope: usize,
    declaration: usize,
    subject: usize,
    role: Role,
    exact: bool,
    unconditional: bool,
}
struct Environment {
    names: HashMap<String, Vec<Binding>>,
    opaque: Vec<(usize, usize)>,
}

impl<'doc, 'profile: 'o, 'o, 'v, 'request> Assembly<'doc, 'profile, 'o, 'v, 'request> {
    fn python_comprehension(&self, index: usize) -> Result<bool, TreeSitterModelError> {
        Ok(matches!(
            self.parsed.node(index)?.kind.as_str(),
            "list_comprehension"
                | "set_comprehension"
                | "dictionary_comprehension"
                | "generator_expression"
        ))
    }
    fn python_scope(
        &self,
        reference: usize,
        comprehension: bool,
    ) -> Result<usize, TreeSitterModelError> {
        let mut cursor = Some(reference);
        while let Some(index) = cursor {
            self.parsed.original.check()?;
            let node = self.parsed.node(index)?;
            if node.kind == "module" {
                return Ok(index);
            }
            if comprehension && self.python_comprehension(index)? {
                return Ok(index);
            }
            if matches!(
                node.kind.as_str(),
                "function_definition" | "class_definition" | "lambda"
            ) && let Some(body) = self.parsed.field(index, "body")?
                && self.native_ancestor(body, reference)?
            {
                return Ok(index);
            }
            cursor = node.parent;
        }
        Err(TreeSitterModelError::NativeContract(
            "Python node lacks an actual lexical block",
        ))
    }
    fn python_word(&self, index: usize) -> Result<Option<String>, TreeSitterModelError> {
        self.parsed.original.check()?;
        if self.parsed.node(index)?.kind != "identifier" {
            return Ok(None);
        }
        let origin = self.parsed.origin(index)?;
        if recovered(origin.recovery) {
            return Ok(None);
        }
        let Some(raw) = exact_range(&origin.original) else {
            return Ok(None);
        };
        let result = crate::python_names::canonical_identifier(raw.text()).ok();
        self.parsed.original.check()?;
        Ok(result)
    }
    fn python_direct_declaration(
        &self,
        declaration: usize,
        scope: usize,
    ) -> Result<bool, TreeSitterModelError> {
        let Some(parent) = self.parsed.node(declaration)?.parent else {
            return Ok(false);
        };
        Ok(parent == scope
            || (self.parsed.node(parent)?.kind == "block"
                && self.parsed.field(scope, "body")? == Some(parent)))
    }
    fn python_add_binding(
        &self,
        env: &mut Environment,
        scope: usize,
        declaration: usize,
        subject: usize,
        role: Role,
    ) -> Result<(), TreeSitterModelError> {
        self.parsed.original.check()?;
        let Some(name) = self.python_word(subject)? else {
            env.opaque.push((scope, subject));
            return Ok(());
        };
        let origin = self.parsed.origin(declaration)?;
        let exact = exact_range(&origin.original).is_some() && !recovered(origin.recovery);
        let unconditional = self.python_direct_declaration(declaration, scope)?;
        env.names.entry(name).or_default().push(Binding {
            scope,
            declaration,
            subject,
            role,
            exact,
            unconditional,
        });
        Ok(())
    }
    fn python_target_pattern(
        &self,
        env: &mut Environment,
        scope: usize,
        declaration: usize,
        pattern: usize,
        role: Role,
    ) -> Result<(), TreeSitterModelError> {
        match self.parsed.node(pattern)?.kind.as_str() {
            "identifier" => self.python_add_binding(env, scope, declaration, pattern, role),
            // These targets mutate an object; they do not bind a lexical identifier.
            "attribute" | "subscript" => Ok(()),
            _ => {
                env.opaque.push((scope, pattern));
                Ok(())
            }
        }
    }
    pub(super) fn python_parameter_name(
        &self,
        parameter: usize,
    ) -> Result<Option<usize>, TreeSitterModelError> {
        if self.parsed.node(parameter)?.kind == "identifier" {
            return Ok(Some(parameter));
        }
        if let Some(name) = self.parsed.field(parameter, "name")? {
            return Ok(Some(name));
        }
        if matches!(
            self.parsed.node(parameter)?.kind.as_str(),
            "typed_parameter" | "list_splat_pattern" | "dictionary_splat_pattern"
        ) {
            let mut name = None;
            for &index in &self.parsed.children[parameter] {
                self.parsed.original.check()?;
                if self.parsed.node(index)?.kind == "identifier"
                    && self.parsed.node(index)?.field.is_none()
                {
                    if name.is_some() {
                        return Ok(None);
                    }
                    name = Some(index);
                }
            }
            return Ok(name);
        }
        Ok(None)
    }
    fn python_import_subject(
        &self,
        index: usize,
        from: bool,
    ) -> Result<Option<usize>, TreeSitterModelError> {
        let node = self.parsed.node(index)?;
        if node.kind == "aliased_import" {
            return self.parsed.field(index, "alias");
        }
        if node.kind == "identifier" {
            return Ok(Some(index));
        }
        if node.kind == "dotted_name" {
            if from {
                let mut names = Vec::new();
                for &child in &self.parsed.children[index] {
                    self.parsed.original.check()?;
                    if self.parsed.node(child)?.kind == "identifier" {
                        names.push(child);
                    }
                }
                return Ok((names.len() == 1).then(|| names[0]));
            }
            for &child in &self.parsed.children[index] {
                self.parsed.original.check()?;
                if self.parsed.node(child)?.kind == "identifier" {
                    return Ok(Some(child));
                }
            }
        }
        Ok(None)
    }
    fn python_environment(&self) -> Result<Environment, TreeSitterModelError> {
        let nodes = &self.parsed.context.observations()?.nodes;
        let mut env = Environment {
            names: HashMap::new(),
            opaque: Vec::new(),
        };
        for (index, node) in nodes.iter().enumerate() {
            self.parsed.original.check()?;
            match node.kind.as_str() {
                "function_definition" | "class_definition" => {
                    let declaration = self.python_wrapper(index)?;
                    let parent = self.parsed.node(declaration)?.parent.ok_or(
                        TreeSitterModelError::NativeContract(
                            "Python definition lacks its native parent",
                        ),
                    )?;
                    let scope = self.python_scope(parent, true)?;
                    if let Some(subject) = self.parsed.field(index, "name")? {
                        self.python_add_binding(
                            &mut env,
                            scope,
                            declaration,
                            subject,
                            if node.kind == "function_definition" {
                                Role::Callable
                            } else {
                                Role::Class
                            },
                        )?;
                    } else {
                        env.opaque.push((scope, index));
                    }
                }
                "parameters" | "lambda_parameters" => {
                    let Some(scope) = node.parent else {
                        continue;
                    };
                    for &parameter in &self.parsed.children[index] {
                        self.parsed.original.check()?;
                        let kind = self.parsed.node(parameter)?;
                        if !kind.named
                            || kind.kind == "comment"
                            || matches!(
                                kind.kind.as_str(),
                                "positional_separator" | "keyword_separator"
                            )
                        {
                            continue;
                        }
                        if let Some(subject) = self.python_parameter_name(parameter)? {
                            self.python_add_binding(
                                &mut env,
                                scope,
                                parameter,
                                subject,
                                Role::Parameter,
                            )?;
                        } else {
                            env.opaque.push((scope, parameter));
                        }
                    }
                }
                "assignment" | "augmented_assignment" => {
                    let scope = self.python_scope(index, true)?;
                    if let Some(pattern) = self.parsed.field(index, "left")? {
                        self.python_target_pattern(
                            &mut env,
                            scope,
                            index,
                            pattern,
                            Role::Variable,
                        )?;
                    } else {
                        env.opaque.push((scope, index));
                    }
                }
                "for_statement" | "for_in_clause" => {
                    let scope = self.python_scope(index, true)?;
                    if let Some(pattern) = self.parsed.field(index, "left")? {
                        self.python_target_pattern(
                            &mut env,
                            scope,
                            index,
                            pattern,
                            Role::Variable,
                        )?;
                    } else {
                        env.opaque.push((scope, index));
                    }
                }
                "import_statement" | "import_from_statement" => {
                    let scope = self.python_scope(index, true)?;
                    let from = node.kind == "import_from_statement";
                    let mut found = false;
                    for &child in &self.parsed.children[index] {
                        self.parsed.original.check()?;
                        let n = self.parsed.node(child)?;
                        if n.field.as_deref() != Some("name") {
                            continue;
                        }
                        found = true;
                        if let Some(subject) = self.python_import_subject(child, from)? {
                            self.python_add_binding(&mut env, scope, index, subject, Role::Import)?;
                        } else {
                            env.opaque.push((scope, child));
                        }
                    }
                    if !found {
                        env.opaque.push((scope, index));
                    }
                }
                "named_expression" => {
                    // A comprehension assignment expression can affect the enclosing block.
                    let scope = self.python_scope(index, false)?;
                    env.opaque.push((scope, index));
                }
                "with_item"
                | "except_clause"
                | "type_parameter"
                | "as_pattern"
                | "global_statement"
                | "nonlocal_statement"
                | "delete_statement"
                | "match_statement"
                | "type_alias_statement" => {
                    env.opaque.push((self.python_scope(index, true)?, index));
                }
                "call" => {
                    if let Some(function) = self.parsed.field(index, "function")?
                        && self
                            .python_word(function)?
                            .is_some_and(|s| matches!(s.as_str(), "exec" | "eval"))
                    {
                        env.opaque.push((self.python_scope(index, true)?, index));
                    }
                }
                _ => {}
            }
        }
        Ok(env)
    }
    fn python_visible_scopes(&self, reference: usize) -> Result<Vec<usize>, TreeSitterModelError> {
        let mut scope = self.python_scope(reference, true)?;
        let mut result = Vec::new();
        let mut saw_function = false;
        let mut saw_class = false;
        loop {
            self.parsed.original.check()?;
            let node = self.parsed.node(scope)?;
            if node.kind != "class_definition" || (!saw_function && !saw_class) {
                result.push(scope);
            }
            saw_function |= matches!(node.kind.as_str(), "function_definition" | "lambda");
            saw_class |= node.kind == "class_definition";
            if node.kind == "module" {
                break;
            }
            let parent = node.parent.ok_or(TreeSitterModelError::NativeContract(
                "Python scope lacks enclosing block",
            ))?;
            scope = self.python_scope(parent, true)?;
        }
        Ok(result)
    }
    fn python_target(
        &self,
        env: &Environment,
        relation: &TsRelationObservation<'doc, 'profile, 'o, 'v>,
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
        let node = self.parsed.node(subject)?;
        let origin = self.parsed.origin(subject)?;
        if recovered(origin.recovery) {
            return Ok(unresolved(TsLocalUnresolvedReason::Recovery {
                node_index: subject,
            }));
        }
        if exact_range(&origin.original).is_none() {
            return Ok(unresolved(TsLocalUnresolvedReason::NonExact {
                node_index: subject,
            }));
        }
        if query.relation_type().value() != "CALLS" {
            return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                node_index: subject,
            }));
        }
        let Some(call) = node.parent else {
            return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                node_index: subject,
            }));
        };
        if self.parsed.node(call)?.kind == "attribute" {
            return Ok(unresolved(TsLocalUnresolvedReason::DynamicReceiver {
                node_index: call,
            }));
        }
        if self.parsed.node(call)?.kind != "call"
            || self.parsed.field(call, "function")? != Some(subject)
        {
            return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                node_index: call,
            }));
        }
        let mut cursor = Some(subject);
        let mut deferred = false;
        let mut enclosing_class = false;
        while let Some(index) = cursor {
            self.parsed.original.check()?;
            let n = self.parsed.node(index)?;
            if self.python_comprehension(index)? {
                // The first iterable and implicit comprehension body have different environments.
                // Until that role is proved, neither may fall back to an outer binding.
                return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                    node_index: index,
                }));
            }
            if n.kind == "decorator" {
                return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                    node_index: index,
                }));
            }
            if n.kind == "class_definition" {
                enclosing_class = true;
            }
            if matches!(n.kind.as_str(), "function_definition" | "class_definition") {
                let in_body = match self.parsed.field(index, "body")? {
                    Some(body) => self.native_ancestor(body, subject)?,
                    None => false,
                };
                if !in_body {
                    return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                        node_index: index,
                    }));
                }
                deferred |= n.kind == "function_definition";
            }
            cursor = n.parent;
        }
        let Some(name) = self.python_word(subject)? else {
            return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                node_index: subject,
            }));
        };
        if enclosing_class
            && (name == "__class__" || (name.starts_with("__") && !name.ends_with("__")))
        {
            // Private-name mangling and the implicit class cell need their own native proof.
            // Preserve the authored identifier; do not synthesize a mangled binding.
            return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                node_index: subject,
            }));
        }
        let labels = rule.value().labels().value();
        for scope in self.python_visible_scopes(subject)? {
            self.parsed.original.check()?;
            evidence.push(scope);
            if let Some((_, opaque)) = env.opaque.iter().find(|(s, _)| *s == scope) {
                evidence.push(*opaque);
                return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                    node_index: *opaque,
                }));
            }
            let bindings = env
                .names
                .get(&name)
                .map(|b| {
                    b.iter()
                        .filter(|b| b.scope == scope)
                        .copied()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if bindings.is_empty() {
                continue;
            }
            let mut candidates = Vec::new();
            for binding in bindings {
                evidence.extend([binding.declaration, binding.subject]);
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
                if binding.role != Role::Callable {
                    return Ok(unresolved(TsLocalUnresolvedReason::Shadowed {
                        declaration_node_index: binding.declaration,
                    }));
                }
                if !binding.unconditional
                    || self.parsed.node(binding.declaration)?.kind == "decorated_definition"
                {
                    return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                        node_index: binding.declaration,
                    }));
                }
                if self.parsed.node(binding.declaration)?.range.start > node.range.start
                    && !(self.parsed.node(scope)?.kind == "module" && deferred)
                {
                    return Ok(unresolved(TsLocalUnresolvedReason::Shadowed {
                        declaration_node_index: binding.declaration,
                    }));
                }
                let anchor = exact_range(&self.parsed.origin(binding.subject)?.original).ok_or(
                    TreeSitterModelError::NativeContract(
                        "Python exact binding lost its native name",
                    ),
                )?;
                for (index, entity) in self.entities.iter().enumerate() {
                    self.parsed.original.check()?;
                    if entity.name() == name
                        && labels.iter().any(|l| l.value() == entity.identity.label())
                        && same_range(entity.name_raw, anchor)
                        && entity.contributions.iter().any(|c| {
                            c.derived_value.is_none()
                                && c.declaration_node_index == binding.declaration
                        })
                        && entity.diagnostic_indices.is_empty()
                        && entity.conflicts.is_empty()
                        && entity.model_conflicts.is_empty()
                    {
                        candidates.push(index);
                    }
                }
            }
            candidates.sort_unstable();
            candidates.dedup();
            return Ok(match candidates.as_slice() {
                [] => unresolved(TsLocalUnresolvedReason::Shadowed {
                    declaration_node_index: scope,
                }),
                [entity_index] => TsLocalTargetState::Resolved {
                    entity_index: *entity_index,
                },
                _ => TsLocalTargetState::Ambiguous {
                    entity_indices: candidates,
                },
            });
        }
        Ok(TsLocalTargetState::NotFound)
    }
    pub(in crate::tree_sitter_model) fn python_local_relations(
        &self,
        relations: &[TsRelationObservation<'doc, 'profile, 'o, 'v>],
    ) -> Result<Vec<TsLocalRelationResolution>, TreeSitterModelError> {
        self.parsed.original.check()?;
        let environment = self.python_environment()?;
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
                        reason: if self.parsed.node(*container_node_index)?.kind == "lambda" {
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
                            self.python_target(
                                &environment,
                                relation,
                                source.node_index,
                                &mut evidence,
                            )?
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
