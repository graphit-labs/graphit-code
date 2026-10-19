//! Graph publication for genuine PostgreSQL/TSQL/COBOL fallback observations.
//!
//! Only native declaration ancestry produces CONTAINS. Relation occurrences keep
//! their capture/origin/context/rule evidence; names alone never produce CALLS.
use super::*;
use crate::antlr_engine::{
    ParsedSyntax,
    generic_model::{GenericAntlrObservations, GenericAntlrTree, ParsedGenericAntlrDocument},
    semantic::{
        CaptureBinding, CapturedNode, SemanticContext, SemanticDocument, SemanticOccurrence,
    },
};

pub struct AstGraphAntlrGenericInput<'a, 'profile, 'o, 'v, 'request> {
    pub document: &'a ParsedGenericAntlrDocument<'profile, 'o, 'v, 'request>,
    pub source: NativeNnSource<'a>,
}

fn capture_json(capture: &CapturedNode<'_, '_>) -> Result<Value, AstGraphError> {
    Ok(json!({"node_index":capture.node_index,
        "prepared":capture.prepared.map(range_json).transpose()?,
        "original":capture.original.as_ref().map(origin_json).transpose()?}))
}
fn binding_json(binding: &CaptureBinding<'_, '_>) -> Result<Value, AstGraphError> {
    Ok(match binding {
        CaptureBinding::NotConfigured => json!({"status":"NotConfigured"}),
        CaptureBinding::Missing => json!({"status":"Missing"}),
        CaptureBinding::Present(capture) => {
            json!({"status":"Present","capture":capture_json(capture)?})
        }
    })
}
fn occurrence_json(
    index: usize,
    occurrence: &SemanticOccurrence<'_, '_, '_>,
) -> Result<Value, AstGraphError> {
    Ok(
        json!({"occurrence_index":index,"query_index":occurrence.query_index,
        "policy":query_json(occurrence.query),"query_kind":format!("{:?}",occurrence.query.r#type().value()),
        "source":format!("{:?}",occurrence.query.source().value()),
        "matched":capture_json(&occurrence.matched)?,"name":binding_json(&occurrence.name)?,
        "span":binding_json(&occurrence.span)?,"value":binding_json(&occurrence.value)?,
        "parent":binding_json(&occurrence.parent)?,"qualifier":binding_json(&occurrence.qualifier)?,
        "contexts":occurrence.contexts}),
    )
}
fn context_json(context: &SemanticContext<'_, '_, '_>) -> Result<Value, AstGraphError> {
    Ok(
        json!({"selector":context.selector,"label":context.label,"name_is_explicit":context.name_is_explicit,
        "matched":capture_json(&context.matched)?,"name":binding_json(&context.name)?}),
    )
}
fn syntax_json(
    syntax: &ParsedSyntax<'_>,
    original: &SnapshotContext,
) -> Result<Value, AstGraphError> {
    let mut nodes = Vec::new();
    for node in &syntax.nodes {
        original.check()?;
        nodes.push(json!({"kind":node.kind,"class":format!("{:?}",node.class),"span":node.span,
            "status":format!("{:?}",node.status),"parent":node.parent,"error":node.error,"missing":node.missing}));
    }
    let mut diagnostics = Vec::new();
    for d in &syntax.diagnostics {
        original.check()?;
        diagnostics.push(
            json!({"grammar":d.grammar,"phase":format!("{:?}",d.phase),"line":d.line,
            "column":d.column,"span":d.span,"status":format!("{:?}",d.status),"message":d.message}),
        );
    }
    Ok(json!({"nodes":nodes,"diagnostics":diagnostics}))
}
fn main_json(
    document: &SemanticDocument<'_, '_, '_>,
    original: &SnapshotContext,
) -> Result<Value, AstGraphError> {
    let mut occurrences = Vec::new();
    let mut contexts = Vec::new();
    for (index, occurrence) in document.occurrences.iter().enumerate() {
        original.check()?;
        occurrences.push(occurrence_json(index, occurrence)?);
    }
    for context in &document.contexts {
        original.check()?;
        contexts.push(context_json(context)?);
    }
    Ok(
        json!({"tree":GenericAntlrTree::Main,"syntax":syntax_json(&document.syntax,original)?,"occurrences":occurrences,"contexts":contexts}),
    )
}
fn observed_relation(
    tree: GenericAntlrTree,
    index: usize,
    occurrence: &SemanticOccurrence<'_, '_, '_>,
    main: &SemanticDocument<'_, '_, '_>,
    extra: Value,
) -> Result<AstGraphRelationOutcome, AstGraphError> {
    let rule = main
        .profile
        .target_rules()
        .value()
        .iter()
        .find(|(name, _)| name.value() == occurrence.query.relation_type().value())
        .map(|(_, rule)| rule);
    Ok(AstGraphRelationOutcome {
        relation_index: index,
        source_index: None,
        capture_index: None,
        evidence_node_indices: vec![occurrence.matched.node_index],
        target: AstGraphRelationTarget::ObservedOnly,
        observation_json: serde_json::to_string(
            &json!({"tree":tree,"occurrence":occurrence_json(index,occurrence)?,
            "target_rule":rule.map(|rule|json!({"labels":rule.value().labels().value().iter().map(|s|s.value()).collect::<Vec<_>>(),"fallback":rule.value().fallback().value()})),
            "extra":extra,"source_state":"ObservedAntlrRelation"}),
        )?,
    })
}

/// Validate genuine Source association before returning an owned existing Graph batch.
pub fn assemble_graph_antlr_generic(
    input: AstGraphAntlrGenericInput<'_, '_, '_, '_, '_>,
    original: &Arc<SnapshotContext>,
) -> Result<AstSemanticGraphBatch, AstGraphError> {
    original.check()?;
    let result = (|| {
        let document = input.document;
        if !std::ptr::eq(document.original_context(), original.as_ref()) {
            return Err(NativeNnError::RequestMismatch.into());
        }
        let identity = document.identity_document();
        let (path, hash) = associated_source(&input.source, identity, document.source(), original)?;
        let empty = document.source().is_empty();
        let source_observation = match &input.source {
            NativeNnSource::Indexed(_) => AstGraphSourceObservation::IndexedStored { empty },
            NativeNnSource::Physical { .. } => {
                AstGraphSourceObservation::PhysicalTextNotStored { empty }
            }
        };
        let raw = match input.source {
            NativeNnSource::Indexed(raw) | NativeNnSource::Physical { raw, .. } => raw,
        };
        let branch = raw.branch_observation();
        let association = AstGraphAssociation {
            path: path.clone(),
            relative_components: identity
                .relative_components()
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            raw_version: raw.version(),
            source_sha256: hash,
            source_observation,
            physical_branch: branch.and_then(|b| b.physical_branch).map(str::to_owned),
            inherited_target: branch.and_then(|b| b.inherited_target).map(str::to_owned),
        };
        let observations = document.observations();
        let main = observations.main();
        let profile = main.profile;
        let mut nodes = Vec::new();
        for entity in document.entities() {
            original.check()?;
            let mut properties = BTreeMap::new();
            for (key, value) in [
                ("uid", entity.uid()),
                ("data_key", entity.data_key()),
                ("name", entity.name().text()),
                ("raw_name", entity.name().text()),
                ("path", path.as_str()),
                ("lang", profile.language().value()),
                ("grammar", profile.grammar().value()),
                ("name_representation", "NativeSource"),
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
                "source_stored".into(),
                AstGraphValue::Bool(matches!(
                    source_observation,
                    AstGraphSourceObservation::IndexedStored { .. }
                )),
            );
            properties.insert("source_empty".into(), AstGraphValue::Bool(empty));
            properties.insert(
                "line_number".into(),
                AstGraphValue::Int64(line(entity.name().start_point().row)?),
            );
            add_range(&mut properties, "name_", entity.name())?;
            add_range(&mut properties, "display_", entity.name())?;
            let status = if entity.extent_conflict() {
                "Conflict"
            } else if let Some(extent) = entity.extent() {
                add_range(&mut properties, "", extent)?;
                "Exact"
            } else {
                "Unavailable"
            };
            properties.insert("extent_status".into(), AstGraphValue::String(status.into()));
            properties.insert("available".into(), AstGraphValue::Bool(status == "Exact"));
            if entity.values().len() == 1 {
                properties.insert(
                    "value".into(),
                    AstGraphValue::String(entity.values()[0].text().into()),
                );
            }
            properties.insert("metadata_json".into(),AstGraphValue::String(serde_json::to_string(&json!({
                "declaration":entity.declaration(),"contributions":entity.contributions(),"owner_indices":entity.owner_indices(),
                "extent_conflict":entity.extent_conflict(),"extent_provenance":entity.extent_provenance(),"values":entity.values().iter().copied().map(range_json).collect::<Result<Vec<_>,_>>()?}))?));
            for (key, value) in [
                ("physical_branch", association.physical_branch()),
                ("inherited_target", association.inherited_target()),
            ] {
                if let Some(value) = value {
                    properties.insert(key.into(), AstGraphValue::String(value.into()));
                }
            }
            nodes.push(AstGraphNode {
                label: entity.label().into(),
                key: serde_json::to_string(&[entity.uid(), entity.data_key()])?,
                properties,
            });
        }
        let mut edges = Vec::new();
        let mut edge_keys = BTreeSet::new();
        for (child, entity) in document.entities().iter().enumerate() {
            original.check()?;
            if let [owner] = entity.owner_indices() {
                if entity.extent().is_none()
                    || entity.extent_conflict()
                    || document.entities()[*owner].extent().is_none()
                    || document.entities()[*owner].extent_conflict()
                {
                    continue;
                }
                let source_key = node_key(&nodes, *owner)?;
                let target_key = node_key(&nodes, child)?;
                let uid = serde_json::to_string(&(
                    "CONTAINS",
                    source_key,
                    target_key,
                    entity.declaration(),
                ))?;
                if edge_keys.insert(uid.clone()) {
                    edges.push(AstGraphEdge {
                        kind: "CONTAINS".into(),
                        source_key: source_key.into(),
                        target_key: target_key.into(),
                        uid,
                    });
                }
            }
        }
        let mut outcomes = Vec::new();
        for (index, occurrence) in main.occurrences.iter().enumerate() {
            original.check()?;
            if *occurrence.query.r#type().value() == crate::profiles::QueryKind::Relation {
                outcomes.push(observed_relation(
                    GenericAntlrTree::Main,
                    index,
                    occurrence,
                    main,
                    Value::Null,
                )?);
            }
        }
        let mut diagnostics = main_json(main, original)?;
        match observations {
            GenericAntlrObservations::TSql(_) => {}
            GenericAntlrObservations::Cobol(document) => {
                let mut occurrences = Vec::new();
                for (index, found) in document.preprocessor.occurrences.iter().enumerate() {
                    original.check()?;
                    let scope = match &found.scope {
                        crate::antlr_engine::cobol_copy::CopyProgramScope::Linked {
                            main_anchor_node_index,
                            program_node_index,
                            context_indices,
                        } => {
                            json!({"state":"Linked","main_anchor_node_index":main_anchor_node_index,"program_node_index":program_node_index,"context_indices":context_indices})
                        }
                        other => json!({"state":format!("{other:?}")}),
                    };
                    occurrences.push(json!({"occurrence":occurrence_json(index,&found.occurrence)?,"scope":scope}));
                    if *found.occurrence.query.r#type().value()
                        == crate::profiles::QueryKind::Relation
                    {
                        outcomes.push(observed_relation(
                            GenericAntlrTree::CobolPreprocessor,
                            index,
                            &found.occurrence,
                            main,
                            scope,
                        )?);
                    }
                }
                diagnostics["preprocessor"] = json!({"tree":GenericAntlrTree::CobolPreprocessor,"grammar":document.preprocessor.grammar(),"syntax":syntax_json(&document.preprocessor.syntax,original)?,"occurrences":occurrences});
            }
            GenericAntlrObservations::PostgreSql(document) => {
                let mut bodies = Vec::new();
                for (body_index, body) in document.bodies.iter().enumerate() {
                    original.check()?;
                    let tree = GenericAntlrTree::PostgreSqlBody(body_index);
                    let mut matches = Vec::new();
                    for scoped in &body.matches {
                        original.check()?;
                        let group = &body.observations.groups[scoped.query_index];
                        let found = &group.matches[scoped.match_index];
                        let captures=found.captures.iter().enumerate().map(|(slot,capture)| {
                            original.check()?;
                            Ok(json!({"node_index":scoped.capture_nodes[slot],"capture_name":capture.native.name,"prepared":range_json(capture.prepared)?,"original":origin_json(&capture.original)?}))
                        }).collect::<Result<Vec<_>,AstGraphError>>()?;
                        let observation = json!({"tree":tree,"query_index":scoped.query_index,"match_index":scoped.match_index,"policy":query_json(group.query.value()),
                            "captures":captures,"bindings":format!("{:?}",found.bindings),"properties":found.properties.iter().map(|p|json!({"key":p.key,"value":p.value,"capture_id":p.capture_id})).collect::<Vec<_>>(),"host_contexts":body.host_contexts});
                        if *group.query.value().r#type().value()
                            == crate::profiles::QueryKind::Relation
                        {
                            outcomes.push(AstGraphRelationOutcome {
                                relation_index: outcomes.len(),
                                source_index: None,
                                capture_index: None,
                                evidence_node_indices: scoped.capture_nodes.clone(),
                                target: AstGraphRelationTarget::ObservedOnly,
                                observation_json: serde_json::to_string(&observation)?,
                            });
                        }
                        matches.push(observation);
                    }
                    let decisions=body.decisions.iter().map(|decision| {
                        original.check()?;
                        Ok(json!({"node_index":decision.node_index,"prepared":range_json(decision.prepared)?,"original":origin_json(&decision.original)?}))
                    }).collect::<Result<Vec<_>,AstGraphError>>()?;
                    let mut nodes = Vec::new();
                    for (index, n) in body.observations.nodes.iter().enumerate() {
                        original.check()?;
                        nodes.push(json!({"node_index":index,"kind":n.kind,"range":n.range,"parent":n.parent,"error":n.error,"missing":n.missing,"field":n.field,"named":n.named}));
                    }
                    bodies.push(json!({"tree":tree,"nodes":nodes,"matches":matches,"host_contexts":body.host_contexts,"decisions":decisions}));
                }
                diagnostics["bodies"] = json!(bodies);
            }
        }
        for (index, outcome) in outcomes.iter_mut().enumerate() {
            original.check()?;
            outcome.relation_index = index;
        }
        let diagnostics_json = serde_json::to_string(&diagnostics)?;
        let capabilities_json = serde_json::to_string(
            &json!({"model":"antlr_generic","exact_entities":true,"native_ancestry_contains":true,"relation_resolution":"ObservedOnly","tree_qualified_ordinals":true,"decision_observations":true,"computed_complexity":false}),
        )?;
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
