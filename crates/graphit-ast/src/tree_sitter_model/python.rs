use super::*;

mod local;

pub(super) fn details_agree(
    a: &Option<TsPythonDeclarationDetails<'_, '_, '_>>,
    b: &Option<TsPythonDeclarationDetails<'_, '_, '_>>,
    original: &SnapshotContext,
) -> Result<bool, TreeSitterModelError> {
    original.check()?;
    match (a, b) {
        (None, None) => Ok(true),
        (Some(a), Some(b)) => {
            if a.native_kind != b.native_kind
                || a.export != b.export
                || a.attachment_owner_node_index != b.attachment_owner_node_index
                || a.leading_string.as_ref().map(|s| s.node_index)
                    != b.leading_string.as_ref().map(|s| s.node_index)
                || a.attachments.len() != b.attachments.len()
            {
                return Ok(false);
            }
            for (a, b) in a.attachments.iter().zip(&b.attachments) {
                original.check()?;
                if a.placement != b.placement || !origin_agrees(a.original, b.original, original)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

impl<'doc, 'profile: 'o, 'o, 'v, 'request> Assembly<'doc, 'profile, 'o, 'v, 'request> {
    pub(super) fn is_python(&self) -> Result<bool, TreeSitterModelError> {
        Ok(self
            .parsed
            .context
            .observations()?
            .profile
            .language()
            .value()
            == "python")
    }
    fn python_native(
        &self,
        index: usize,
    ) -> Result<TsNativeSyntax<'doc, 'o, 'v>, TreeSitterModelError> {
        self.parsed.original.check()?;
        let origin = self.parsed.origin(index)?;
        Ok(TsNativeSyntax {
            node_index: index,
            kind: &self.parsed.node(index)?.kind,
            prepared: origin.prepared,
            original: &origin.original,
            raw: exact_range(&origin.original),
        })
    }
    fn python_inner(&self, declaration: usize) -> Result<usize, TreeSitterModelError> {
        if self.parsed.node(declaration)?.kind == "decorated_definition" {
            return self.parsed.field(declaration, "definition")?.ok_or(
                TreeSitterModelError::NativeContract(
                    "Python decorated definition lacks its native definition",
                ),
            );
        }
        Ok(declaration)
    }
    fn python_wrapper(&self, declaration: usize) -> Result<usize, TreeSitterModelError> {
        if let Some(parent) = self.parsed.node(declaration)?.parent
            && self.parsed.node(parent)?.kind == "decorated_definition"
            && self.parsed.field(parent, "definition")? == Some(declaration)
        {
            return Ok(parent);
        }
        Ok(declaration)
    }
    fn python_container(&self, declaration: usize) -> Result<Option<usize>, TreeSitterModelError> {
        let mut cursor = self.parsed.node(declaration)?.parent;
        while let Some(index) = cursor {
            self.parsed.original.check()?;
            let node = self.parsed.node(index)?;
            if node.kind == "lambda" {
                return Ok(Some(index));
            }
            if matches!(
                node.kind.as_str(),
                "class_definition" | "function_definition"
            ) {
                return Ok(Some(self.python_wrapper(index)?));
            }
            cursor = node.parent;
        }
        Ok(None)
    }
    pub(super) fn python_declaration(
        &self,
        name: usize,
        _label: &str,
    ) -> Result<Option<(usize, Option<usize>)>, TreeSitterModelError> {
        let node = self.parsed.node(name)?;
        let Some(parent) = node.parent else {
            return Ok(None);
        };
        let p = self.parsed.node(parent)?;
        if node.kind == "identifier" {
            if node.field.as_deref() == Some("name")
                && matches!(p.kind.as_str(), "class_definition" | "function_definition")
            {
                let declaration = self.python_wrapper(parent)?;
                return Ok(Some((declaration, self.python_container(declaration)?)));
            }
            if (node.field.as_deref() == Some("left") && p.kind == "assignment")
                || (node.field.as_deref() == Some("name")
                    && matches!(
                        p.kind.as_str(),
                        "default_parameter" | "typed_default_parameter"
                    ))
                || (p.kind == "typed_parameter" && node.field.is_none())
            {
                return Ok(Some((parent, self.python_container(parent)?)));
            }
            if p.kind == "parameters" && node.field.is_none() {
                return Ok(Some((name, self.python_container(name)?)));
            }
        }
        if node.kind == "dotted_name"
            && ((p.kind == "import_statement" && node.field.as_deref() == Some("name"))
                || (p.kind == "import_from_statement"
                    && node.field.as_deref() == Some("module_name")))
        {
            return Ok(Some((parent, self.python_container(parent)?)));
        }
        if matches!(
            node.kind.as_str(),
            "integer"
                | "float"
                | "true"
                | "false"
                | "none"
                | "string"
                | "concatenated_string"
                | "list"
                | "tuple"
                | "dictionary"
                | "set"
        ) {
            return Ok(Some((name, self.python_container(name)?)));
        }
        Ok(None)
    }
    fn python_name(
        &mut self,
        target: Target<'doc>,
        index: usize,
        raw: SourceRange<'o>,
    ) -> Result<Option<(String, TsNameRepresentation<'doc>)>, TreeSitterModelError> {
        self.parsed.original.check()?;
        let kind = self.parsed.node(index)?.kind.as_str();
        if kind == "identifier" {
            return Ok(
                match crate::python_names::canonical_identifier(raw.text()) {
                    Ok(name) => Some((name, TsNameRepresentation::Identifier)),
                    Err(error) => {
                        self.issue(
                            target,
                            TsIssue::PythonIdentifier(error),
                            Some(&self.parsed.origin(index)?.original),
                        );
                        None
                    }
                },
            );
        }
        if matches!(
            kind,
            "dotted_name"
                | "integer"
                | "float"
                | "true"
                | "false"
                | "none"
                | "string"
                | "concatenated_string"
                | "list"
                | "tuple"
                | "dictionary"
                | "set"
        ) {
            return Ok(Some((
                raw.text().to_owned(),
                TsNameRepresentation::LiteralSource { native_kind: kind },
            )));
        }
        self.issue(
            target,
            TsIssue::UnsupportedOwnerName { node_index: index },
            Some(&self.parsed.origin(index)?.original),
        );
        Ok(None)
    }
    fn python_excluded_context(
        &self,
        owner: usize,
        _selected: usize,
        target: Target<'doc>,
        _name: usize,
    ) -> Result<bool, TreeSitterModelError> {
        Ok(target.declaration == Some(owner)
            || target.declaration == Some(self.python_wrapper(owner)?))
    }
    pub(super) fn python_seed(
        &mut self,
        target: Target<'doc>,
        name_node: usize,
        raw: SourceRange<'o>,
    ) -> Result<Option<EntitySeed<'doc, 'o, 'request>>, TreeSitterModelError> {
        let mut ancestor = Some(name_node);
        while let Some(index) = ancestor {
            self.parsed.original.check()?;
            let node = self.parsed.node(index)?;
            if node.error || node.missing {
                self.issue(
                    target,
                    TsIssue::Recovery {
                        role: "name ancestry",
                        node_index: index,
                    },
                    Some(&self.parsed.origin(index)?.original),
                );
                return Ok(None);
            }
            ancestor = node.parent;
        }
        let Some((name, representation)) = self.python_name(target, name_node, raw)? else {
            return Ok(None);
        };
        let slot =
            target
                .occurrence
                .name_capture_index
                .ok_or(TreeSitterModelError::NativeContract(
                    "Python name lacks capture address",
                ))?;
        let key = (
            target.occurrence.query_index,
            target.occurrence.match_index,
            slot,
        );
        let scope_index =
            *self
                .parsed
                .scope_indices
                .get(&key)
                .ok_or(TreeSitterModelError::NativeContract(
                    "Python capture lacks scope",
                ))?;
        let scope = &self.parsed.context.scopes()?[scope_index];
        for &context_index in &scope.unresolved {
            self.issue(target, TsIssue::MissingOwner { context_index }, None);
        }
        if !scope.unresolved.is_empty() {
            return Ok(None);
        }
        let contexts = self.parsed.context.contexts()?;
        let mut symbols = Vec::new();
        let mut namespaces = Vec::new();
        let mut excluded_context_indices = scope.excluded_self.clone();
        let mut outer = None;
        for &context_index in scope.contexts.iter().rev() {
            self.parsed.original.check()?;
            let context = &contexts[context_index];
            let (Some(owner), ContextName::Found { selected, .. }) =
                (&context.owner, &context.name)
            else {
                self.issue(target, TsIssue::MissingOwner { context_index }, None);
                return Ok(None);
            };
            if self.python_excluded_context(
                owner.node_index,
                selected.node_index,
                target,
                name_node,
            )? {
                excluded_context_indices.push(context_index);
                continue;
            }
            let invalid_outer = match outer {
                Some(p) => !self.native_ancestor(p, owner.node_index)?,
                None => false,
            };
            if !self.native_ancestor(owner.node_index, name_node)?
                || !self.native_ancestor(owner.node_index, selected.node_index)?
                || invalid_outer
            {
                self.issue(
                    target,
                    TsIssue::InvalidOwner { context_index },
                    Some(&selected.original),
                );
                return Ok(None);
            }
            let origin = self.parsed.origin(selected.node_index)?;
            let Some(anchor) = exact_range(&origin.original).filter(|r| r.start() != r.end())
            else {
                self.issue(
                    target,
                    TsIssue::NonExact {
                        role: "owner_name",
                        node_index: selected.node_index,
                    },
                    Some(&origin.original),
                );
                return Ok(None);
            };
            if recovered(origin.recovery) {
                self.issue(
                    target,
                    TsIssue::Recovery {
                        role: "owner_name",
                        node_index: selected.node_index,
                    },
                    Some(&origin.original),
                );
                return Ok(None);
            }
            let Some((owner_name, owner_representation)) =
                self.python_name(target, selected.node_index, anchor)?
            else {
                return Ok(None);
            };
            let extent = self.extent(target, self.python_wrapper(owner.node_index)?, "owner")?;
            if let ExtentObservation::Exact(e) = &extent
                && (raw.start() < e.range().start()
                    || raw.end() > e.range().end()
                    || anchor.start() < e.range().start()
                    || anchor.end() > e.range().end())
            {
                self.issue(
                    target,
                    TsIssue::InvalidOwner { context_index },
                    Some(&origin.original),
                );
                return Ok(None);
            }
            let refs = symbols.iter().collect::<Vec<_>>();
            symbols.push(self.parsed.identity.symbol(
                &context.label,
                anchor,
                &refs,
                LogicalDeclaration::named(&owner_name),
            )?);
            namespaces.push(TsNamespace {
                node_index: self.python_wrapper(owner.node_index)?,
                context_index: Some(context_index),
                explicit_parent_capture_index: None,
                label: context.label.clone(),
                name: owner_name,
                anchor,
                extent,
                selection: match owner_representation {
                    TsNameRepresentation::Identifier => TsOwnerSelection::ConfiguredIdentifier,
                    TsNameRepresentation::LiteralSource { .. } => {
                        TsOwnerSelection::ConfiguredLiteralSource
                    }
                },
            });
            outer = Some(owner.node_index);
        }
        let refs = symbols.iter().collect::<Vec<_>>();
        let identity = self.parsed.identity.symbol(
            target.label,
            raw,
            &refs,
            LogicalDeclaration::named(&name),
        )?;
        Ok(Some(EntitySeed {
            identity,
            name,
            raw,
            name_representation: representation,
            namespaces,
            excluded_context_indices,
        }))
    }

    pub(super) fn python_value(
        &mut self,
        target: Target<'doc>,
        declaration: usize,
        _subject: usize,
        matched: &'doc TreeSitterProfileMatch<'o, 'v>,
    ) -> Result<TsDeclaredValue<'doc, 'o, 'v>, TreeSitterModelError> {
        let slots = match &matched.bindings.value {
            CaptureBinding::NotConfigured => return Ok(TsDeclaredValue::NotConfigured),
            CaptureBinding::Missing { declared } => {
                return Ok(TsDeclaredValue::Missing {
                    declared: *declared,
                });
            }
            CaptureBinding::Present(slots) => slots,
        };
        let initializer = self.parsed.field(
            declaration,
            if self.parsed.node(declaration)?.kind == "assignment" {
                "right"
            } else {
                "value"
            },
        )?;
        let mut captures = Vec::new();
        for &capture_index in slots {
            self.parsed.original.check()?;
            let capture =
                matched
                    .captures
                    .get(capture_index)
                    .ok_or(TreeSitterModelError::NativeContract(
                        "Python value references foreign capture",
                    ))?;
            let index = self.parsed.captured_node(capture)?;
            let binding = if initializer == Some(index) {
                TsValueBinding::Subject { ordinal: 0 }
            } else if let Some(root) = initializer
                && self.native_ancestor(root, index)?
            {
                TsValueBinding::ExpressionPart { ordinal: 0 }
            } else {
                self.issue(
                    target,
                    TsIssue::InvalidValueCapture { capture_index },
                    Some(&capture.original),
                );
                TsValueBinding::OutsideInitializer
            };
            let origin = self.parsed.origin(index)?;
            if recovered(origin.recovery) {
                self.issue(
                    target,
                    TsIssue::Recovery {
                        role: "value",
                        node_index: index,
                    },
                    Some(&origin.original),
                );
            }
            if exact_range(&origin.original).is_none() {
                self.issue(
                    target,
                    TsIssue::NonExact {
                        role: "value",
                        node_index: index,
                    },
                    Some(&origin.original),
                );
            }
            captures.push(TsValueCapture {
                capture_index,
                syntax: self.python_native(index)?,
                binding,
            });
        }
        let native_initializer = initializer.map(|i| self.python_native(i)).transpose()?;
        Ok(TsDeclaredValue::Native {
            subject_ordinal: 0,
            subject_count: 1,
            expressions: native_initializer.iter().copied().collect(),
            initializer: native_initializer.map(Box::new),
            captures,
        })
    }

    pub(super) fn python_explicit_parent(
        &mut self,
        target: Target<'doc>,
        declaration: usize,
        matched: &'doc TreeSitterProfileMatch<'o, 'v>,
        seed: &mut EntitySeed<'doc, 'o, 'request>,
    ) -> Result<(), TreeSitterModelError> {
        let slots = match &matched.bindings.parent {
            CaptureBinding::NotConfigured => return Ok(()),
            CaptureBinding::Missing { declared } => {
                self.issue(
                    target,
                    TsIssue::MissingCapture {
                        role: "parent",
                        declared: *declared,
                    },
                    None,
                );
                return Ok(());
            }
            CaptureBinding::Present(slots) => slots,
        };
        let label = self.parsed.context.observations()?.groups[target.occurrence.query_index]
            .query
            .value()
            .parent_label()
            .value();
        for &capture_index in slots {
            self.parsed.original.check()?;
            let capture =
                matched
                    .captures
                    .get(capture_index)
                    .ok_or(TreeSitterModelError::NativeContract(
                        "Python parent references foreign capture",
                    ))?;
            let index = self.parsed.captured_node(capture)?;
            let owner = self.python_declaration(index, label)?.map(|(d, _)| d);
            let origin = self.parsed.origin(index)?;
            let anchor = exact_range(&origin.original);
            let valid = match owner {
                Some(owner) => owner != declaration && self.native_ancestor(owner, declaration)?,
                None => false,
            };
            if label.is_empty() || !valid || recovered(origin.recovery) || anchor.is_none() {
                self.issue(
                    target,
                    TsIssue::InvalidExplicitParent { capture_index },
                    Some(&capture.original),
                );
                continue;
            }
            let owner = owner.ok_or(TreeSitterModelError::NativeContract(
                "checked Python parent disappeared",
            ))?;
            let anchor = anchor.ok_or(TreeSitterModelError::NativeContract(
                "checked Python parent anchor disappeared",
            ))?;
            let Some((name, representation)) = self.python_name(target, index, anchor)? else {
                continue;
            };
            let extent = self.extent(target, owner, "parent")?;
            if let Some(frame) = seed.namespaces.iter_mut().find(|n| n.node_index == owner) {
                if frame.label != *label || !same_range(frame.anchor, anchor) {
                    self.issue(
                        target,
                        TsIssue::InvalidExplicitParent { capture_index },
                        Some(&capture.original),
                    );
                } else {
                    frame.explicit_parent_capture_index = Some(capture_index);
                }
                continue;
            }
            let mut position = seed.namespaces.len();
            let mut valid_chain = true;
            for (i, frame) in seed.namespaces.iter().enumerate() {
                if self.native_ancestor(owner, frame.node_index)? {
                    position = position.min(i);
                } else if !self.native_ancestor(frame.node_index, owner)? {
                    valid_chain = false;
                    break;
                }
            }
            if !valid_chain {
                self.issue(
                    target,
                    TsIssue::InvalidExplicitParent { capture_index },
                    Some(&capture.original),
                );
                continue;
            }
            seed.namespaces.insert(
                position,
                TsNamespace {
                    node_index: owner,
                    context_index: None,
                    explicit_parent_capture_index: Some(capture_index),
                    label: label.clone(),
                    name,
                    anchor,
                    extent,
                    selection: match representation {
                        TsNameRepresentation::Identifier => TsOwnerSelection::ConfiguredIdentifier,
                        TsNameRepresentation::LiteralSource { .. } => {
                            TsOwnerSelection::ConfiguredLiteralSource
                        }
                    },
                },
            );
        }
        let mut symbols = Vec::new();
        for frame in &seed.namespaces {
            self.parsed.original.check()?;
            let refs = symbols.iter().collect::<Vec<_>>();
            symbols.push(self.parsed.identity.symbol(
                &frame.label,
                frame.anchor,
                &refs,
                LogicalDeclaration::named(&frame.name),
            )?);
        }
        seed.identity = self.parsed.identity.symbol(
            target.label,
            seed.raw,
            &symbols.iter().collect::<Vec<_>>(),
            LogicalDeclaration::named(&seed.name),
        )?;
        Ok(())
    }

    fn python_attachments(
        &self,
        owner: usize,
        issues: &mut Vec<TsCallableIssue<'doc, 'o>>,
    ) -> Result<Vec<TsAttachment<'doc, 'o>>, TreeSitterModelError> {
        let Some(parent) = self.parsed.node(owner)?.parent else {
            return Ok(Vec::new());
        };
        let siblings = &self.parsed.children[parent];
        let at = siblings.iter().position(|&i| i == owner).ok_or(
            TreeSitterModelError::NativeContract("Python declaration absent from native parent"),
        )?;
        let mut next = owner;
        let mut attachments = Vec::new();
        for position in (0..at).rev() {
            self.parsed.original.check()?;
            let index = siblings[position];
            if !self.configured_comment(index)? || !self.whitespace_gap(index, next)? {
                break;
            }
            if position > 0 {
                let predecessor = siblings[position - 1];
                if self.parsed.node(predecessor)?.end.row == self.parsed.node(index)?.start.row {
                    break;
                }
                if !self.configured_comment(predecessor)? {
                    let separation = match (
                        exact_range(&self.parsed.origin(predecessor)?.original),
                        exact_range(&self.parsed.origin(index)?.original),
                    ) {
                        (Some(a), Some(b)) if a.end() <= b.start() => {
                            let gap = self.parsed.source.range(a.end(), b.start())?.text();
                            gap.chars().all(char::is_whitespace) && gap.contains('\n')
                        }
                        _ => false,
                    };
                    if !separation {
                        issues.push(TsCallableIssue {
                            node_index: index,
                            original: Some(&self.parsed.origin(index)?.original),
                            kind: TsCallableIssueKind::InvalidAttachmentOrigin,
                        });
                        break;
                    }
                }
            }
            let origin = self.parsed.origin(index)?;
            let raw = exact_range(&origin.original);
            let valid =
                self.original_gap(index, next)? && !recovered(origin.recovery) && raw.is_some();
            if !valid {
                issues.push(TsCallableIssue {
                    node_index: index,
                    original: Some(&origin.original),
                    kind: TsCallableIssueKind::InvalidAttachmentOrigin,
                });
            }
            attachments.push(TsAttachment {
                node_index: index,
                original: &origin.original,
                raw,
                placement: TsAttachmentPlacement::Outer,
                content: if valid {
                    TsAttachmentContent::Comment
                } else {
                    TsAttachmentContent::Unavailable
                },
            });
            next = index;
        }
        attachments.reverse();
        Ok(attachments)
    }

    pub(super) fn python_declared_type(
        &mut self,
        target: Target<'doc>,
        declaration: usize,
    ) -> Result<TsDeclaredType<'doc, 'o, 'v>, TreeSitterModelError> {
        let declaration = self.python_inner(declaration)?;
        let Some(index) = self.parsed.field(declaration, "type")? else {
            return Ok(TsDeclaredType::NotApplicable);
        };
        let origin = self.parsed.origin(index)?;
        if recovered(origin.recovery) {
            self.issue(
                target,
                TsIssue::Recovery {
                    role: "type",
                    node_index: index,
                },
                Some(&origin.original),
            );
            return Ok(TsDeclaredType::Unavailable);
        }
        if exact_range(&origin.original).is_none() {
            self.issue(
                target,
                TsIssue::NonExact {
                    role: "type",
                    node_index: index,
                },
                Some(&origin.original),
            );
            return Ok(TsDeclaredType::Unavailable);
        }
        Ok(TsDeclaredType::Native(self.python_native(index)?))
    }
    pub(super) fn python_callable_kind(
        &self,
        declaration: usize,
    ) -> Result<Option<TsCallableKind>, TreeSitterModelError> {
        Ok(
            (self.parsed.node(self.python_inner(declaration)?)?.kind == "function_definition")
                .then_some(TsCallableKind::Function),
        )
    }
    fn python_subject(&self, declaration: usize) -> Result<Option<usize>, TreeSitterModelError> {
        let inner = self.python_inner(declaration)?;
        if self.parsed.node(inner)?.kind == "identifier" {
            return Ok(Some(inner));
        }
        for role in ["name", "left"] {
            if let Some(index) = self.parsed.field(inner, role)? {
                return Ok(Some(index));
            }
        }
        if self.parsed.node(inner)?.kind == "typed_parameter" {
            return self.python_parameter_name(inner);
        }
        Ok(None)
    }
    fn python_export(
        &self,
        declaration: usize,
        subject: Option<usize>,
        issues: &mut Vec<TsCallableIssue<'doc, 'o>>,
    ) -> Result<TsExport, TreeSitterModelError> {
        let policy = self
            .parsed
            .context
            .observations()?
            .profile
            .exports()
            .value();
        if policy.strategy().value() == &ExportStrategy::None {
            return Ok(TsExport::NotConfigured);
        }
        let config = policy.config().value();
        if policy.strategy().value() == &ExportStrategy::NoPrefix
            && policy.config_list().value().is_empty()
            && config.len() == 1
            && let Some((_, prefix)) = config.iter().find(|(key, _)| key.value() == "prefix")
            && let Some(subject) = subject
            && let Some(raw) = exact_range(&self.parsed.origin(subject)?.original)
            && !recovered(self.parsed.origin(subject)?.recovery)
        {
            let name = if self.parsed.node(subject)?.kind == "identifier" {
                crate::python_names::canonical_identifier(raw.text()).ok()
            } else {
                // The capture has already proved a native literal-source name.
                Some(raw.text().to_owned())
            };
            if let Some(name) = name {
                return Ok(TsExport::NamePrefix(
                    !name.starts_with(prefix.value().as_str()),
                ));
            }
        }
        issues.push(TsCallableIssue {
            node_index: declaration,
            original: None,
            kind: TsCallableIssueKind::UnsupportedExportPolicy,
        });
        Ok(TsExport::Unavailable)
    }
    fn python_leading_string(
        &self,
        declaration: usize,
    ) -> Result<Option<TsNativeSyntax<'doc, 'o, 'v>>, TreeSitterModelError> {
        let inner = self.python_inner(declaration)?;
        let Some(body) = self.parsed.field(inner, "body")? else {
            return Ok(None);
        };
        let mut first = None;
        for &index in &self.parsed.children[body] {
            self.parsed.original.check()?;
            if self.parsed.node(index)?.named && !self.configured_comment(index)? {
                first = Some(index);
                break;
            }
        }
        let Some(statement) = first else {
            return Ok(None);
        };
        if self.parsed.node(statement)?.kind != "expression_statement" {
            return Ok(None);
        }
        for &index in &self.parsed.children[statement] {
            self.parsed.original.check()?;
            if matches!(
                self.parsed.node(index)?.kind.as_str(),
                "string" | "concatenated_string"
            ) {
                return self.python_native(index).map(Some);
            }
        }
        Ok(None)
    }
    pub(super) fn python_declaration_details(
        &self,
        declaration: usize,
        name_node: usize,
    ) -> Result<TsPythonDeclarationDetails<'doc, 'o, 'v>, TreeSitterModelError> {
        let owner = self.python_wrapper(declaration)?;
        let mut issues = Vec::new();
        let export = self.python_export(declaration, Some(name_node), &mut issues)?;
        let attachments = self.python_attachments(owner, &mut issues)?;
        Ok(TsPythonDeclarationDetails {
            native_kind: &self.parsed.node(self.python_inner(declaration)?)?.kind,
            export,
            attachment_owner_node_index: owner,
            attachments,
            leading_string: self.python_leading_string(declaration)?,
            issues,
        })
    }
    fn python_signature_field(
        &self,
        callable: usize,
        role: &'static str,
        issues: &mut Vec<TsCallableIssue<'doc, 'o>>,
    ) -> Result<Option<TsNativeSyntax<'doc, 'o, 'v>>, TreeSitterModelError> {
        let Some(index) = self.parsed.field(callable, role)? else {
            return Ok(None);
        };
        let origin = self.parsed.origin(index)?;
        if recovered(origin.recovery) {
            issues.push(TsCallableIssue {
                node_index: index,
                original: Some(&origin.original),
                kind: TsCallableIssueKind::Recovery { role },
            });
        }
        if exact_range(&origin.original).is_none() {
            issues.push(TsCallableIssue {
                node_index: index,
                original: Some(&origin.original),
                kind: TsCallableIssueKind::NonExact { role },
            });
        }
        self.python_native(index).map(Some)
    }
    pub(super) fn python_function_details(
        &self,
        declaration: usize,
        kind: TsCallableKind,
    ) -> Result<TsFunctionDetails<'doc, 'o, 'v>, TreeSitterModelError> {
        let callable = self.python_inner(declaration)?;
        let header =
            self.parsed
                .signatures
                .get(&callable)
                .ok_or(TreeSitterModelError::NativeContract(
                    "native Python callable lacks signature boundaries",
                ))?;
        let body = self.parsed.field(callable, "body")?;
        let mut issues = Vec::new();
        if recovered(header.recovery) {
            issues.push(TsCallableIssue {
                node_index: callable,
                original: Some(&header.original),
                kind: TsCallableIssueKind::Recovery { role: "signature" },
            });
        }
        if exact_range(&header.original).is_none() {
            issues.push(TsCallableIssue {
                node_index: callable,
                original: Some(&header.original),
                kind: TsCallableIssueKind::NonExact { role: "signature" },
            });
        }
        let parameters = self.python_signature_field(callable, "parameters", &mut issues)?;
        if parameters.is_none() {
            issues.push(TsCallableIssue {
                node_index: callable,
                original: None,
                kind: TsCallableIssueKind::MissingField { role: "parameters" },
            });
        }
        let python = TsPythonSignature {
            parameters,
            type_parameters: self.python_signature_field(
                callable,
                "type_parameters",
                &mut issues,
            )?,
            return_type: self.python_signature_field(callable, "return_type", &mut issues)?,
        };
        let attachments =
            self.python_attachments(self.python_wrapper(declaration)?, &mut issues)?;
        let export =
            self.python_export(declaration, self.python_subject(declaration)?, &mut issues)?;
        let complexity = self.complexity(callable, body, &mut issues)?;
        Ok(TsFunctionDetails {
            declaration_node_index: callable,
            kind,
            body_node_index: body,
            defaultness_node_index: None,
            signature: TsSignature {
                prepared: header.prepared,
                original: &header.original,
                raw: exact_range(&header.original),
                syntax: None,
                go: None,
                ecma: None,
                python: Some(python),
            },
            visibility: TsVisibility {
                original: None,
                raw: None,
                syntax: None,
            },
            attachments,
            complexity,
            export,
            issues,
        })
    }
    pub(super) fn python_relation_container(
        &self,
        name: usize,
    ) -> Result<Option<usize>, TreeSitterModelError> {
        let mut cursor = Some(name);
        while let Some(index) = cursor {
            self.parsed.original.check()?;
            let node = self.parsed.node(index)?;
            if node.kind == "lambda" {
                return Ok(Some(index));
            }
            if matches!(
                node.kind.as_str(),
                "function_definition" | "class_definition" | "decorated_definition"
            ) {
                return self.python_wrapper(index).map(Some);
            }
            cursor = node.parent;
        }
        Ok(None)
    }
}
