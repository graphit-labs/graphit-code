use super::*;
use crate::ecma_names::{EcmaNameDialect, decode_ecma_identifier_name};

mod local;

pub(super) fn is_ecma_language(language: &str) -> bool {
    matches!(language, "javascript" | "typescript" | "tsx")
}

pub(super) fn details_agree(
    a: &Option<TsEcmaDeclarationDetails<'_, '_>>,
    b: &Option<TsEcmaDeclarationDetails<'_, '_>>,
    original: &SnapshotContext,
) -> Result<bool, TreeSitterModelError> {
    original.check()?;
    match (a, b) {
        (None, None) => Ok(true),
        (Some(a), Some(b)) => {
            if a.native_kind != b.native_kind
                || a.export != b.export
                || a.attachment_owner_node_index != b.attachment_owner_node_index
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
    pub(super) fn is_ecma(&self) -> Result<bool, TreeSitterModelError> {
        Ok(is_ecma_language(
            self.parsed
                .context
                .observations()?
                .profile
                .language()
                .value(),
        ))
    }

    fn ecma_native(
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

    pub(super) fn ecma_declaration(
        &self,
        name: usize,
        _label: &str,
    ) -> Result<Option<(usize, Option<usize>)>, TreeSitterModelError> {
        let node = self.parsed.node(name)?;
        let Some(parent) = node.parent else {
            return Ok(None);
        };
        let p = self.parsed.node(parent)?;
        if matches!(
            node.kind.as_str(),
            "identifier" | "type_identifier" | "property_identifier"
        ) {
            if node.field.as_deref() == Some("name")
                && matches!(
                    p.kind.as_str(),
                    "function_declaration"
                        | "generator_function_declaration"
                        | "function_expression"
                        | "method_definition"
                        | "class_declaration"
                        | "interface_declaration"
                        | "type_alias_declaration"
                        | "enum_declaration"
                        | "enum_assignment"
                        | "variable_declarator"
                        | "public_field_definition"
                )
            {
                return Ok(Some((parent, self.ecma_container(parent)?)));
            }
            if (p.kind == "field_definition" && node.field.as_deref() == Some("property"))
                || (p.kind == "pair" && node.field.as_deref() == Some("key"))
                || (matches!(p.kind.as_str(), "required_parameter" | "optional_parameter")
                    && node.field.as_deref() == Some("pattern"))
            {
                return Ok(Some((parent, self.ecma_container(parent)?)));
            }
            if p.kind == "formal_parameters" && node.kind == "identifier" && node.field.is_none() {
                return Ok(Some((name, p.parent)));
            }
            if p.kind == "enum_body"
                && node.kind == "property_identifier"
                && node.field.as_deref() == Some("name")
            {
                return Ok(Some((name, p.parent)));
            }
        }
        if node.kind == "string"
            && node.field.as_deref() == Some("source")
            && matches!(p.kind.as_str(), "import_statement" | "export_statement")
        {
            return Ok(Some((parent, None)));
        }
        if node.kind == "string"
            && p.kind == "arguments"
            && let Some(call) = p.parent
            && self.parsed.node(call)?.kind == "call_expression"
            && self.parsed.field(call, "arguments")? == Some(parent)
        {
            return Ok(Some((call, None)));
        }
        if node.kind == "string_fragment" && p.kind == "string" {
            if let Some(pair) = p.parent
                && self.parsed.node(pair)?.kind == "pair"
                && self.parsed.field(pair, "key")? == Some(parent)
            {
                return Ok(Some((pair, None)));
            }
            if let Some(array) = p.parent
                && self.parsed.node(array)?.kind == "array"
            {
                return Ok(Some((name, Some(array))));
            }
        }
        Ok(None)
    }

    fn ecma_container(&self, declaration: usize) -> Result<Option<usize>, TreeSitterModelError> {
        let Some(parent) = self.parsed.node(declaration)?.parent else {
            return Ok(None);
        };
        let node = self.parsed.node(parent)?;
        Ok(
            if matches!(
                node.kind.as_str(),
                "class_body" | "enum_body" | "interface_body" | "formal_parameters"
            ) {
                node.parent
            } else {
                None
            },
        )
    }

    fn ecma_name(
        &mut self,
        target: Target<'doc>,
        index: usize,
        raw: SourceRange<'o>,
    ) -> Result<Option<(String, TsNameRepresentation<'doc>)>, TreeSitterModelError> {
        let kind = self.parsed.node(index)?.kind.as_str();
        if matches!(
            kind,
            "identifier" | "type_identifier" | "property_identifier"
        ) {
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
            let decoded = decode_ecma_identifier_name(raw.text(), dialect);
            self.parsed.original.check()?;
            return Ok(match decoded {
                Ok(name) => Some((name, TsNameRepresentation::Identifier)),
                Err(error) => {
                    self.issue(
                        target,
                        TsIssue::EcmaIdentifier(error),
                        Some(&self.parsed.origin(index)?.original),
                    );
                    None
                }
            });
        }
        if matches!(
            kind,
            "string"
                | "string_fragment"
                | "number"
                | "true"
                | "false"
                | "null"
                | "undefined"
                | "template_string"
                | "regex"
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

    fn ecma_excluded_context(
        &self,
        owner: usize,
        selected: usize,
        target: Target<'doc>,
        name: usize,
    ) -> Result<bool, TreeSitterModelError> {
        if target.declaration == Some(owner) {
            return Ok(true);
        }
        if self.parsed.node(owner)?.kind != "lexical_declaration" {
            return Ok(false);
        }
        for &child in &self.parsed.children[owner] {
            self.parsed.original.check()?;
            if self.parsed.node(child)?.kind != "variable_declarator"
                || !self.native_ancestor(child, name)?
            {
                continue;
            }
            let Some(own_name) = self.parsed.field(child, "name")? else {
                return Ok(false);
            };
            return Ok(target.declaration == Some(child) || selected != own_name);
        }
        Ok(false)
    }

    pub(super) fn ecma_seed(
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
        let Some((name, representation)) = self.ecma_name(target, name_node, raw)? else {
            return Ok(None);
        };
        let slot =
            target
                .occurrence
                .name_capture_index
                .ok_or(TreeSitterModelError::NativeContract(
                    "ECMA name lacks capture address",
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
                    "ECMA capture lacks scope",
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
            if self.ecma_excluded_context(
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
                self.ecma_name(target, selected.node_index, anchor)?
            else {
                return Ok(None);
            };
            let extent = self.extent(target, owner.node_index, "owner")?;
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
                node_index: owner.node_index,
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

    pub(super) fn ecma_declared_type(
        &mut self,
        target: Target<'doc>,
        declaration: usize,
    ) -> Result<TsDeclaredType<'doc, 'o, 'v>, TreeSitterModelError> {
        let role = if self.parsed.node(declaration)?.kind == "type_alias_declaration" {
            "value"
        } else {
            "type"
        };
        let Some(index) = self.parsed.field(declaration, role)? else {
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
        Ok(TsDeclaredType::Native(self.ecma_native(index)?))
    }

    pub(super) fn ecma_value(
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
        let initializer = self.parsed.field(declaration, "value")?;
        let mut captures = Vec::new();
        for &capture_index in slots {
            self.parsed.original.check()?;
            let capture =
                matched
                    .captures
                    .get(capture_index)
                    .ok_or(TreeSitterModelError::NativeContract(
                        "ECMA value references foreign capture",
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
                syntax: self.ecma_native(index)?,
                binding,
            });
        }
        let native_initializer = initializer.map(|i| self.ecma_native(i)).transpose()?;
        Ok(TsDeclaredValue::Native {
            subject_ordinal: 0,
            subject_count: 1,
            expressions: native_initializer.iter().copied().collect(),
            initializer: native_initializer.map(Box::new),
            captures,
        })
    }

    pub(super) fn ecma_explicit_parent(
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
                        "ECMA parent references foreign capture",
                    ))?;
            let index = self.parsed.captured_node(capture)?;
            let owner = self.ecma_declaration(index, label)?.map(|(d, _)| d);
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
                "checked ECMA parent disappeared",
            ))?;
            let anchor = anchor.ok_or(TreeSitterModelError::NativeContract(
                "checked ECMA parent anchor disappeared",
            ))?;
            let Some((name, representation)) = self.ecma_name(target, index, anchor)? else {
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

    fn ecma_callable_node(&self, declaration: usize) -> Result<usize, TreeSitterModelError> {
        if self.parsed.node(declaration)?.kind == "variable_declarator"
            && let Some(value) = self.parsed.field(declaration, "value")?
            && matches!(
                self.parsed.node(value)?.kind.as_str(),
                "arrow_function" | "function_expression" | "generator_function"
            )
        {
            return Ok(value);
        }
        Ok(declaration)
    }

    pub(super) fn ecma_callable_kind(
        &self,
        declaration: usize,
    ) -> Result<Option<TsCallableKind>, TreeSitterModelError> {
        let callable = self.ecma_callable_node(declaration)?;
        Ok(match self.parsed.node(callable)?.kind.as_str() {
            "function_declaration" => Some(TsCallableKind::Function),
            "generator_function_declaration" | "generator_function" => {
                Some(TsCallableKind::GeneratorEcma)
            }
            "method_definition" => Some(TsCallableKind::MethodEcma),
            "arrow_function" => Some(TsCallableKind::ArrowEcma),
            "function_expression" => Some(TsCallableKind::ExpressionEcma),
            _ => None,
        })
    }

    fn ecma_attachment_owner(&self, declaration: usize) -> Result<usize, TreeSitterModelError> {
        let mut owner = declaration;
        if let Some(parent) = self.parsed.node(owner)?.parent
            && self.parsed.node(owner)?.kind == "variable_declarator"
            && matches!(
                self.parsed.node(parent)?.kind.as_str(),
                "lexical_declaration" | "variable_declaration"
            )
        {
            owner = parent;
        }
        if let Some(parent) = self.parsed.node(owner)?.parent
            && self.parsed.node(parent)?.kind == "export_statement"
            && self.parsed.field(parent, "declaration")? == Some(owner)
        {
            owner = parent;
        }
        Ok(owner)
    }

    fn ecma_export(
        &self,
        declaration: usize,
        issues: &mut Vec<TsCallableIssue<'doc, 'o>>,
    ) -> Result<TsExport, TreeSitterModelError> {
        let policy = self
            .parsed
            .context
            .observations()?
            .profile
            .exports()
            .value();
        match policy.strategy().value() {
            ExportStrategy::None => Ok(TsExport::NotConfigured),
            ExportStrategy::ExportStatement
                if policy.config().value().is_empty()
                    && policy.config_list().value().is_empty() =>
            {
                Ok(TsExport::Statement(
                    self.parsed
                        .node(self.ecma_attachment_owner(declaration)?)?
                        .kind
                        == "export_statement",
                ))
            }
            _ => {
                issues.push(TsCallableIssue {
                    node_index: declaration,
                    original: None,
                    kind: TsCallableIssueKind::UnsupportedExportPolicy,
                });
                Ok(TsExport::Unavailable)
            }
        }
    }

    fn ecma_attachments(
        &self,
        owner: usize,
        issues: &mut Vec<TsCallableIssue<'doc, 'o>>,
    ) -> Result<Vec<TsAttachment<'doc, 'o>>, TreeSitterModelError> {
        let Some(parent) = self.parsed.node(owner)?.parent else {
            return Ok(Vec::new());
        };
        let siblings = &self.parsed.children[parent];
        let at = siblings.iter().position(|&i| i == owner).ok_or(
            TreeSitterModelError::NativeContract("ECMA declaration absent from native parent"),
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

    pub(super) fn ecma_declaration_details(
        &self,
        declaration: usize,
    ) -> Result<TsEcmaDeclarationDetails<'doc, 'o>, TreeSitterModelError> {
        let owner = self.ecma_attachment_owner(declaration)?;
        let mut issues = Vec::new();
        let export = self.ecma_export(declaration, &mut issues)?;
        let attachments = self.ecma_attachments(owner, &mut issues)?;
        Ok(TsEcmaDeclarationDetails {
            native_kind: &self.parsed.node(declaration)?.kind,
            export,
            attachment_owner_node_index: owner,
            attachments,
            issues,
        })
    }

    fn ecma_signature_field(
        &self,
        callable: usize,
        role: &'static str,
        issues: &mut Vec<TsCallableIssue<'doc, 'o>>,
    ) -> Result<Option<TsNativeSyntax<'doc, 'o, 'v>>, TreeSitterModelError> {
        let Some(index) = self.parsed.field(callable, role)? else {
            return Ok(None);
        };
        let syntax = self.ecma_native(index)?;
        if recovered(self.parsed.origin(index)?.recovery) {
            issues.push(TsCallableIssue {
                node_index: index,
                original: Some(syntax.original),
                kind: TsCallableIssueKind::Recovery { role },
            });
        }
        if syntax.raw.is_none() {
            issues.push(TsCallableIssue {
                node_index: index,
                original: Some(syntax.original),
                kind: TsCallableIssueKind::NonExact { role },
            });
        }
        Ok(Some(syntax))
    }

    pub(super) fn ecma_function_details(
        &self,
        declaration: usize,
        kind: TsCallableKind,
    ) -> Result<TsFunctionDetails<'doc, 'o, 'v>, TreeSitterModelError> {
        let callable = self.ecma_callable_node(declaration)?;
        let header =
            self.parsed
                .signatures
                .get(&callable)
                .ok_or(TreeSitterModelError::NativeContract(
                    "native ECMA callable lacks signature boundaries",
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
        let parameters = self
            .ecma_signature_field(callable, "parameters", &mut issues)?
            .or(self.ecma_signature_field(callable, "parameter", &mut issues)?);
        if parameters.is_none() {
            issues.push(TsCallableIssue {
                node_index: callable,
                original: None,
                kind: TsCallableIssueKind::MissingField { role: "parameters" },
            });
        }
        let ecma = TsEcmaSignature {
            parameters,
            type_parameters: self.ecma_signature_field(callable, "type_parameters", &mut issues)?,
            return_type: self.ecma_signature_field(callable, "return_type", &mut issues)?,
        };
        let owner = self.ecma_attachment_owner(declaration)?;
        let attachments = self.ecma_attachments(owner, &mut issues)?;
        let export = self.ecma_export(declaration, &mut issues)?;
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
                ecma: Some(ecma),
                python: None,
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

    pub(super) fn ecma_relation_container(
        &self,
        name: usize,
    ) -> Result<Option<usize>, TreeSitterModelError> {
        let mut cursor = Some(name);
        while let Some(index) = cursor {
            self.parsed.original.check()?;
            let node = self.parsed.node(index)?;
            if matches!(
                node.kind.as_str(),
                "arrow_function" | "function_expression" | "generator_function"
            ) {
                if let Some(parent) = node.parent
                    && self.parsed.node(parent)?.kind == "variable_declarator"
                    && self.parsed.field(parent, "value")? == Some(index)
                    && self.parsed.field(parent, "name")?.is_some()
                {
                    return Ok(Some(parent));
                }
                return Ok(Some(index));
            }
            if matches!(
                node.kind.as_str(),
                "function_declaration"
                    | "generator_function_declaration"
                    | "method_definition"
                    | "class_declaration"
                    | "interface_declaration"
                    | "enum_declaration"
            ) {
                return Ok(Some(index));
            }
            if node.kind == "export_statement"
                && let Some(declaration) = self.parsed.field(index, "declaration")?
            {
                return Ok(Some(declaration));
            }
            cursor = node.parent;
        }
        Ok(None)
    }
}
