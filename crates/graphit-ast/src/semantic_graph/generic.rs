use super::*;
use crate::tree_sitter_generic_model::{
    GenericComplexity, GenericEmbeddedBlock, GenericExport, GenericExtent, GenericRelationTarget,
    GenericSemanticDocument,
};

pub struct AstGraphGenericInput<'a, 'doc, 'profile, 'o, 'v, 'request> {
    pub document: &'a GenericSemanticDocument<'doc, 'profile, 'o, 'v, 'request>,
    pub source: NativeNnSource<'a>,
}

/// Consume native semantics while its parse, input mapping and identity owner are alive.
pub fn assemble_graph_generic(
    input: AstGraphGenericInput<'_, '_, '_, '_, '_, '_>,
    original: &Arc<SnapshotContext>,
) -> Result<AstSemanticGraphBatch, AstGraphError> {
    original.check()?;
    let result = (|| {
        let document = input.document;
        if !std::ptr::eq(document.original_context()?, original.as_ref()) {
            return Err(NativeNnError::RequestMismatch.into());
        }
        let identity = document.identity_document()?;
        let source = document.source()?;
        let (path, hash) = associated_source(&input.source, identity, source.as_str(), original)?;
        let empty = source.as_str().is_empty();
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
        let observations = document.observations()?;
        let profile = observations.profile;
        let mut nodes = Vec::new();
        for (entity_index, entity) in document.entities()?.iter().enumerate() {
            original.check()?;
            let uid = entity.identity().uid().as_str();
            let name = entity.name();
            let mut properties = BTreeMap::new();
            for (key, value) in [
                ("uid", uid),
                ("data_key", entity.data_key()),
                ("name", name.display()),
                ("raw_name", name.raw().text()),
                ("path", path.as_str()),
                ("lang", profile.language().value().as_str()),
                ("grammar", profile.grammar().value().as_str()),
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
                AstGraphValue::Int64(line(name.raw().start_point().row)?),
            );
            properties.insert(
                "name_representation".into(),
                AstGraphValue::String(format!("{:?}", name.representation())),
            );
            properties.insert(
                "source_stored".into(),
                AstGraphValue::Bool(matches!(
                    source_observation,
                    AstGraphSourceObservation::IndexedStored { .. }
                )),
            );
            properties.insert("source_empty".into(), AstGraphValue::Bool(empty));
            add_range(&mut properties, "name_", name.raw())?;
            add_range(&mut properties, "display_", name.display_range())?;
            let extent_status = if let Some(extent) = entity.extent() {
                add_range(&mut properties, "", extent.range())?;
                "Exact"
            } else if entity.conflicts().is_empty() {
                "Unavailable"
            } else {
                "Conflict"
            };
            properties.insert(
                "extent_status".into(),
                AstGraphValue::String(extent_status.into()),
            );
            properties.insert(
                "available".into(),
                AstGraphValue::Bool(extent_status == "Exact" && entity.conflicts().is_empty()),
            );
            if let Some(value) = entity.value() {
                properties.insert("value".into(), AstGraphValue::String(value.text().into()));
            }
            if let GenericExport::Decision { exported, .. } = &document.exports()?[entity_index] {
                properties.insert("is_exported".into(), AstGraphValue::Bool(*exported));
            }
            if let GenericComplexity::Score { value, .. } = &document.complexities()?[entity_index]
            {
                properties.insert(
                    "cyclomatic_complexity".into(),
                    AstGraphValue::Int64(integer(*value)?),
                );
            }
            let mut contributions = Vec::new();
            for contribution in entity.contributions() {
                let occurrence = contribution.occurrence;
                let group = &observations.groups[occurrence.query_index];
                let matched = &group.matches[occurrence.match_index];
                let mut captures = Vec::new();
                for (slot, capture) in matched.captures.iter().enumerate() {
                    captures.push(json!({"slot":slot,"name":capture.native.name,"node_id":capture.native.node_id,"prepared":range_json(capture.prepared)?,"original":origin_json(&capture.original)?}));
                }
                let extent = match &contribution.extent {
                    GenericExtent::Exact {
                        extent,
                        node_index,
                        capture_indices,
                    } => {
                        json!({"status":"Exact","range":range_json(extent.range())?,"node_index":node_index,"capture_indices":capture_indices})
                    }
                    GenericExtent::Missing { configured } => {
                        json!({"status":"Missing","configured":configured})
                    }
                    GenericExtent::NonExact { capture_indices } => {
                        json!({"status":"NonExact","capture_indices":capture_indices})
                    }
                    GenericExtent::Conflict { capture_indices } => {
                        json!({"status":"Conflict","capture_indices":capture_indices})
                    }
                    GenericExtent::DoesNotContainName { capture_indices } => {
                        json!({"status":"DoesNotContainName","capture_indices":capture_indices})
                    }
                    GenericExtent::Recovery {
                        capture_index,
                        node_index,
                    } => {
                        json!({"status":"Recovery","capture_index":capture_index,"node_index":node_index})
                    }
                };
                contributions.push(json!({"query_index":occurrence.query_index,"match_index":occurrence.match_index,"name_slot":occurrence.name_slot,"value_slot":occurrence.value_slot,
                    "query_policy":query_json(group.query.value()),"capture_names":group.capture_names(),"captures":captures,"extent":extent,"declaration":format!("{:?}",contribution.declaration),
                    "values":contribution.values.iter().map(|value|range_json(*value)).collect::<Result<Vec<_>,_>>()?,
                    "properties":matched.properties.iter().map(|p|json!({"key":p.key,"value":p.value,"capture_id":p.capture_id})).collect::<Vec<_>>()}));
            }
            properties.insert("metadata_json".into(), AstGraphValue::String(serde_json::to_string(&json!({"contributions":contributions,"conflicts":format!("{:?}",entity.conflicts()),"export":format!("{:?}",document.exports()?[entity_index]),"complexity":format!("{:?}",document.complexities()?[entity_index]),"owners":entity.owners()}))?));
            if let Some(branch) = association.physical_branch() {
                properties.insert(
                    "physical_branch".into(),
                    AstGraphValue::String(branch.into()),
                );
            }
            if let Some(target) = association.inherited_target() {
                properties.insert(
                    "inherited_target".into(),
                    AstGraphValue::String(target.into()),
                );
            }
            nodes.push(AstGraphNode {
                label: entity.identity().label().into(),
                key: serde_json::to_string(&[uid, entity.data_key()])?,
                properties,
            });
        }
        let mut edges = Vec::new();
        let mut edge_keys = BTreeSet::new();
        let mut push_edge = |kind: &str,
                             from: usize,
                             to: usize,
                             provenance: Value|
         -> Result<String, AstGraphError> {
            let source_key = node_key(&nodes, from)?;
            let target_key = node_key(&nodes, to)?;
            let uid = serde_json::to_string(&(kind, source_key, target_key, provenance))?;
            if edge_keys.insert(uid.clone()) {
                edges.push(AstGraphEdge {
                    kind: kind.into(),
                    source_key: source_key.into(),
                    target_key: target_key.into(),
                    uid: uid.clone(),
                });
            }
            Ok(uid)
        };
        for (index, entity) in document.entities()?.iter().enumerate() {
            if let Some(&parent) = entity.owners().last() {
                push_edge("CONTAINS", parent, index, json!({"native_owner":true}))?;
            }
        }
        let mut outcomes = Vec::new();
        let mut effective_labels = BTreeMap::<String, Vec<String>>::new();
        for (relation_index, relation) in document.relations()?.iter().enumerate() {
            original.check()?;
            let mut graph_edge_uids = Vec::new();
            let target = match &relation.target {
                GenericRelationTarget::Resolved { entity_index } => {
                    if let Some(source) = relation.source_index {
                        graph_edge_uids.push(push_edge(
                            &relation.kind,
                            source,
                            *entity_index,
                            json!({"query":relation.occurrence.query_index,"match":relation.occurrence.match_index,"slot":relation.occurrence.name_slot}),
                        )?);
                    }
                    AstGraphRelationTarget::Resolved {
                        key: node_key(&nodes, *entity_index)?.into(),
                    }
                }
                GenericRelationTarget::NotFound => AstGraphRelationTarget::NotFound,
                GenericRelationTarget::Ambiguous { entity_indices } => {
                    AstGraphRelationTarget::Ambiguous {
                        keys: entity_indices
                            .iter()
                            .map(|&index| node_key(&nodes, index).map(str::to_owned))
                            .collect::<Result<Vec<_>, _>>()?,
                    }
                }
                GenericRelationTarget::Unresolved { reason } => {
                    AstGraphRelationTarget::GenericUnresolved {
                        reason: (*reason).into(),
                    }
                }
            };
            let target_labels = effective_labels
                .entry(relation.kind.clone())
                .or_insert_with(|| {
                    let mut labels = Vec::new();
                    for group in &observations.groups {
                        let query = group.query.value();
                        if *query.r#type().value() == crate::profiles::QueryKind::Relation
                            && query.relation_type().value() == &relation.kind
                        {
                            for label in query.target_labels().value().iter() {
                                if !labels.contains(label.value()) {
                                    labels.push(label.value().clone());
                                }
                            }
                        }
                    }
                    if let Some((_, rule)) = profile
                        .target_rules()
                        .value()
                        .iter()
                        .find(|(kind, _)| kind.value() == &relation.kind)
                    {
                        for label in rule.value().labels().value().iter() {
                            if !labels.contains(label.value()) {
                                labels.push(label.value().clone());
                            }
                        }
                    }
                    if labels.is_empty() {
                        for group in &observations.groups {
                            let query = group.query.value();
                            if *query.r#type().value() != crate::profiles::QueryKind::Entity {
                                continue;
                            }
                            for label in [query.graph_label().value(), query.value_label().value()]
                            {
                                if !label.is_empty() && !labels.contains(label) {
                                    labels.push(label.clone());
                                }
                            }
                        }
                    }
                    labels
                });
            let occurrence = relation.occurrence;
            let capture = &observations.groups[occurrence.query_index].matches
                [occurrence.match_index]
                .captures[occurrence.name_slot];
            outcomes.push(AstGraphRelationOutcome { relation_index, source_index: relation.source_index, capture_index: Some(occurrence.name_slot), evidence_node_indices: relation.evidence_node_indices.clone(), target,
                observation_json: serde_json::to_string(&json!({"query_index":occurrence.query_index,"match_index":occurrence.match_index,"name_slot":occurrence.name_slot,"kind":relation.kind,"original":origin_json(&capture.original)?,"graph_edge_uids":graph_edge_uids,"target_rule":{"labels":target_labels}}))? });
        }
        let mut diagnostics = document.diagnostics()?.iter().map(|diagnostic| json!({"occurrence":diagnostic.occurrence.map(|o|json!({"query_index":o.query_index,"match_index":o.match_index,"name_slot":o.name_slot,"value_slot":o.value_slot})),"context_index":diagnostic.context_index,"issue":format!("{:?}",diagnostic.issue)})).collect::<Vec<_>>();
        for recovery in document.syntax_recoveries()? {
            original.check()?;
            diagnostics.push(json!({"syntax_recovery":{"node_index":recovery.node_index,"node_id":recovery.node_id,"kind":recovery.kind,"error":recovery.error,"missing":recovery.missing,"prepared":{"start":recovery.prepared.start,"end":recovery.prepared.end},"original":origin_json(&recovery.original)?}}));
        }
        let diagnostics_json = serde_json::to_string(&diagnostics)?;
        let capabilities_json = serde_json::to_string(&observations.groups.iter().enumerate().map(|(index,group)|json!({"query_index":index,"adapter":"GenericSemanticDocument","query_kind":format!("{:?}",group.query.value().r#type().value()),"matches":group.matches.len(),"policy":query_json(group.query.value())})).collect::<Vec<_>>())?;
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

fn refresh_embedded_file(
    batch: &mut AstSemanticGraphBatch,
    original: &SnapshotContext,
) -> Result<(), AstGraphError> {
    let Some(AstGraphValue::String(language)) = batch.file_observation.properties.get("lang")
    else {
        return Err(AstGraphError::NativeContract(
            "embedded host FILE has no language",
        ));
    };
    let Some(AstGraphValue::String(grammar)) = batch.file_observation.properties.get("grammar")
    else {
        return Err(AstGraphError::NativeContract(
            "embedded host FILE has no grammar",
        ));
    };
    batch.file_observation = file_observation_node(
        &batch.association,
        language,
        grammar,
        &batch.outcomes,
        &batch.diagnostics_json,
        &batch.capabilities_json,
        original,
    )?;
    Ok(())
}

/// One native host composition: accumulate observations and encode FILE once at finish.
pub struct AstGraphEmbeddedComposition {
    batch: AstSemanticGraphBatch,
    diagnostics: Vec<Value>,
    capabilities: Vec<Value>,
}

impl AstGraphEmbeddedComposition {
    pub fn new(
        batch: AstSemanticGraphBatch,
        original: &SnapshotContext,
    ) -> Result<Self, AstGraphError> {
        original.check()?;
        let diagnostics = serde_json::from_str(&batch.diagnostics_json)?;
        let capabilities = serde_json::from_str(&batch.capabilities_json)?;
        original.check()?;
        Ok(Self {
            batch,
            diagnostics,
            capabilities,
        })
    }

    pub fn finish(
        mut self,
        original: &SnapshotContext,
    ) -> Result<AstSemanticGraphBatch, AstGraphError> {
        original.check()?;
        self.batch.diagnostics_json = serde_json::to_string(&self.diagnostics)?;
        self.batch.capabilities_json = serde_json::to_string(&self.capabilities)?;
        refresh_embedded_file(&mut self.batch, original)?;
        original.check()?;
        Ok(self.batch)
    }
}

/// Accumulate an observed embedded outcome, including unsuccessful subparses.
pub fn observe_graph_embedded(
    composition: &mut AstGraphEmbeddedComposition,
    observation: Value,
    original: &SnapshotContext,
) -> Result<(), AstGraphError> {
    original.check()?;
    composition
        .diagnostics
        .push(json!({"embedded":observation}));
    Ok(())
}

/// Merge owned rows only after both models proved the same genuine Source.
pub fn merge_graph_embedded(
    host: &mut AstGraphEmbeddedComposition,
    mut child: AstSemanticGraphBatch,
    observation: Value,
    original: &SnapshotContext,
) -> Result<(), AstGraphError> {
    original.check()?;
    if host.batch.association != child.association {
        return Err(AstGraphError::NativeContract(
            "embedded Graph belongs to a different Source association",
        ));
    }
    for outcome in &mut child.outcomes {
        original.check()?;
        // These ordinals address the retained child model, not the merged node vector.
        let native: Value = serde_json::from_str(&outcome.observation_json)?;
        outcome.observation_json =
            serde_json::to_string(&json!({"embedded":observation,"native":native}))?;
    }
    host.batch.nodes.extend(child.nodes);
    host.batch.edges.extend(child.edges);
    host.batch.outcomes.extend(child.outcomes);
    host.capabilities.push(json!({"embedded":observation,"capabilities":serde_json::from_str::<Value>(&child.capabilities_json)?}));
    let child_diagnostics: Value = serde_json::from_str(&child.diagnostics_json)?;
    observe_graph_embedded(
        host,
        json!({"state":"Merged","selection":observation,"diagnostics":child_diagnostics}),
        original,
    )
}

/// Attribute only source-less resolved child relations to an actual containing native host.
pub fn bind_graph_generic_embedded_host(
    host: &GenericSemanticDocument<'_, '_, '_, '_, '_>,
    block: &GenericEmbeddedBlock<'_, '_, '_>,
    child: &GenericSemanticDocument<'_, '_, '_, '_, '_>,
    composition: &mut AstGraphEmbeddedComposition,
    original: &SnapshotContext,
) -> Result<(), AstGraphError> {
    original.check()?;
    if !std::ptr::eq(host.original_context()?, original)
        || !std::ptr::eq(child.original_context()?, original)
        || host.source()?.as_str().as_ptr() != child.source()?.as_str().as_ptr()
        || host.source()?.as_str().len() != child.source()?.as_str().len()
    {
        return Err(NativeNnError::RequestMismatch.into());
    }
    let observations = host.observations()?;
    let body_index = observations
        .nodes
        .iter()
        .position(|node| {
            node.node_id == block.body.node_id
                && node.range.start == block.body.prepared.start()
                && node.range.end == block.body.prepared.end()
        })
        .ok_or(AstGraphError::NativeContract(
            "embedded body is outside its retained host",
        ))?;
    let labels = block.descriptor.host_labels().value();
    let mut cursor = Some(body_index);
    let mut selected = Vec::new();
    while let Some(node_index) = cursor {
        original.check()?;
        for (index, entity) in host.entities()?.iter().enumerate() {
            if !labels.is_empty()
                && !labels
                    .iter()
                    .any(|label| label.value() == entity.identity().label())
            {
                continue;
            }
            if !entity.conflicts().is_empty() {
                continue;
            }
            if entity.contributions().iter().any(|contribution| matches!(contribution.extent, GenericExtent::Exact { node_index: extent_node, .. } if extent_node == node_index)) { selected.push(index); }
        }
        if !selected.is_empty() {
            break;
        }
        cursor = observations.nodes[node_index].parent;
    }
    let [host_index] = selected.as_slice() else {
        observe_graph_embedded(
            composition,
            json!({"state":if selected.is_empty(){"HostNotFound"}else{"HostAmbiguous"},"entity_indices":selected,"embedded_index":block.embedded_index,"match_index":block.match_index,"body_slot":block.body_slot}),
            original,
        )?;
        return Ok(());
    };
    let entity = &host.entities()?[*host_index];
    let source_key = serde_json::to_string(&[entity.identity().uid().as_str(), entity.data_key()])?;
    let batch = &mut composition.batch;
    for (relation_index, relation) in child.relations()?.iter().enumerate() {
        if relation.source_index.is_some() {
            continue;
        }
        let GenericRelationTarget::Resolved { entity_index } = relation.target else {
            continue;
        };
        let target_key = node_key(&batch.nodes, entity_index)?.to_owned();
        let uid = serde_json::to_string(&(
            &relation.kind,
            &source_key,
            &target_key,
            block.embedded_index,
            block.match_index,
            block.body_slot,
            relation_index,
        ))?;
        batch.edges.push(AstGraphEdge {
            kind: relation.kind.clone(),
            source_key: source_key.clone(),
            target_key,
            uid,
        });
        // The source lives in the host batch; keep its key in observation rather
        // than fabricating an entity index in the child batch.
        let outcome =
            batch
                .outcomes
                .get_mut(relation_index)
                .ok_or(AstGraphError::NativeContract(
                    "embedded native relation has no Graph outcome",
                ))?;
        let native: Value = serde_json::from_str(&outcome.observation_json)?;
        outcome.observation_json =
            serde_json::to_string(&json!({"native":native,"embedded_host_key":source_key}))?;
    }
    original.check()?;
    Ok(())
}
