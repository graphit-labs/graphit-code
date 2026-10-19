use super::*;

pub(super) struct DerivedValueRow<'doc, 'o, 'v, 'request> {
    pub target: Target<'doc>,
    pub seed: EntitySeed<'doc, 'o, 'request>,
    pub contribution: TsContribution<'doc, 'o, 'v>,
}

impl<'doc, 'profile: 'o, 'o, 'v, 'request> Assembly<'doc, 'profile, 'o, 'v, 'request> {
    fn copy_value_namespace(
        &self,
        namespace: &TsNamespace<'doc, 'o>,
    ) -> Result<TsNamespace<'doc, 'o>, TreeSitterModelError> {
        self.parsed.original.check()?;
        let extent = match &namespace.extent {
            ExtentObservation::Missing => ExtentObservation::Missing,
            ExtentObservation::NonExact(origin) => ExtentObservation::NonExact(origin),
            ExtentObservation::Exact(extent) => ExtentObservation::Exact(EntityExtent::new(
                &self.parsed.source,
                extent.range(),
                self.parsed.original,
            )?),
        };
        Ok(TsNamespace {
            node_index: namespace.node_index,
            context_index: namespace.context_index,
            explicit_parent_capture_index: namespace.explicit_parent_capture_index,
            label: namespace.label.clone(),
            name: namespace.name.clone(),
            anchor: namespace.anchor,
            extent,
            selection: namespace.selection,
        })
    }

    pub(super) fn value_rows(
        &mut self,
        owner_target: Target<'doc>,
        owner: &EntitySeed<'doc, 'o, 'request>,
        contribution: &TsContribution<'doc, 'o, 'v>,
        matched: &'doc TreeSitterProfileMatch<'o, 'v>,
    ) -> Result<Vec<DerivedValueRow<'doc, 'o, 'v, 'request>>, TreeSitterModelError> {
        self.parsed.original.check()?;
        let label = self.parsed.context.observations()?.groups[owner_target.occurrence.query_index]
            .query
            .value()
            .value_label()
            .value()
            .as_str();
        if label.is_empty() {
            return Ok(Vec::new());
        }
        let TsDeclaredValue::Native {
            subject_ordinal,
            subject_count,
            expressions,
            captures,
            ..
        } = &contribution.declared_value
        else {
            return Ok(Vec::new());
        };
        let mut rows = Vec::new();
        for value in captures {
            self.parsed.original.check()?;
            let index = value.syntax.node_index;
            let mut target = owner_target;
            target.label = label;
            target.declaration = Some(index);
            target.subject = Some(index);
            target.occurrence.name_capture_index = Some(value.capture_index);
            let proven = match value.binding {
                TsValueBinding::Subject { ordinal } => ordinal == *subject_ordinal,
                TsValueBinding::Initializer => *subject_count == 1,
                TsValueBinding::ExpressionPart { ordinal } => {
                    if *subject_count == expressions.len() && ordinal != *subject_ordinal {
                        continue;
                    }
                    *subject_count == expressions.len() && ordinal == *subject_ordinal
                }
                TsValueBinding::OtherSubject { .. } => continue,
                _ => false,
            };
            if !proven {
                self.issue(
                    target,
                    TsIssue::UnprovenValueOwner {
                        capture_index: value.capture_index,
                        binding: value.binding,
                    },
                    Some(value.syntax.original),
                );
                continue;
            }
            if !self.native_ancestor(contribution.declaration_node_index, index)? {
                self.issue(
                    target,
                    TsIssue::InvalidValueCapture {
                        capture_index: value.capture_index,
                    },
                    Some(value.syntax.original),
                );
                continue;
            }
            let origin = self.parsed.origin(index)?;
            let extent = self.extent(target, index, "value")?;
            let Some(raw) = exact_range(&origin.original).filter(|r| r.start() != r.end()) else {
                continue;
            };
            if recovered(origin.recovery) {
                continue;
            }
            let ExtentObservation::Exact(owner_extent) = &contribution.extent else {
                self.issue(
                    target,
                    TsIssue::UnavailableValueOwner {
                        capture_index: value.capture_index,
                    },
                    Some(value.syntax.original),
                );
                continue;
            };
            let selection = match owner.name_representation {
                TsNameRepresentation::Identifier => TsOwnerSelection::DerivedDeclarationIdentifier,
                TsNameRepresentation::LiteralSource { .. } => {
                    TsOwnerSelection::DerivedDeclarationLiteralSource
                }
            };
            let mut owner_symbols = Vec::new();
            let mut namespaces = Vec::new();
            for namespace in &contribution.namespaces {
                self.parsed.original.check()?;
                let references = owner_symbols.iter().collect::<Vec<_>>();
                owner_symbols.push(self.parsed.identity.symbol(
                    &namespace.label,
                    namespace.anchor,
                    &references,
                )?);
                namespaces.push(self.copy_value_namespace(namespace)?);
            }
            let references = owner_symbols.iter().collect::<Vec<_>>();
            let parent = self
                .parsed
                .identity
                .symbol(owner_target.label, owner.raw, &references)?;
            if parent.uid().as_str() != owner.identity.uid().as_str() {
                return Err(TreeSitterModelError::NativeContract(
                    "derived value owner differs from actual declaration",
                ));
            }
            owner_symbols.push(parent);
            let references = owner_symbols.iter().collect::<Vec<_>>();
            let identity = self.parsed.identity.symbol(label, raw, &references)?;
            namespaces.push(TsNamespace {
                node_index: contribution.declaration_node_index,
                context_index: None,
                explicit_parent_capture_index: None,
                label: owner_target.label.to_owned(),
                name: owner.name.clone(),
                anchor: owner.raw,
                extent: ExtentObservation::Exact(EntityExtent::new(
                    &self.parsed.source,
                    owner_extent.range(),
                    self.parsed.original,
                )?),
                selection,
            });
            let capture = matched.captures.get(value.capture_index).ok_or(
                TreeSitterModelError::NativeContract("derived value references foreign capture"),
            )?;
            let contribution_namespaces = namespaces
                .iter()
                .map(|n| self.copy_value_namespace(n))
                .collect::<Result<Vec<_>, _>>()?;
            rows.push(DerivedValueRow {
                target,
                seed: EntitySeed {
                    identity,
                    name: raw.text().to_owned(),
                    raw,
                    name_representation: TsNameRepresentation::LiteralSource {
                        native_kind: value.syntax.kind,
                    },
                    namespaces,
                    excluded_context_indices: contribution.excluded_context_indices.clone(),
                },
                contribution: TsContribution {
                    occurrence: target.occurrence,
                    declaration_node_index: index,
                    container_node_index: Some(contribution.declaration_node_index),
                    name_capture: capture,
                    span_capture_indices: vec![value.capture_index],
                    extent,
                    declared_type: TsDeclaredType::NotApplicable,
                    callable: TsCallable::NotApplicable,
                    declared_value: TsDeclaredValue::NotConfigured,
                    derived_value: Some(TsDerivedValueSubject {
                        owner_uid: owner.identity.uid().as_str().to_owned(),
                        owner_label: owner_target.label,
                        owner_declaration_node_index: contribution.declaration_node_index,
                        owner_name_node_index: owner_target.subject.ok_or(
                            TreeSitterModelError::NativeContract(
                                "derived value owner lacks subject",
                            ),
                        )?,
                        owner_name_capture_index: owner_target
                            .occurrence
                            .name_capture_index
                            .ok_or(TreeSitterModelError::NativeContract(
                                "derived value owner lacks name capture",
                            ))?,
                        value_capture_index: value.capture_index,
                        binding: value.binding,
                    }),
                    go_declaration: None,
                    ecma_declaration: None,
                    python_declaration: None,
                    excluded_context_indices: contribution.excluded_context_indices.clone(),
                    namespaces: contribution_namespaces,
                },
            });
        }
        self.parsed.original.check()?;
        Ok(rows)
    }
}
