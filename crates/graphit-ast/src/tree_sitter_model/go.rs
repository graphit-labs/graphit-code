use super::*;
use std::sync::LazyLock;

#[derive(Clone, Copy)]
struct GoBinding {
    declaration: usize,
    subject: usize,
    scope: usize,
    depth: usize,
    visible_after: usize,
    exact: bool,
}

impl<'doc, 'profile: 'o, 'o, 'v, 'request> Assembly<'doc, 'profile, 'o, 'v, 'request> {
    pub(super) fn is_go(&self) -> Result<bool, TreeSitterModelError> {
        Ok(self
            .parsed
            .context
            .observations()?
            .profile
            .language()
            .value()
            == "go")
    }

    pub(super) fn go_declaration(
        &self,
        name: usize,
        label: &str,
    ) -> Result<Option<(usize, Option<usize>)>, TreeSitterModelError> {
        let node = self.parsed.node(name)?;
        let Some(parent) = node.parent else {
            return Ok(None);
        };
        let declaration = self.parsed.node(parent)?;
        if declaration.kind == "import_spec"
            && node.field.as_deref() == Some("path")
            && matches!(
                node.kind.as_str(),
                "interpreted_string_literal" | "raw_string_literal"
            )
        {
            return Ok(Some((parent, None)));
        }
        if node.field.as_deref() != Some("name")
            && !(declaration.kind == "const_spec" && node.kind == "identifier")
        {
            return Ok(None);
        }
        match (node.kind.as_str(), declaration.kind.as_str()) {
            (
                "identifier",
                "function_declaration"
                | "const_spec"
                | "var_spec"
                | "parameter_declaration"
                | "variadic_parameter_declaration",
            )
            | ("field_identifier", "method_declaration") => Ok(Some((parent, None))),
            ("type_identifier", "type_spec" | "type_alias") => {
                let Some(ty) = self.parsed.field(parent, "type")? else {
                    return Ok(Some((parent, None)));
                };
                if (label == "Struct" && self.parsed.node(ty)?.kind != "struct_type")
                    || (label == "Interface" && self.parsed.node(ty)?.kind != "interface_type")
                {
                    Ok(None)
                } else {
                    Ok(Some((parent, None)))
                }
            }
            ("field_identifier", "field_declaration") => {
                let Some(list) = declaration.parent else {
                    return Ok(None);
                };
                if self.parsed.node(list)?.kind != "field_declaration_list" {
                    return Ok(None);
                }
                let Some(ty) = self.parsed.node(list)?.parent else {
                    return Ok(None);
                };
                if self.parsed.node(ty)?.kind != "struct_type" {
                    return Ok(None);
                }
                let container = match self.parsed.node(ty)?.parent {
                    Some(p)
                        if matches!(
                            self.parsed.node(p)?.kind.as_str(),
                            "type_spec" | "type_alias"
                        ) =>
                    {
                        Some(p)
                    }
                    _ => None,
                };
                Ok(Some((parent, container)))
            }
            _ => Ok(None),
        }
    }

    fn go_names(&self, declaration: usize) -> Result<Vec<usize>, TreeSitterModelError> {
        let nodes = &self.parsed.context.observations()?.nodes;
        Ok(self.parsed.children[declaration]
            .iter()
            .copied()
            .filter(|&i| {
                nodes[i].named
                    && matches!(
                        nodes[i].kind.as_str(),
                        "identifier" | "type_identifier" | "field_identifier"
                    )
                    && (nodes[i].field.as_deref() == Some("name")
                        || (nodes[declaration].kind == "const_spec"
                            && nodes[i].kind == "identifier"))
            })
            .collect())
    }

    pub(super) fn go_declared_type(
        &mut self,
        target: Target<'doc>,
        declaration: usize,
    ) -> Result<TsDeclaredType<'doc, 'o, 'v>, TreeSitterModelError> {
        let kind = self.parsed.node(declaration)?.kind.as_str();
        let required = matches!(
            kind,
            "field_declaration"
                | "type_spec"
                | "type_alias"
                | "parameter_declaration"
                | "variadic_parameter_declaration"
        );
        if !required && !matches!(kind, "const_spec" | "var_spec") {
            return Ok(TsDeclaredType::NotApplicable);
        }
        let Some(index) = self.parsed.field(declaration, "type")? else {
            if required {
                self.issue(target, TsIssue::MissingDeclaredType, None);
                return Ok(TsDeclaredType::Unavailable);
            }
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
        Ok(TsDeclaredType::Native(self.go_native(index)?))
    }

    pub(super) fn go_value(
        &mut self,
        target: Target<'doc>,
        declaration: usize,
        subject: usize,
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
        let nodes = &self.parsed.context.observations()?.nodes;
        let names = self.go_names(declaration)?;
        let ordinal = names.iter().position(|&i| i == subject).ok_or(
            TreeSitterModelError::NativeContract("Go value subject differs from declaration"),
        )?;
        let initializer_index = self.parsed.field(declaration, "value")?;
        let expression_indices: Vec<_> = match initializer_index {
            Some(i) if self.parsed.node(i)?.kind == "expression_list" => self.parsed.children[i]
                .iter()
                .copied()
                .filter(|&c| nodes[c].named && nodes[c].kind != "comment")
                .collect(),
            Some(i) => vec![i],
            None => Vec::new(),
        };
        let mut captures = Vec::new();
        for &capture_index in slots {
            self.parsed.original.check()?;
            let capture =
                matched
                    .captures
                    .get(capture_index)
                    .ok_or(TreeSitterModelError::NativeContract(
                        "value references foreign capture",
                    ))?;
            let index = self.parsed.captured_node(capture)?;
            let binding =
                if let Some(value_ordinal) = expression_indices.iter().position(|&i| i == index) {
                    if names.len() != expression_indices.len() {
                        TsValueBinding::UnprovenArity {
                            subjects: names.len(),
                            expressions: expression_indices.len(),
                        }
                    } else if ordinal == value_ordinal {
                        TsValueBinding::Subject { ordinal }
                    } else {
                        TsValueBinding::OtherSubject {
                            ordinal: value_ordinal,
                        }
                    }
                } else if initializer_index == Some(index) {
                    TsValueBinding::Initializer
                } else if let Some(value_ordinal) = expression_indices
                    .iter()
                    .copied()
                    .map(|expression| self.native_ancestor(expression, index))
                    .collect::<Result<Vec<_>, _>>()?
                    .iter()
                    .position(|&inside| inside)
                {
                    TsValueBinding::ExpressionPart {
                        ordinal: value_ordinal,
                    }
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
                syntax: self.go_native(index)?,
                binding,
            });
        }
        let expressions = expression_indices
            .into_iter()
            .map(|i| self.go_native(i))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(TsDeclaredValue::Native {
            subject_ordinal: ordinal,
            subject_count: names.len(),
            initializer: initializer_index
                .map(|i| self.go_native(i))
                .transpose()?
                .map(Box::new),
            expressions,
            captures,
        })
    }

    pub(super) fn go_explicit_parent(
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
        let query = self.parsed.context.observations()?.groups[target.occurrence.query_index]
            .query
            .value();
        let label = query.parent_label().value();
        for &capture_index in slots {
            self.parsed.original.check()?;
            let capture =
                matched
                    .captures
                    .get(capture_index)
                    .ok_or(TreeSitterModelError::NativeContract(
                        "parent references foreign capture",
                    ))?;
            let index = self.parsed.captured_node(capture)?;
            let node = self.parsed.node(index)?;
            let owner = if node.field.as_deref() == Some("name")
                || (node.kind == "identifier"
                    && matches!(node.parent, Some(p) if self.parsed.node(p)?.kind == "const_spec"))
            {
                node.parent.unwrap_or(index)
            } else {
                index
            };
            if label.is_empty()
                || owner == declaration
                || !self.native_ancestor(owner, declaration)?
            {
                self.issue(
                    target,
                    TsIssue::InvalidExplicitParent { capture_index },
                    Some(&capture.original),
                );
                continue;
            }
            let names = self.go_names(owner)?;
            let selected = if owner != index && names.contains(&index) {
                Some(index)
            } else if names.len() == 1 {
                Some(names[0])
            } else {
                None
            };
            let Some(selected) = selected else {
                self.issue(
                    target,
                    TsIssue::InvalidExplicitParent { capture_index },
                    Some(&capture.original),
                );
                continue;
            };
            let origin = self.parsed.origin(selected)?;
            if recovered(origin.recovery) || exact_range(&origin.original).is_none() {
                self.issue(
                    target,
                    TsIssue::InvalidExplicitParent { capture_index },
                    Some(&origin.original),
                );
                continue;
            }
            let anchor = exact_range(&origin.original).ok_or(
                TreeSitterModelError::NativeContract("checked parent origin disappeared"),
            )?;
            let extent = self.extent(target, owner, "parent")?;
            if let ExtentObservation::Exact(e) = &extent
                && (anchor.start() < e.range().start()
                    || anchor.end() > e.range().end()
                    || seed.raw.start() < e.range().start()
                    || seed.raw.end() > e.range().end())
            {
                self.issue(
                    target,
                    TsIssue::InvalidExplicitParent { capture_index },
                    Some(&capture.original),
                );
                continue;
            }
            if let Some(existing) = seed.namespaces.iter_mut().find(|n| n.node_index == owner) {
                if existing.label != *label || !same_range(existing.anchor, anchor) {
                    self.issue(
                        target,
                        TsIssue::InvalidExplicitParent { capture_index },
                        Some(&capture.original),
                    );
                } else {
                    existing.explicit_parent_capture_index = Some(capture_index);
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
                    name: anchor.text().to_owned(),
                    anchor,
                    extent,
                    selection: TsOwnerSelection::ConfiguredIdentifier,
                },
            );
        }
        let mut owners = Vec::new();
        for frame in &seed.namespaces {
            self.parsed.original.check()?;
            let references: Vec<_> = owners.iter().collect();
            owners.push(self.parsed.identity.symbol(
                &frame.label,
                frame.anchor,
                &references,
                LogicalDeclaration::named(&frame.name),
            )?);
        }
        let references: Vec<_> = owners.iter().collect();
        seed.identity = self.parsed.identity.symbol(
            target.label,
            seed.raw,
            &references,
            LogicalDeclaration::named(&seed.name),
        )?;
        Ok(())
    }

    pub(super) fn go_declaration_details(
        &self,
        declaration: usize,
        subject: usize,
    ) -> Result<TsGoDeclarationDetails<'doc, 'o, 'v>, TreeSitterModelError> {
        let node = self.parsed.node(declaration)?;
        let alias = if node.kind == "import_spec" {
            self.parsed
                .field(declaration, "name")?
                .map(|i| self.go_native(i))
                .transpose()?
        } else {
            None
        };
        let attachment_owner = match node.parent {
            Some(p)
                if matches!(
                    self.parsed.node(p)?.kind.as_str(),
                    "type_declaration" | "const_declaration" | "var_declaration"
                ) =>
            {
                p
            }
            _ => declaration,
        };
        let mut issues = Vec::new();
        let attachments = self.go_attachments(attachment_owner, &mut issues)?;
        let policy = self
            .parsed
            .context
            .observations()?
            .profile
            .exports()
            .value();
        let export = match policy.strategy().value() {
            ExportStrategy::None => TsExport::NotConfigured,
            ExportStrategy::CapitalizedName => {
                static CAPITALIZED: LazyLock<regex::Regex> = LazyLock::new(|| {
                    regex::Regex::new(r"^\p{Lu}").expect("constant Unicode category expression")
                });
                let package = self
                    .parsed
                    .node(attachment_owner)?
                    .parent
                    .map(|p| self.parsed.node(p).map(|n| n.kind == "source_file"))
                    .transpose()?
                    .unwrap_or(false);
                let eligible = node.kind == "field_declaration"
                    || (package
                        && matches!(
                            node.kind.as_str(),
                            "type_spec" | "type_alias" | "const_spec" | "var_spec"
                        ));
                TsExport::CapitalizedName(
                    eligible
                        && self.parsed.origin(subject)?.prepared.text() != "_"
                        && exact_range(&self.parsed.origin(subject)?.original)
                            .is_some_and(|r| CAPITALIZED.is_match(r.text())),
                )
            }
            _ => {
                issues.push(TsCallableIssue {
                    node_index: declaration,
                    original: None,
                    kind: TsCallableIssueKind::UnsupportedExportPolicy,
                });
                TsExport::Unavailable
            }
        };
        Ok(TsGoDeclarationDetails {
            native_kind: node.kind.as_str(),
            alias,
            export,
            attachment_owner_node_index: attachment_owner,
            attachments,
            issues,
        })
    }

    pub(super) fn go_native(
        &self,
        index: usize,
    ) -> Result<TsNativeSyntax<'doc, 'o, 'v>, TreeSitterModelError> {
        let origin = self.parsed.origin(index)?;
        Ok(TsNativeSyntax {
            node_index: index,
            kind: self.parsed.node(index)?.kind.as_str(),
            prepared: origin.prepared,
            original: &origin.original,
            raw: exact_range(&origin.original),
        })
    }

    fn go_signature_field(
        &self,
        declaration: usize,
        role: &'static str,
        required: bool,
        issues: &mut Vec<TsCallableIssue<'doc, 'o>>,
    ) -> Result<Option<TsNativeSyntax<'doc, 'o, 'v>>, TreeSitterModelError> {
        let Some(index) = self.parsed.field(declaration, role)? else {
            if required {
                issues.push(TsCallableIssue {
                    node_index: declaration,
                    original: None,
                    kind: TsCallableIssueKind::MissingField { role },
                });
            }
            return Ok(None);
        };
        let observation = self.go_native(index)?;
        let origin = self.parsed.origin(index)?;
        if recovered(origin.recovery) {
            issues.push(TsCallableIssue {
                node_index: index,
                original: Some(observation.original),
                kind: TsCallableIssueKind::Recovery { role },
            });
        }
        if observation.raw.is_none() {
            issues.push(TsCallableIssue {
                node_index: index,
                original: Some(observation.original),
                kind: TsCallableIssueKind::NonExact { role },
            });
        }
        Ok(Some(observation))
    }

    pub(super) fn go_function_details(
        &self,
        declaration: usize,
        kind: TsCallableKind,
    ) -> Result<TsFunctionDetails<'doc, 'o, 'v>, TreeSitterModelError> {
        let header = self.parsed.signatures.get(&declaration).ok_or(
            TreeSitterModelError::NativeContract("native Go callable lacks signature boundaries"),
        )?;
        let body = self.parsed.field(declaration, "body")?;
        let mut issues = Vec::new();
        if recovered(header.recovery) {
            issues.push(TsCallableIssue {
                node_index: declaration,
                original: Some(&header.original),
                kind: TsCallableIssueKind::Recovery { role: "signature" },
            });
        }
        if exact_range(&header.original).is_none() {
            issues.push(TsCallableIssue {
                node_index: declaration,
                original: Some(&header.original),
                kind: TsCallableIssueKind::NonExact { role: "signature" },
            });
        }
        let native = TsGoSignature {
            receiver: self.go_signature_field(
                declaration,
                "receiver",
                kind == TsCallableKind::MethodGo,
                &mut issues,
            )?,
            type_parameters: self.go_signature_field(
                declaration,
                "type_parameters",
                false,
                &mut issues,
            )?,
            parameters: self.go_signature_field(declaration, "parameters", true, &mut issues)?,
            result: self.go_signature_field(declaration, "result", false, &mut issues)?,
        };
        let attachments = self.go_attachments(declaration, &mut issues)?;
        let complexity = self.complexity(declaration, body, &mut issues)?;
        let policy = self
            .parsed
            .context
            .observations()?
            .profile
            .exports()
            .value();
        let export = if *policy.strategy().value() != ExportStrategy::None {
            if *policy.strategy().value() == ExportStrategy::CapitalizedName {
                let name = self.parsed.field(declaration, "name")?.ok_or(
                    TreeSitterModelError::NativeContract("Go callable lacks native name"),
                )?;
                static CAPITALIZED: LazyLock<regex::Regex> = LazyLock::new(|| {
                    regex::Regex::new(r"^\p{Lu}").expect("constant Unicode category expression")
                });
                let package_scope = self
                    .parsed
                    .node(declaration)?
                    .parent
                    .map(|p| self.parsed.node(p).map(|n| n.kind == "source_file"))
                    .transpose()?
                    .unwrap_or(false);
                let exported = exact_range(&self.parsed.origin(name)?.original)
                    .is_some_and(|r| CAPITALIZED.is_match(r.text()))
                    && (package_scope || kind == TsCallableKind::MethodGo);
                TsExport::CapitalizedName(exported)
            } else {
                issues.push(TsCallableIssue {
                    node_index: declaration,
                    original: None,
                    kind: TsCallableIssueKind::UnsupportedExportPolicy,
                });
                TsExport::Unavailable
            }
        } else {
            TsExport::NotConfigured
        };
        Ok(TsFunctionDetails {
            declaration_node_index: declaration,
            kind,
            body_node_index: body,
            defaultness_node_index: None,
            signature: TsSignature {
                prepared: header.prepared,
                original: &header.original,
                raw: exact_range(&header.original),
                syntax: None,
                go: Some(native),
                ecma: None,
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

    fn go_attachments(
        &self,
        declaration: usize,
        issues: &mut Vec<TsCallableIssue<'doc, 'o>>,
    ) -> Result<Vec<TsAttachment<'doc, 'o>>, TreeSitterModelError> {
        let Some(parent) = self.parsed.node(declaration)?.parent else {
            return Ok(Vec::new());
        };
        let siblings = &self.parsed.children[parent];
        let at = siblings.iter().position(|&i| i == declaration).ok_or(
            TreeSitterModelError::NativeContract("Go callable absent from native parent"),
        )?;
        let mut attachments = Vec::new();
        let mut next = declaration;
        for position in (0..at).rev() {
            self.parsed.original.check()?;
            let index = siblings[position];
            let node = self.parsed.node(index)?;
            if node.kind != "comment" || !self.configured_comment(index)? {
                break;
            }
            let next_node = self.parsed.node(next)?;
            let gap = self
                .parsed
                .prepared_text
                .get(node.range.end..next_node.range.start)
                .ok_or(TreeSitterModelError::NativeContract(
                    "Go native comment gap is inconsistent",
                ))?;
            if !go_comment_gap(gap) {
                break;
            }
            if position > 0 {
                let previous_index = siblings[position - 1];
                let previous = self.parsed.node(previous_index)?;
                if previous.kind != "comment" {
                    if previous.end.row == node.start.row {
                        break;
                    }
                    let predecessor = self.parsed.origin(previous_index)?;
                    let original_separation = match (
                        exact_range(&predecessor.original),
                        exact_range(&self.parsed.origin(index)?.original),
                    ) {
                        (Some(a), Some(b)) if a.end() <= b.start() => {
                            let gap = self.parsed.source.range(a.end(), b.start())?.text();
                            gap.chars().all(char::is_whitespace) && gap.contains('\n')
                        }
                        _ => false,
                    };
                    if !original_separation {
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
            let valid_gap = match (raw, exact_range(&self.parsed.origin(next)?.original)) {
                (Some(a), Some(b)) if a.end() <= b.start() => {
                    go_comment_gap(self.parsed.source.range(a.end(), b.start())?.text())
                }
                _ => false,
            };
            let content = if !valid_gap || recovered(origin.recovery) {
                issues.push(TsCallableIssue {
                    node_index: index,
                    original: Some(&origin.original),
                    kind: if recovered(origin.recovery) {
                        TsCallableIssueKind::Recovery { role: "comment" }
                    } else {
                        TsCallableIssueKind::InvalidAttachmentOrigin
                    },
                });
                TsAttachmentContent::Unavailable
            } else {
                TsAttachmentContent::Comment
            };
            attachments.push(TsAttachment {
                node_index: index,
                original: &origin.original,
                raw,
                placement: TsAttachmentPlacement::Outer,
                content,
            });
            next = index;
        }
        attachments.reverse();
        Ok(attachments)
    }
    fn go_scope(&self, declaration: usize) -> Result<usize, TreeSitterModelError> {
        let kind = self.parsed.node(declaration)?.kind.as_str();
        let signature = matches!(
            kind,
            "parameter_declaration"
                | "variadic_parameter_declaration"
                | "type_parameter_declaration"
        );
        let mut cursor = self.parsed.node(declaration)?.parent;
        while let Some(index) = cursor {
            self.parsed.original.check()?;
            let node = self.parsed.node(index)?;
            if (signature
                && matches!(
                    node.kind.as_str(),
                    "function_declaration"
                        | "method_declaration"
                        | "func_literal"
                        | "function_type"
                        | "method_elem"
                        | "type_spec"
                        | "type_alias"
                ))
                || matches!(
                    node.kind.as_str(),
                    "block"
                        | "source_file"
                        | "communication_case"
                        | "expression_case"
                        | "type_case"
                        | "default_case"
                        | "for_statement"
                        | "if_statement"
                        | "expression_switch_statement"
                        | "type_switch_statement"
                )
            {
                return Ok(index);
            }
            cursor = node.parent;
        }
        Err(TreeSitterModelError::NativeContract(
            "Go declaration lacks native lexical scope",
        ))
    }

    fn go_bindings(&self) -> Result<HashMap<&'doc str, Vec<GoBinding>>, TreeSitterModelError> {
        let nodes = &self.parsed.context.observations()?.nodes;
        let mut bindings: HashMap<&'doc str, Vec<GoBinding>> = HashMap::new();
        for (subject, node) in nodes.iter().enumerate() {
            self.parsed.original.check()?;
            if !matches!(
                node.kind.as_str(),
                "identifier" | "type_identifier" | "package_identifier"
            ) {
                continue;
            }
            let Some(parent) = node.parent else {
                continue;
            };
            let type_switch = if nodes[parent].kind == "expression_list"
                && nodes[parent].field.as_deref() == Some("alias")
            {
                nodes[parent]
                    .parent
                    .filter(|&i| nodes[i].kind == "type_switch_statement")
            } else {
                None
            };
            let declaration = if let Some(switch) = type_switch {
                switch
            } else if (node.field.as_deref() == Some("name")
                || (nodes[parent].kind == "const_spec" && node.kind == "identifier"))
                && matches!(
                    nodes[parent].kind.as_str(),
                    "function_declaration"
                        | "type_spec"
                        | "type_alias"
                        | "const_spec"
                        | "var_spec"
                        | "parameter_declaration"
                        | "variadic_parameter_declaration"
                        | "type_parameter_declaration"
                        | "import_spec"
                )
            {
                parent
            } else if nodes[parent].kind == "expression_list"
                && nodes[parent].field.as_deref() == Some("left")
            {
                let Some(declaration) = nodes[parent].parent else {
                    continue;
                };
                if !matches!(
                    nodes[declaration].kind.as_str(),
                    "short_var_declaration" | "range_clause" | "receive_statement"
                ) {
                    continue;
                }
                declaration
            } else {
                continue;
            };
            let name = match self
                .parsed
                .origins
                .get(&subject)
                .and_then(|o| exact_range(&o.original))
            {
                Some(raw) => raw.text(),
                None => self.parsed.prepared_text.get(node.range.clone()).ok_or(
                    TreeSitterModelError::NativeContract("Go binding range differs from source"),
                )?,
            };
            if name == "_" {
                continue;
            }
            let scopes = match type_switch {
                Some(switch) => self.parsed.children[switch]
                    .iter()
                    .copied()
                    .filter(|&i| matches!(nodes[i].kind.as_str(), "type_case" | "default_case"))
                    .collect::<Vec<_>>(),
                None => vec![self.go_scope(declaration)?],
            };
            for scope in scopes {
                let mut cursor = Some(scope);
                let mut depth = 0;
                while let Some(index) = cursor {
                    self.parsed.original.check()?;
                    depth += 1;
                    cursor = nodes[index].parent;
                }
                let signature = matches!(
                    nodes[declaration].kind.as_str(),
                    "parameter_declaration"
                        | "variadic_parameter_declaration"
                        | "type_parameter_declaration"
                );
                let visible_after = if type_switch.is_some() {
                    self.parsed.children[scope]
                        .iter()
                        .copied()
                        .find(|&i| nodes[i].kind == ":")
                        .map(|i| nodes[i].range.end)
                        .ok_or(TreeSitterModelError::NativeContract(
                            "Go type-switch clause lacks colon",
                        ))?
                } else if nodes[scope].kind == "source_file"
                    || nodes[declaration].kind == "type_parameter_declaration"
                {
                    0
                } else if signature {
                    self.parsed
                        .field(scope, "body")?
                        .map_or(nodes[scope].range.end, |body| nodes[body].range.start)
                } else if matches!(nodes[declaration].kind.as_str(), "type_spec" | "type_alias") {
                    nodes[subject].range.end
                } else {
                    nodes[declaration].range.end
                };
                bindings.entry(name).or_default().push(GoBinding {
                    declaration,
                    subject,
                    scope,
                    depth,
                    visible_after,
                    exact: self.parsed.origins.get(&subject).is_some_and(|o| {
                        exact_range(&o.original).is_some() && !recovered(o.recovery)
                    }),
                });
            }
        }
        Ok(bindings)
    }

    fn go_visible_bindings(
        &self,
        bindings: &HashMap<&str, Vec<GoBinding>>,
        name: &str,
        site: usize,
    ) -> Result<Vec<GoBinding>, TreeSitterModelError> {
        let mut selected = Vec::new();
        let mut best = None;
        for binding in bindings.get(name).into_iter().flatten() {
            self.parsed.original.check()?;
            if !self.native_ancestor(binding.scope, site)?
                || binding.visible_after > self.parsed.node(site)?.range.start
            {
                continue;
            }
            let rank = (binding.depth, binding.visible_after);
            if best.is_none_or(|b| rank > b) {
                selected.clear();
                best = Some(rank);
            }
            if best == Some(rank) {
                selected.push(*binding);
            }
        }
        Ok(selected)
    }

    fn go_entity_candidates(
        &self,
        declarations: &[usize],
        name: &str,
        relation: &TsRelationObservation<'_, '_, '_, '_>,
    ) -> Result<Vec<usize>, TreeSitterModelError> {
        let Some(rule) = relation.target_rule else {
            return Ok(Vec::new());
        };
        let query_labels = relation.query.value().target_labels().value();
        let mut candidates = Vec::new();
        for (index, entity) in self.entities.iter().enumerate() {
            self.parsed.original.check()?;
            if entity.name != name {
                continue;
            }
            if !rule
                .value()
                .labels()
                .value()
                .iter()
                .any(|label| label.value() == entity.identity.label())
                || (!query_labels.is_empty()
                    && !query_labels
                        .iter()
                        .any(|label| label.value() == entity.identity.label()))
            {
                continue;
            }
            if entity
                .contributions
                .iter()
                .any(|c| declarations.contains(&c.declaration_node_index))
            {
                candidates.push(index);
            }
        }
        Ok(candidates)
    }

    fn go_type_name(&self, mut index: usize) -> Result<Option<usize>, TreeSitterModelError> {
        loop {
            self.parsed.original.check()?;
            let node = self.parsed.node(index)?;
            match node.kind.as_str() {
                "type_identifier" => return Ok(Some(index)),
                "pointer_type" | "parenthesized_type" | "generic_type" => {
                    let nodes = &self.parsed.context.observations()?.nodes;
                    let children: Vec<_> = self.parsed.children[index]
                        .iter()
                        .copied()
                        .filter(|&c| {
                            nodes[c].named
                                && !matches!(nodes[c].kind.as_str(), "comment" | "type_arguments")
                        })
                        .collect();
                    if children.len() != 1 {
                        return Ok(None);
                    }
                    index = children[0];
                }
                _ => return Ok(None),
            }
        }
    }

    fn go_named_type(
        &self,
        bindings: &HashMap<&str, Vec<GoBinding>>,
        type_index: usize,
    ) -> Result<Option<usize>, TreeSitterModelError> {
        let Some(name) = self.go_type_name(type_index)? else {
            return Ok(None);
        };
        let Some(raw) = self
            .parsed
            .origins
            .get(&name)
            .and_then(|o| exact_range(&o.original))
        else {
            return Ok(None);
        };
        let choices = self.go_visible_bindings(bindings, raw.text(), name)?;
        if choices.len() != 1
            || !choices[0].exact
            || !matches!(
                self.parsed.node(choices[0].declaration)?.kind.as_str(),
                "type_spec" | "type_alias"
            )
        {
            return Ok(None);
        }
        Ok(Some(choices[0].declaration))
    }

    fn go_type_target(
        &self,
        bindings: &HashMap<&str, Vec<GoBinding>>,
        mut declaration: usize,
        only_alias: bool,
    ) -> Result<Option<usize>, TreeSitterModelError> {
        let mut seen = HashSet::new();
        while seen.insert(declaration) {
            self.parsed.original.check()?;
            let node = self.parsed.node(declaration)?;
            if only_alias && node.kind != "type_alias" {
                return Ok(Some(declaration));
            }
            let Some(ty) = self.parsed.field(declaration, "type")? else {
                return Ok(None);
            };
            if matches!(
                self.parsed.node(ty)?.kind.as_str(),
                "struct_type" | "interface_type"
            ) {
                return Ok(Some(ty));
            }
            let Some(next) = self.go_named_type(bindings, ty)? else {
                return Ok(None);
            };
            declaration = next;
        }
        Ok(None)
    }

    fn go_is_write_only(&self, selector: usize) -> Result<bool, TreeSitterModelError> {
        let Some(list) = self.parsed.node(selector)?.parent else {
            return Ok(false);
        };
        let node = self.parsed.node(list)?;
        let Some(statement) = node.parent else {
            return Ok(false);
        };
        if node.kind != "expression_list"
            || node.field.as_deref() != Some("left")
            || self.parsed.node(statement)?.kind != "assignment_statement"
        {
            return Ok(false);
        }
        for &child in &self.parsed.children[statement] {
            self.parsed.original.check()?;
            if self.parsed.node(child)?.kind == "=" {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn go_local_relations(
        &self,
        relations: &[TsRelationObservation<'doc, 'profile, 'o, 'v>],
    ) -> Result<Vec<TsLocalRelationResolution>, TreeSitterModelError> {
        let bindings = self.go_bindings()?;
        let mut results = Vec::new();
        for (relation_index, relation) in relations.iter().enumerate() {
            for (source_index, source) in relation.sources.iter().enumerate() {
                self.parsed.original.check()?;
                let mut evidence = vec![source.node_index];
                let target = match source.state {
                    TsRelationSourceState::MissingContainer => TsLocalTargetState::Unresolved {
                        reason: TsLocalUnresolvedReason::MissingSource,
                    },
                    TsRelationSourceState::UnavailableContainer {
                        container_node_index,
                    } => TsLocalTargetState::Unresolved {
                        reason: if self.parsed.node(container_node_index)?.kind == "func_literal" {
                            TsLocalUnresolvedReason::AnonymousSource {
                                node_index: container_node_index,
                            }
                        } else {
                            TsLocalUnresolvedReason::UnavailableSource {
                                node_index: container_node_index,
                            }
                        },
                    },
                    TsRelationSourceState::Bound {
                        container_node_index,
                        ref entity_indices,
                    } => {
                        evidence.push(container_node_index);
                        if recovered(self.parsed.origin(container_node_index)?.recovery)
                            || entity_indices.iter().any(|&i| {
                                !self.entities[i].diagnostic_indices.is_empty()
                                    || !self.entities[i].conflicts.is_empty()
                                    || !self.entities[i].model_conflicts.is_empty()
                            })
                        {
                            TsLocalTargetState::Unresolved {
                                reason: TsLocalUnresolvedReason::UnavailableSource {
                                    node_index: container_node_index,
                                },
                            }
                        } else {
                            self.go_local_target(
                                &bindings,
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
        Ok(results)
    }

    fn go_local_target(
        &self,
        bindings: &HashMap<&str, Vec<GoBinding>>,
        relation: &TsRelationObservation<'_, '_, '_, '_>,
        subject: usize,
        evidence: &mut Vec<usize>,
    ) -> Result<TsLocalTargetState, TreeSitterModelError> {
        let unresolved = |reason| TsLocalTargetState::Unresolved { reason };
        let nodes = &self.parsed.context.observations()?.nodes;
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
        let Some(name) = exact_range(&origin.original) else {
            return Ok(unresolved(TsLocalUnresolvedReason::NonExact {
                node_index: subject,
            }));
        };
        if self
            .name_reject
            .get(&relation.query_index)
            .is_some_and(|r| r.is_match(name.text()))
        {
            return Ok(unresolved(TsLocalUnresolvedReason::ExcludedName));
        }
        let node = self.parsed.node(subject)?;
        let Some(parent) = node.parent else {
            return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                node_index: subject,
            }));
        };
        let kind = query.relation_type().value().as_str();
        if node.kind == "identifier"
            && self.parsed.node(parent)?.kind == "call_expression"
            && kind == "CALLS"
        {
            let visible = self.go_visible_bindings(bindings, name.text(), subject)?;
            if let Some(binding) = visible
                .iter()
                .find(|b| !b.exact || nodes[b.declaration].kind != "function_declaration")
            {
                evidence.push(binding.declaration);
                return Ok(unresolved(
                    if self.parsed.node(binding.declaration)?.kind == "import_spec" {
                        TsLocalUnresolvedReason::ExternalImport {
                            declaration_node_index: binding.declaration,
                        }
                    } else {
                        TsLocalUnresolvedReason::Shadowed {
                            declaration_node_index: binding.declaration,
                        }
                    },
                ));
            }
            let declarations: Vec<_> = visible.iter().map(|b| b.declaration).collect();
            evidence.extend(&declarations);
            return Ok(go_local_state(self.go_entity_candidates(
                &declarations,
                name.text(),
                relation,
            )?));
        }
        if node.kind != "field_identifier"
            || self.parsed.node(parent)?.kind != "selector_expression"
        {
            return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedSyntax {
                node_index: subject,
            }));
        }
        if kind == "READS_FIELD" && self.go_is_write_only(parent)? {
            return Ok(unresolved(TsLocalUnresolvedReason::NotReadAccess {
                node_index: parent,
            }));
        }
        if !matches!(kind, "CALLS" | "READS_FIELD" | "WRITES_FIELD") {
            return Ok(unresolved(TsLocalUnresolvedReason::UnsupportedPolicy));
        }
        let Some(operand) = self.parsed.field(parent, "operand")? else {
            return Ok(unresolved(TsLocalUnresolvedReason::UnknownReceiver {
                node_index: parent,
            }));
        };
        evidence.push(operand);
        if self.parsed.node(operand)?.kind != "identifier" {
            return Ok(unresolved(TsLocalUnresolvedReason::UnknownReceiver {
                node_index: operand,
            }));
        }
        let Some(raw) = self
            .parsed
            .origins
            .get(&operand)
            .and_then(|o| exact_range(&o.original))
        else {
            return Ok(unresolved(TsLocalUnresolvedReason::NonExact {
                node_index: operand,
            }));
        };
        let visible = self.go_visible_bindings(bindings, raw.text(), subject)?;
        if visible.len() != 1 || !visible[0].exact {
            return Ok(unresolved(TsLocalUnresolvedReason::UnknownReceiver {
                node_index: operand,
            }));
        }
        let binding = visible[0];
        evidence.extend([binding.subject, binding.declaration]);
        if self.parsed.node(binding.declaration)?.kind == "import_spec" {
            return Ok(unresolved(TsLocalUnresolvedReason::ExternalImport {
                declaration_node_index: binding.declaration,
            }));
        }
        let Some(type_index) = self.parsed.field(binding.declaration, "type")? else {
            return Ok(unresolved(TsLocalUnresolvedReason::UnknownType {
                node_index: binding.declaration,
            }));
        };
        evidence.push(type_index);
        let nominal = self.go_named_type(bindings, type_index)?;
        let structure = if matches!(
            self.parsed.node(type_index)?.kind.as_str(),
            "struct_type" | "interface_type"
        ) {
            Some(type_index)
        } else {
            match nominal {
                Some(n) => self.go_type_target(bindings, n, false)?,
                None => None,
            }
        };
        if let Some(index) = structure {
            evidence.push(index);
            if self.parsed.node(index)?.kind == "interface_type" {
                return Ok(unresolved(TsLocalUnresolvedReason::DynamicReceiver {
                    node_index: index,
                }));
            }
        }
        let mut declarations = Vec::new();
        let mut promoted = None;
        if let Some(structure) = structure {
            for (index, node) in self.parsed.context.observations()?.nodes.iter().enumerate() {
                self.parsed.original.check()?;
                if node.kind != "field_declaration" || !self.native_ancestor(structure, index)? {
                    continue;
                }
                let Some(list) = node.parent else {
                    continue;
                };
                if self.parsed.node(list)?.parent != Some(structure) {
                    continue;
                }
                let names: Vec<_> = self.parsed.children[index]
                    .iter()
                    .copied()
                    .filter(|&i| nodes[i].field.as_deref() == Some("name"))
                    .collect();
                if names.is_empty() {
                    promoted = Some(index);
                    continue;
                }
                if kind != "CALLS"
                    && names.iter().any(|&i| {
                        self.parsed
                            .origins
                            .get(&i)
                            .and_then(|o| exact_range(&o.original))
                            .is_some_and(|r| r.text() == name.text())
                    })
                {
                    declarations.push(index);
                }
            }
        }
        if kind == "CALLS" {
            let Some(nominal) = nominal else {
                return Ok(unresolved(TsLocalUnresolvedReason::UnknownType {
                    node_index: type_index,
                }));
            };
            let Some(receiver_target) = self.go_type_target(bindings, nominal, true)? else {
                return Ok(unresolved(TsLocalUnresolvedReason::UnknownType {
                    node_index: type_index,
                }));
            };
            evidence.push(receiver_target);
            for entity in &self.entities {
                self.parsed.original.check()?;
                if entity.name != name.text() {
                    continue;
                }
                for contribution in &entity.contributions {
                    let declaration = contribution.declaration_node_index;
                    if self.parsed.node(declaration)?.kind != "method_declaration" {
                        continue;
                    }
                    let Some(receiver) = self.parsed.field(declaration, "receiver")? else {
                        continue;
                    };
                    for &parameter in &self.parsed.children[receiver] {
                        if self.parsed.node(parameter)?.kind != "parameter_declaration" {
                            continue;
                        }
                        let Some(ty) = self.parsed.field(parameter, "type")? else {
                            continue;
                        };
                        if let Some(nominal) = self.go_named_type(bindings, ty)?
                            && self.go_type_target(bindings, nominal, true)?
                                == Some(receiver_target)
                            && !declarations.contains(&declaration)
                        {
                            declarations.push(declaration);
                        }
                    }
                }
            }
        } else if structure.is_none() {
            return Ok(unresolved(TsLocalUnresolvedReason::UnknownType {
                node_index: type_index,
            }));
        }
        evidence.extend(&declarations);
        if declarations.is_empty()
            && let Some(node_index) = promoted
        {
            return Ok(unresolved(TsLocalUnresolvedReason::PromotedMember {
                node_index,
            }));
        }
        Ok(go_local_state(self.go_entity_candidates(
            &declarations,
            name.text(),
            relation,
        )?))
    }
}

fn go_comment_gap(gap: &str) -> bool {
    gap.chars().all(char::is_whitespace) && gap.bytes().filter(|&b| b == b'\n').count() == 1
}

fn go_local_state(mut candidates: Vec<usize>) -> TsLocalTargetState {
    match candidates.len() {
        0 => TsLocalTargetState::NotFound,
        1 => TsLocalTargetState::Resolved {
            entity_index: candidates.remove(0),
        },
        _ => TsLocalTargetState::Ambiguous {
            entity_indices: candidates,
        },
    }
}
