use super::*;
use crate::{
    profiles::QueryKind,
    semantic_model::{
        JsonContribution, JsonEntityExtent, JsonLiteral, JsonSemanticDocument, JsonValueObservation,
    },
};

pub struct AstGraphJsonInput<'a, 'doc, 'profile, 'o, 'v, 'request> {
    pub document: &'a JsonSemanticDocument<'doc, 'profile, 'o, 'v, 'request>,
    pub source: NativeNnSource<'a>,
}
fn literal_json(literal: &JsonLiteral<'_>) -> Result<Value, AstGraphError> {
    let raw = literal.raw();
    let value = match literal {
        JsonLiteral::String { decoded, .. } => {
            json!({"kind":"String","decoded_value":decoded.as_ref()})
        }
        JsonLiteral::Number { decoded, .. } => {
            json!({"kind":"Number","validated_number_lexeme":decoded.get()})
        }
        JsonLiteral::Bool { decoded, .. } => json!({"kind":"Bool","decoded_value":decoded}),
        JsonLiteral::Null { .. } => json!({"kind":"Null","decoded_value":Value::Null}),
    };
    Ok(json!({"literal_source":raw.text(),"range":range_json(raw)?,"value":value}))
}
fn contribution_json(c: &JsonContribution<'_, '_, '_>) -> Result<Value, AstGraphError> {
    let value = match &c.declared_value {
        JsonValueObservation::NotConfigured => json!({"status":"NotConfigured"}),
        JsonValueObservation::Missing => json!({"status":"Missing"}),
        JsonValueObservation::Unavailable => json!({"status":"Unavailable"}),
        JsonValueObservation::Present(value) => {
            json!({"status":"Present","literal":literal_json(value)?})
        }
    };
    Ok(
        json!({"query_index":c.occurrence.query_index,"match_index":c.occurrence.match_index,
        "name_slot":c.occurrence.name_slot,"value_slot":c.occurrence.value_slot,
        "capture_name":c.name_capture.native.name,
        "name_original":origin_json(&c.name_capture.original)?,"extent":extent_json(&c.extent)?,
        "declared_value":value}),
    )
}

/// Consume the active genuine JSON model without collecting or parsing its Source again.
pub fn assemble_graph_json(
    input: AstGraphJsonInput<'_, '_, '_, '_, '_, '_>,
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
        for entity in document.entities() {
            original.check()?;
            let uid = entity.identity().uid().as_str();
            let key = serde_json::to_string(&[uid, entity.data_key()])?;
            let mut properties = BTreeMap::new();
            for (key, value) in [
                ("uid", uid),
                ("data_key", entity.data_key()),
                ("name", entity.name()),
                ("raw_name", entity.name_raw().text()),
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
                AstGraphValue::Int64(line(entity.name_raw().start_point().row)?),
            );
            properties.insert(
                "name_representation".into(),
                AstGraphValue::String("JsonName".into()),
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
                JsonEntityExtent::Exact(e) => {
                    add_range(&mut properties, "", e.range())?;
                    "Exact"
                }
                JsonEntityExtent::NonExact(_) => "NonExact",
                JsonEntityExtent::Missing => "Missing",
                JsonEntityExtent::Conflict(_) => "Conflict",
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
                            .all(|n| matches!(n.extent, ExtentObservation::Exact(_))),
                ),
            );
            let mut contributions = Vec::new();
            for c in entity.contributions() {
                let mut value = contribution_json(c)?;
                let group = observations.groups.get(c.occurrence.query_index).ok_or(
                    AstGraphError::NativeContract("JSON contribution references foreign query"),
                )?;
                let matched = group.matches.get(c.occurrence.match_index).ok_or(
                    AstGraphError::NativeContract("JSON contribution references foreign match"),
                )?;
                value["query_policy"] = query_json(group.query.value());
                value["query_capture_names"] = json!(group.capture_names());
                value["query_properties"] = json!(
                    matched
                        .properties
                        .iter()
                        .map(|p| json!({"key":p.key,"value":p.value,"capture_id":p.capture_id}))
                        .collect::<Vec<_>>()
                );
                contributions.push(value);
            }
            let mut namespaces = Vec::new();
            for n in entity.namespace() {
                namespaces.push(json!({"node_index":n.node_index,"label":n.label,"name":n.name.as_ref(),"anchor":range_json(n.anchor)?,"extent":extent_json(&n.extent)?}));
            }
            properties.insert("metadata_json".into(),AstGraphValue::String(serde_json::to_string(&json!({"diagnostic_indices":entity.diagnostic_indices(),"conflicts":format!("{:?}",entity.conflicts()),"contributions":contributions,"namespaces":namespaces}))?));
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
        for d in document.diagnostics() {
            original.check()?;
            diagnostics.push(json!({"query_index":d.occurrence.query_index,"match_index":d.occurrence.match_index,
                "name_slot":d.occurrence.name_slot,"value_slot":d.occurrence.value_slot,
                "issue":format!("{:?}",d.issue),"original":d.original.map(origin_json).transpose()?}));
        }
        let mut capabilities = Vec::new();
        let mut outcomes = Vec::new();
        for (query_index, group) in observations.groups.iter().enumerate() {
            original.check()?;
            capabilities.push(json!({"query_index":query_index,"policy":query_json(group.query.value()),"adapter":"JsonSemanticDocument","query_kind":format!("{:?}",group.query.value().r#type().value()),"matches":group.matches.len()}));
            if *group.query.value().r#type().value() != QueryKind::Relation {
                continue;
            }
            let relation_type = group.query.value().relation_type().value();
            let target_rule = profile
                .target_rules()
                .value()
                .iter()
                .find(|(key, _)| key.value() == relation_type)
                .map(|(_, value)| value);
            for (match_index, matched) in group.matches.iter().enumerate() {
                let mut captures = Vec::new();
                for (capture_index, capture) in matched.captures.iter().enumerate() {
                    captures.push(json!({"capture_index":capture_index,"name":capture.native.name,"prepared":range_json(capture.prepared)?,"original":origin_json(&capture.original)?}));
                }
                outcomes.push(AstGraphRelationOutcome{relation_index:outcomes.len(),source_index:None,capture_index:None,evidence_node_indices:Vec::new(),target:AstGraphRelationTarget::ObservedOnly,
                    observation_json:serde_json::to_string(&json!({"query_index":query_index,"match_index":match_index,"data_key":group.query.value().data_key().value(),"relation_type":relation_type,"target_rule":target_rule.map(|rule|json!({"labels":rule.value().labels().value().iter().map(|s|s.value()).collect::<Vec<_>>(),"fallback":rule.value().fallback().value()})),"policy":query_json(group.query.value()),"capture_names":group.capture_names(),"captures":captures,"properties":matched.properties.iter().map(|p|json!({"key":p.key,"value":p.value,"capture_id":p.capture_id})).collect::<Vec<_>>(),"source_state":"UnsupportedJsonRelationQuery"}))?});
            }
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
            edges: Vec::new(),
            outcomes,
            diagnostics_json,
            capabilities_json,
        })
    })();
    original.check()?;
    result
}
