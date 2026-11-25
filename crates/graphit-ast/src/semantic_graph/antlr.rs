use super::*;
use crate::antlr_engine::{
    model::{
        AntlrContribution, AntlrEntityExtent, AntlrNativeSyntax, AntlrOracleApplication,
        AntlrOracleApplicationBinding, AntlrOracleCallIssue, AntlrOracleCallOutcome,
        AntlrOracleCallReason, AntlrOracleCallSourceState, AntlrOracleCallTargetState,
        AntlrOracleDeclaration, AntlrOracleExpression, AntlrOracleKeyword, AntlrOracleLookup,
        AntlrQualifiedName, AntlrSemanticDocument, AntlrTypeObservation, AntlrValueObservation,
    },
    semantic::{CaptureBinding, CapturedNode, SemanticOccurrence},
};

pub struct AstGraphAntlrInput<'a, 'doc, 'profile, 'o, 'v, 'request> {
    pub document: &'a AntlrSemanticDocument<'doc, 'profile, 'o, 'v, 'request>,
    pub source: NativeNnSource<'a>,
}
fn node_json(node: &CapturedNode<'_, '_>) -> Result<Value, AstGraphError> {
    Ok(
        json!({"node_index":node.node_index,"prepared":node.prepared.map(range_json).transpose()?,"original":node.original.as_ref().map(origin_json).transpose()?}),
    )
}
fn binding_json(binding: &CaptureBinding<'_, '_>) -> Result<Value, AstGraphError> {
    Ok(match binding {
        CaptureBinding::NotConfigured => json!({"status":"NotConfigured"}),
        CaptureBinding::Missing => json!({"status":"Missing"}),
        CaptureBinding::Present(node) => json!({"status":"Present","capture":node_json(node)?}),
    })
}

fn application_native_json(
    node: &AntlrNativeSyntax<'_, '_>,
    include_literal_source: bool,
) -> Result<Value, AstGraphError> {
    let mut value = json!({"node_index":node.node_index,"kind":node.node.kind,"class":format!("{:?}",node.node.class),
        "status":format!("{:?}",node.node.status),"error":node.node.error,"missing":node.node.missing,
        "original":node.original.map(origin_json).transpose()?});
    if include_literal_source {
        let literal_source = match node.original {
            Some(crate::prepared_input::OriginalProjection::Exact(range)) => Some(range.text()),
            _ => None,
        };
        value["literal_source"] = json!(literal_source);
    }
    Ok(value)
}
fn application_syntax_json(node: &AntlrNativeSyntax<'_, '_>) -> Result<Value, AstGraphError> {
    application_native_json(node, true)
}
fn application_boundary_json(node: &AntlrNativeSyntax<'_, '_>) -> Result<Value, AstGraphError> {
    application_native_json(node, false)
}
fn application_json(application: &AntlrOracleApplication<'_, '_>) -> Result<Value, AstGraphError> {
    let mut groups = Vec::new();
    for group in &application.groups {
        let mut arguments = Vec::new();
        for argument in &group.arguments {
            arguments.push(json!({"syntax":application_syntax_json(&argument.syntax)?,
                "name":argument.name.as_ref().map(application_syntax_json).transpose()?,
                "expression":oracle_expression_json(&argument.expression)?}));
        }
        groups.push(
            json!({"syntax":application_syntax_json(&group.syntax)?,"arguments":arguments,
            "keep_clause":group.keep_clause.as_ref().map(application_syntax_json).transpose()?}),
        );
    }
    let mut value = json!({"occurrence_index":application.occurrence_index,"kind":format!("{:?}",application.kind),
        "head":application_syntax_json(&application.head)?,"groups":groups,
        "boundaries":application.boundaries.iter().map(application_boundary_json).collect::<Result<Vec<_>,_>>()?});
    if let Some(collection) = &application.collection_statement {
        value["collection_statement"] = json!({"syntax":application_boundary_json(&collection.syntax)?,
            "receiver":oracle_expression_json(&collection.receiver)?,
            "indices":collection.indices.iter().map(application_syntax_json).collect::<Result<Vec<_>,_>>()?,
            "delimiters":collection.delimiters.iter().map(application_boundary_json).collect::<Result<Vec<_>,_>>()?});
    }
    if let Some(member) = &application.member {
        value["member"] = json!({"syntax":application_boundary_json(&member.syntax)?,
            "receiver":member.receiver.as_ref().map(application_boundary_json).transpose()?,
            "name":member.name.as_ref().map(application_boundary_json).transpose()?,
            "separator":member.separator.as_ref().map(application_boundary_json).transpose()?,
            "delimiters":member.delimiters.iter().map(application_boundary_json).collect::<Result<Vec<_>,_>>()?});
    }
    if let Some(assignment) = &application.assignment {
        value["assignment"] = json!({"statement":application_boundary_json(&assignment.statement)?,
            "branch":application_boundary_json(&assignment.branch)?,"side":format!("{:?}",assignment.side),
            "expressions":assignment.expressions.iter().map(application_boundary_json).collect::<Result<Vec<_>,_>>()?});
    }
    Ok(value)
}

fn oracle_lookup_json(lookup: &AntlrOracleLookup<'_, '_>) -> Result<Value, AstGraphError> {
    let mut candidates = Vec::new();
    for c in &lookup.candidates {
        candidates.push(json!({"declaration":application_boundary_json(&c.declaration)?,
            "name":c.name.as_ref().map(application_boundary_json).transpose()?,
            "scope":application_boundary_json(&c.scope)?,
            "scope_evidence":c.scope_evidence.iter().map(application_boundary_json).collect::<Result<Vec<_>,_>>()?,
            "kind":format!("{:?}",c.kind),"decoded_name":c.decoded_name.as_deref(),"quote_style":c.quote_style,
            "entity_indices":c.entity_indices}));
    }
    Ok(
        json!({"state":format!("{:?}",lookup.state),"reference":lookup.reference.as_ref().map(application_boundary_json).transpose()?,
        "decoded_name":lookup.decoded_name.as_deref(),"quote_style":lookup.quote_style,
        "candidates":candidates,"scope_continuity":lookup.scope_continuity.iter().map(|p|Ok(json!({"scope_node_index":p.scope_node_index,"reference_node_index":p.reference_node_index,"prepared_start":p.prepared_start,"prepared_end":p.prepared_end,"original":origin_json(p.original)?,"verified":p.verified}))).collect::<Result<Vec<_>,AstGraphError>>()?,"issues":lookup.issues.iter().map(|i|format!("{i:?}")).collect::<Vec<_>>()}),
    )
}
fn oracle_binding_json(
    binding: &AntlrOracleApplicationBinding<'_, '_>,
) -> Result<Value, AstGraphError> {
    let mut value = json!({"occurrence_index":binding.occurrence_index,"receiver":oracle_lookup_json(&binding.receiver)?,
        "declared_type":binding.declared_type.as_ref().map(oracle_lookup_json).transpose()?,
        "classification":format!("{:?}",binding.classification),"collection_type":binding.collection_type.map(|k|format!("{k:?}")),
        "type_evidence":binding.type_evidence.iter().map(application_boundary_json).collect::<Result<Vec<_>,_>>()?,
        "issues":binding.issues.iter().map(|i|format!("{i:?}")).collect::<Vec<_>>()});
    if let Some(kind) = binding.collection_method {
        value["collection_method"] = json!(format!("{kind:?}"));
    }
    if let Some(proof) = &binding.member_continuity {
        value["member_continuity"] = json!({"syntax":application_boundary_json(&proof.syntax)?,
            "verified":proof.verified});
    }
    Ok(value)
}

fn oracle_call_source_json(source: &AntlrOracleCallSourceState) -> Value {
    match source {
        AntlrOracleCallSourceState::Bound { entity_index } => {
            json!({"state":"Bound","entity_index":entity_index})
        }
        AntlrOracleCallSourceState::Ambiguous { entity_indices } => {
            json!({"state":"Ambiguous","entity_indices":entity_indices})
        }
        AntlrOracleCallSourceState::Unavailable { reason } => {
            json!({"state":"Unavailable","reason":format!("{reason:?}")})
        }
    }
}
fn oracle_call_target_json(target: &AntlrOracleCallTargetState) -> Value {
    match target {
        AntlrOracleCallTargetState::Resolved { entity_index } => {
            json!({"state":"Resolved","entity_index":entity_index})
        }
        AntlrOracleCallTargetState::NotFound => json!({"state":"NotFound"}),
        AntlrOracleCallTargetState::Ambiguous { entity_indices } => {
            json!({"state":"Ambiguous","entity_indices":entity_indices})
        }
        AntlrOracleCallTargetState::Unresolved { reason } => {
            json!({"state":"Unresolved","reason":format!("{reason:?}")})
        }
    }
}
fn oracle_call_issues_json(issues: &[AntlrOracleCallIssue]) -> Value {
    json!(
        issues
            .iter()
            .map(|i| json!({"reason":format!("{:?}",i.reason),"node_index":i.node_index}))
            .collect::<Vec<_>>()
    )
}
fn oracle_call_json(call: &AntlrOracleCallOutcome<'_, '_>) -> Result<Value, AstGraphError> {
    Ok(json!({
        "call_index":call.call_index,"occurrence_index":call.occurrence_index,
        "source":oracle_call_source_json(&call.source),"target":oracle_call_target_json(&call.target),
        "caller":call.caller.as_ref().map(application_boundary_json).transpose()?,
        "body":call.body.as_ref().map(application_boundary_json).transpose()?,
        "evidence":call.evidence.iter().map(application_boundary_json).collect::<Result<Vec<_>,_>>()?,
        "binding_issues":call.binding_issues.iter().map(|i|format!("{i:?}")).collect::<Vec<_>>(),
        "issues":oracle_call_issues_json(&call.issues),
        "call_lookup":call.call_lookup.as_ref().map(oracle_lookup_json).transpose()?,
        "group_outcomes":call.group_outcomes.iter().map(|g| Ok(json!({
            "group_index":g.group_index,"syntax":application_boundary_json(&g.syntax)?,
            "target":oracle_call_target_json(&g.target),"issues":oracle_call_issues_json(&g.issues)
        }))).collect::<Result<Vec<_>,AstGraphError>>()?
    }))
}
fn oracle_call_unresolved_reason(
    call: &AntlrOracleCallOutcome<'_, '_>,
    reason: AntlrOracleCallReason,
    head: usize,
) -> TsLocalUnresolvedReason {
    use AntlrOracleCallReason as R;
    let index = call
        .issues
        .iter()
        .rev()
        .find(|i| i.reason == reason)
        .and_then(|i| i.node_index);
    match (reason, index) {
        (R::MissingTargetRule, _) => TsLocalUnresolvedReason::MissingTargetRule,
        (R::UnsupportedPolicy, _) => TsLocalUnresolvedReason::UnsupportedPolicy,
        (R::NativeRecovery, Some(node_index)) => TsLocalUnresolvedReason::Recovery { node_index },
        (R::NonExactOrigin, Some(node_index)) => TsLocalUnresolvedReason::NonExact { node_index },
        (R::DeclarationVisibility, Some(declaration_node_index)) => {
            TsLocalUnresolvedReason::Shadowed {
                declaration_node_index,
            }
        }
        _ => TsLocalUnresolvedReason::UnsupportedSyntax { node_index: head },
    }
}

fn policy_json(q: &crate::catalog::Query) -> Value {
    let mut value = query_json(q);
    value["source"] = json!(format!("{:?}", q.source().value()));
    value["query_kind"] = json!(format!("{:?}", q.r#type().value()));
    value["provenance"] = json!({
        "source":format!("{:?}",q.source().provenance()),
        "data_key":format!("{:?}",q.data_key().provenance()),
        "graph_label":format!("{:?}",q.graph_label().provenance()),
        "pattern":format!("{:?}",q.pattern().provenance()),
        "select":format!("{:?}",q.select().provenance()),
        "keyword":format!("{:?}",q.keyword().provenance()),
        "child_rule":format!("{:?}",q.child_rule().provenance()),
        "negate":format!("{:?}",q.negate().provenance()),
        "captures":format!("{:?}",q.captures().provenance()),
        "parent_label":format!("{:?}",q.parent_label().provenance()),
        "value_label":format!("{:?}",q.value_label().provenance()),
        "relation_type":format!("{:?}",q.relation_type().provenance()),
        "name_reject":format!("{:?}",q.name_reject().provenance()),
        "name_is_data":format!("{:?}",q.name_is_data().provenance()),
        "target_labels":format!("{:?}",q.target_labels().provenance()),
        "target_fallback":format!("{:?}",q.target_fallback().provenance()),
        "query_kind":format!("{:?}",q.r#type().provenance()),
    });
    value
}
fn occurrence_json(
    index: usize,
    occurrence: &SemanticOccurrence<'_, '_, '_>,
) -> Result<Value, AstGraphError> {
    Ok(
        json!({"occurrence_index":index,"query_index":occurrence.query_index,"policy":policy_json(occurrence.query),
        "matched":node_json(&occurrence.matched)?,"name":binding_json(&occurrence.name)?,
        "value":binding_json(&occurrence.value)?,"span":binding_json(&occurrence.span)?,
        "parent":binding_json(&occurrence.parent)?,"qualifier":binding_json(&occurrence.qualifier)?,
        "context_indices":occurrence.contexts}),
    )
}
fn qualified_json(name: Option<&AntlrQualifiedName<'_>>) -> Result<Value, AstGraphError> {
    let Some(name) = name else {
        return Ok(Value::Null);
    };
    let mut components = Vec::new();
    for component in name.components() {
        components.push(json!({"node_index":component.node_index,"raw_name":component.raw.text(),
            "range":range_json(component.raw)?,"decoded":component.decoded.as_ref(),"quote_style":component.quote_style}));
    }
    Ok(json!({"components":components}))
}
fn type_json(observation: &AntlrTypeObservation<'_, '_>) -> Result<Value, AstGraphError> {
    Ok(match observation {
        AntlrTypeObservation::Missing => json!({"status":"Missing"}),
        AntlrTypeObservation::NonExact(origin) => {
            json!({"status":"NonExact","origin":origin_json(origin)?})
        }
        AntlrTypeObservation::Present { node_index, raw } => {
            json!({"status":"Present","node_index":node_index,"literal_source":raw.text(),"range":range_json(*raw)?})
        }
    })
}
fn value_json(value: &AntlrValueObservation<'_, '_>) -> Result<Value, AstGraphError> {
    use sqlparser::{keywords::Keyword, tokenizer::Token};
    Ok(match value {
        AntlrValueObservation::NotConfigured => json!({"status":"NotConfigured"}),
        AntlrValueObservation::Missing => json!({"status":"Missing"}),
        AntlrValueObservation::NonExact(origin) => {
            json!({"status":"NonExact","origin":origin_json(origin)?})
        }
        AntlrValueObservation::Expression { raw } => {
            json!({"status":"Expression","literal_source":raw.text(),"range":range_json(*raw)?})
        }
        AntlrValueObservation::Scalar { raw, token } => {
            let decoded = match token {
                Token::Number(lexeme, long) => {
                    json!({"kind":"Number","validated_number_lexeme":lexeme,"long":long})
                }
                Token::SingleQuotedString(value) => json!({"kind":"String","decoded_value":value}),
                Token::NationalStringLiteral(value) => {
                    json!({"kind":"NationalString","decoded_value":value})
                }
                Token::Word(word) => match word.keyword {
                    Keyword::TRUE => json!({"kind":"Bool","decoded_value":true}),
                    Keyword::FALSE => json!({"kind":"Bool","decoded_value":false}),
                    Keyword::NULL => json!({"kind":"Null","decoded_value":Value::Null}),
                    _ => {
                        return Err(AstGraphError::NativeContract(
                            "ANTLR model scalar has unsupported word",
                        ));
                    }
                },
                _ => {
                    return Err(AstGraphError::NativeContract(
                        "ANTLR model scalar has unsupported token",
                    ));
                }
            };
            json!({"status":"Scalar","literal_source":raw.text(),"range":range_json(*raw)?,"value":decoded})
        }
    })
}
fn oracle_keyword_json(value: &AntlrOracleKeyword<'_, '_>) -> Result<Value, AstGraphError> {
    Ok(
        json!({"node_index":value.node_index,"kind":value.node.kind,"class":format!("{:?}",value.node.class),"span_status":format!("{:?}",value.node.status),"error":value.node.error,"missing":value.node.missing,"original":value.original.map(origin_json).transpose()?,"literal_source":match value.original { Some(crate::prepared_input::OriginalProjection::Exact(raw))=>Some(raw.text()), _=>None }}),
    )
}
fn oracle_expression_json(value: &AntlrOracleExpression<'_, '_>) -> Result<Value, AstGraphError> {
    Ok(match value {
        AntlrOracleExpression::Missing {
            parent_node_index,
            role,
        } => json!({"status":"Missing","parent_node_index":parent_node_index,"role":role}),
        AntlrOracleExpression::Present {
            node_index,
            node,
            original,
        } => {
            let keyword = AntlrOracleKeyword {
                node_index: *node_index,
                node,
                original: *original,
            };
            json!({"status":"Present","native":oracle_keyword_json(&keyword)?})
        }
    })
}
fn oracle_declaration_json(
    value: &AntlrOracleDeclaration<'_, '_>,
    original: &SnapshotContext,
) -> Result<Value, AstGraphError> {
    original.check()?;
    Ok(match value {
        AntlrOracleDeclaration::Parameter {
            modes,
            default_value,
        } => {
            let mut keywords = Vec::new();
            for mode in modes {
                original.check()?;
                keywords.push(oracle_keyword_json(mode)?);
            }
            json!({"kind":"Parameter","modes":keywords,"default_value":default_value.as_ref().map(oracle_expression_json).transpose()?})
        }
        AntlrOracleDeclaration::Type(details) => {
            let crate::antlr_engine::model::AntlrOracleTypeDeclaration {
                kind,
                definition,
                element_type,
                index_type,
                bound,
                modifiers,
            } = details.as_ref();
            let mut keywords = Vec::new();
            for modifier in modifiers {
                original.check()?;
                keywords.push(oracle_keyword_json(modifier)?);
            }
            json!({"kind":"Type","type_kind":format!("{:?}",kind),"definition":type_json(definition)?,"element_type":element_type.as_ref().map(type_json).transpose()?,"index_type":index_type.as_ref().map(type_json).transpose()?,"bound":bound.as_ref().map(oracle_expression_json).transpose()?,"modifiers":keywords})
        }
    })
}
fn contribution_json(
    c: &AntlrContribution<'_, '_, '_, '_>,
    original: &SnapshotContext,
) -> Result<Value, AstGraphError> {
    let mut roles = Vec::new();
    for typed in &c.type_roles {
        original.check()?;
        roles.push(json!({"role":format!("{:?}",typed.role),"name_node_index":typed.name_node_index,"observation":type_json(&typed.observation)?}));
    }
    Ok(
        json!({"occurrence":occurrence_json(c.occurrence_index,c.occurrence)?,"extent":extent_json(&c.extent)?,
        "declared_type":type_json(&c.declared_type)?,"declared_value":value_json(&c.declared_value)?,"type_roles":roles}),
    )
}

/// Consume the retained genuine ANTLR model; no Source parse or DTO reconstruction.
pub fn assemble_graph_antlr(
    input: AstGraphAntlrInput<'_, '_, '_, '_, '_, '_>,
    original: &Arc<SnapshotContext>,
) -> Result<AstSemanticGraphBatch, AstGraphError> {
    original.check()?;
    let result = (|| {
        let document = input.document;
        let parsed = document.parsed();
        if !std::ptr::eq(parsed.original_context(), original.as_ref()) {
            return Err(NativeNnError::RequestMismatch.into());
        }
        let identity = parsed.identity_document();
        let (path, hash) = associated_source(&input.source, identity, parsed.source(), original)?;
        let empty = parsed.source().is_empty();
        let source_observation = match &input.source {
            NativeNnSource::Indexed(_) => AstGraphSourceObservation::IndexedStored { empty },
            NativeNnSource::Physical { .. } => {
                AstGraphSourceObservation::PhysicalTextNotStored { empty }
            }
        };
        let raw = match input.source {
            NativeNnSource::Indexed(raw) | NativeNnSource::Physical { raw, .. } => raw,
        };
        let observed = raw.branch_observation();
        let association = AstGraphAssociation {
            path: path.clone(),
            relative_components: identity
                .relative_components()
                .iter()
                .map(|s| (*s).into())
                .collect(),
            raw_version: raw.version(),
            source_sha256: hash,
            source_observation,
            physical_branch: observed.and_then(|o| o.physical_branch).map(str::to_owned),
            inherited_target: observed.and_then(|o| o.inherited_target).map(str::to_owned),
        };
        let observations = parsed.observations();
        let profile = observations.profile;
        let mut nodes = Vec::new();
        for (entity_index, entity) in document.entities().iter().enumerate() {
            original.check()?;
            let uid = entity.identity().uid().as_str();
            let key = syntax_key(entity.identity(), entity.data_key())?;
            let mut properties = BTreeMap::new();
            for (key, value) in [
                ("uid", uid),
                ("data_key", entity.data_key()),
                ("name", entity.name()),
                ("raw_name", entity.name_raw().text()),
                ("path", path.as_str()),
                ("lang", profile.language().value().as_str()),
                ("grammar", profile.grammar().value().as_str()),
                ("name_representation", "Identifier"),
            ] {
                properties.insert(key.into(), AstGraphValue::String(value.into()));
            }
            properties.insert(
                "raw_version".into(),
                AstGraphValue::String(raw.version().to_string()),
            );
            properties.insert(
                "source_sha256".into(),
                AstGraphValue::String(hash.iter().map(|b| format!("{b:02x}")).collect()),
            );
            properties.insert(
                "line_number".into(),
                AstGraphValue::Int64(line(entity.name_raw().start_point().row)?),
            );
            properties.insert(
                "source_stored".into(),
                AstGraphValue::Bool(matches!(
                    source_observation,
                    AstGraphSourceObservation::IndexedStored { .. }
                )),
            );
            properties.insert("source_empty".into(), AstGraphValue::Bool(empty));
            add_range(&mut properties, "name_", entity.name_raw())?;
            let extent_status = match entity.extent() {
                AntlrEntityExtent::Exact(extent) => {
                    add_range(&mut properties, "", extent.range())?;
                    "Exact"
                }
                AntlrEntityExtent::NonExact(_) => "NonExact",
                AntlrEntityExtent::Missing => "Missing",
                AntlrEntityExtent::Conflict(_) => "Conflict",
            };
            properties.insert(
                "extent_status".into(),
                AstGraphValue::String(extent_status.into()),
            );
            properties.insert(
                "available".into(),
                AstGraphValue::Bool(
                    extent_status == "Exact"
                        && entity.conflicts().is_empty()
                        && entity.diagnostic_indices().is_empty()
                        && entity
                            .namespace()
                            .iter()
                            .all(|n| matches!(n.extent(), ExtentObservation::Exact(_))),
                ),
            );
            let mut contributions = Vec::new();
            for contribution in entity.contributions() {
                original.check()?;
                contributions.push(contribution_json(contribution, original)?);
            }
            let mut namespaces = Vec::new();
            for namespace in entity.namespace() {
                original.check()?;
                namespaces.push(json!({"uid":namespace.identity().uid().as_str(),"label":namespace.identity().label(),"name":namespace.name(),
                    "node_index":namespace.node_index(),"context_node_index":namespace.context_node_index(),
                    "anchor":range_json(namespace.anchor())?,"extent":extent_json(namespace.extent())?,"qualified_name":qualified_json(namespace.qualified_name())?}));
            }
            let mut metadata = json!({
                "declaration_node_index":entity.declaration_node_index(),"declaration_role":format!("{:?}",entity.role()),
                "qualified_name":qualified_json(entity.qualified_name())?,"diagnostic_indices":entity.diagnostic_indices(),
                "conflicts":entity.conflicts().iter().map(|c|json!({"field":format!("{:?}",c.field),"first_contribution":c.first_contribution,"second_contribution":c.second_contribution})).collect::<Vec<_>>(),
                "contributions":contributions,"namespaces":namespaces,
                "owner_links":document.owner_links().iter().filter(|link|link.child_entity_index==entity_index).map(|link|json!({"child_entity_index":link.child_entity_index,"namespace_index":link.namespace_index,"owner_entity_indices":link.owner_entity_indices})).collect::<Vec<_>>(),
            });
            if let Some(observation) = entity.oracle_declaration() {
                metadata["oracle_declaration"] = oracle_declaration_json(observation, original)?;
            }
            properties.insert(
                "metadata_json".into(),
                AstGraphValue::String(serde_json::to_string(&metadata)?),
            );
            if let Some(value) = association.physical_branch() {
                properties.insert(
                    "physical_branch".into(),
                    AstGraphValue::String(value.into()),
                );
            }
            if let Some(value) = association.inherited_target() {
                properties.insert(
                    "inherited_target".into(),
                    AstGraphValue::String(value.into()),
                );
            }
            nodes.push(AstGraphNode {
                label: entity.identity().label().into(),
                key,
                properties,
            });
        }
        let mut diagnostics = Vec::new();
        for diagnostic in document.diagnostics() {
            original.check()?;
            diagnostics.push(json!({"occurrence_index":diagnostic.occurrence_index,"declaration_node_index":diagnostic.declaration_node_index,
                "subject_node_index":diagnostic.subject_node_index,"issue":format!("{:?}",diagnostic.issue),
                "original":diagnostic.original.map(origin_json).transpose()?}));
        }
        for diagnostic in &observations.syntax.diagnostics {
            original.check()?;
            diagnostics.push(json!({"source":"NativeSyntax","grammar":diagnostic.grammar,"phase":format!("{:?}",diagnostic.phase),
                "line":diagnostic.line,"column":diagnostic.column,"prepared_span":diagnostic.span,"status":format!("{:?}",diagnostic.status),"message":diagnostic.message}));
        }
        let mut capabilities = Vec::new();
        for (query_index, entry) in profile.queries().value().iter().enumerate() {
            original.check()?;
            capabilities.push(json!({"query_index":query_index,"policy":policy_json(entry.value()),"adapter":"AntlrSemanticDocument",
                "query_kind":format!("{:?}",entry.value().r#type().value()),"matches":observations.occurrences.iter().filter(|o|o.query_index==query_index).count()}));
        }
        let mut outcomes = Vec::new();
        let mut edges = Vec::new();
        let mut edge_keys = std::collections::BTreeSet::new();
        let mut relation_occurrences = LogicalRelationOccurrences::new(logical_file(&association)?);
        for &index in document.unresolved_relations() {
            original.check()?;
            let occurrence =
                observations
                    .occurrences
                    .get(index)
                    .ok_or(AstGraphError::NativeContract(
                        "ANTLR relation references foreign occurrence",
                    ))?;
            let relation_type = occurrence.query.relation_type().value();
            let target_rule = profile
                .target_rules()
                .value()
                .iter()
                .find(|(key, _)| key.value() == relation_type)
                .map(|(_, rule)| rule);
            let mut contexts = Vec::new();
            for &context_index in &occurrence.contexts {
                original.check()?;
                let context = observations.contexts.get(context_index).ok_or(
                    AstGraphError::NativeContract("ANTLR relation references foreign context"),
                )?;
                contexts.push(json!({"context_index":context_index,"selector":context.selector,"label":context.label,
                    "name_is_explicit":context.name_is_explicit,"matched":node_json(&context.matched)?,"name":binding_json(&context.name)?}));
            }
            let mut observation = json!({"occurrence":occurrence_json(index,occurrence)?,"contexts":contexts,
                "target_rule":target_rule.map(|rule|json!({"labels":rule.value().labels().value().iter().map(|s|s.value()).collect::<Vec<_>>(),"fallback":rule.value().fallback().value()})),
                "source_state":"ObservedAntlrRelation"});
            if let Some(application) = document.oracle_application(index) {
                observation["oracle_application"] = application_json(application)?;
            }
            if let Some(binding) = document.oracle_binding(index) {
                observation["oracle_binding"] = oracle_binding_json(binding)?;
            }
            let mut source_index = None;
            let mut evidence_node_indices = vec![occurrence.matched.node_index];
            let mut target = AstGraphRelationTarget::ObservedOnly;
            if let Some(call) = document.oracle_call(index) {
                observation["oracle_call"] = oracle_call_json(call)?;
                observation["source_state"] = json!(match call.source {
                    AntlrOracleCallSourceState::Bound { .. } => "Bound",
                    AntlrOracleCallSourceState::Ambiguous { .. } => "Ambiguous",
                    AntlrOracleCallSourceState::Unavailable { .. } => "Unavailable",
                });
                evidence_node_indices = call.evidence.iter().map(|e| e.node_index).collect();
                if let AntlrOracleCallSourceState::Bound { entity_index } = call.source {
                    node_key(&nodes, entity_index)?;
                    source_index = Some(call.call_index);
                }
                target = match &call.target {
                    AntlrOracleCallTargetState::Resolved { entity_index } => {
                        let target_key = node_key(&nodes, *entity_index)?;
                        if let AntlrOracleCallSourceState::Bound {
                            entity_index: caller_index,
                        } = call.source
                        {
                            let source_key = node_key(&nodes, caller_index)?;
                            let CaptureBinding::Present(capture) = &occurrence.name else {
                                return Err(AstGraphError::NativeContract(
                                    "Oracle CALLS resolved target lacks capture",
                                ));
                            };
                            let Some(OriginalProjection::Exact(anchor)) = &capture.original else {
                                return Err(AstGraphError::NativeContract(
                                    "Oracle CALLS resolved target lacks exact Original capture",
                                ));
                            };
                            let kind = occurrence.query.relation_type().value();
                            let edge = relation_occurrences.admit(
                                source_key,
                                target_key,
                                kind,
                                ParserRelationOccurrence {
                                    query_data_key: Some(occurrence.query.data_key().value()),
                                    target: anchor.text(),
                                    anchor: (anchor.start(), anchor.end()),
                                },
                            )?;
                            if edge_keys.insert(edge.uid.clone()) {
                                edges.push(edge);
                            }
                        }
                        AstGraphRelationTarget::Resolved {
                            key: target_key.into(),
                        }
                    }
                    AntlrOracleCallTargetState::NotFound => AstGraphRelationTarget::NotFound,
                    AntlrOracleCallTargetState::Ambiguous { entity_indices } => {
                        AstGraphRelationTarget::Ambiguous {
                            keys: entity_indices
                                .iter()
                                .map(|&i| node_key(&nodes, i).map(str::to_owned))
                                .collect::<Result<Vec<_>, _>>()?,
                        }
                    }
                    AntlrOracleCallTargetState::Unresolved { reason } => {
                        AstGraphRelationTarget::Unresolved {
                            reason: oracle_call_unresolved_reason(
                                call,
                                *reason,
                                occurrence.matched.node_index,
                            ),
                        }
                    }
                };
            }
            outcomes.push(AstGraphRelationOutcome {
                relation_index: index,
                source_index,
                capture_index: None,
                evidence_node_indices,
                target,
                observation_json: serde_json::to_string(&observation)?,
            });
        }
        let diagnostics_json = serde_json::to_string(&diagnostics)?;
        let capabilities_json = serde_json::to_string(&capabilities)?;
        let file_observation = file_observation_node(
            &association,
            identity.language(),
            identity.grammar(),
            &outcomes,
            &diagnostics_json,
            &capabilities_json,
            original,
        )?;
        Ok(AstSemanticGraphBatch {
            file_observation,
            association,
            nodes,
            edges,
            outcomes,
            diagnostics_json,
            capabilities_json,
        })
    })();
    original.check()?;
    result
}
