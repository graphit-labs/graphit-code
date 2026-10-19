//! Configured Oracle object targets; Root supplies candidates from same-job staging.
use super::*;
use crate::antlr_engine::model::{AntlrSemanticDocument, oracle_targets};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AstOracleGraphRows {
    endpoints: Vec<AstOracleGraphEndpoint>,
    references: Vec<AstOracleGraphReference>,
}
impl AstOracleGraphRows {
    pub fn endpoints(&self) -> &[AstOracleGraphEndpoint] {
        &self.endpoints
    }
    pub fn references(&self) -> &[AstOracleGraphReference] {
        &self.references
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AstOracleGraphEndpoint {
    association: AstGraphAssociation,
    name: oracle_targets::ObjectName,
    lookup_key: String,
    key: String,
    label: String,
    available: bool,
}
impl AstOracleGraphEndpoint {
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn lookup_key(&self) -> &str {
        &self.lookup_key
    }
    pub fn association(&self) -> &AstGraphAssociation {
        &self.association
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AstOracleGraphReference {
    association: AstGraphAssociation,
    name: Option<oracle_targets::ObjectName>,
    lookup_key: Option<String>,
    source_key: Option<String>,
    labels: Vec<String>,
    kind: String,
    data_key: String,
    anchor: Option<(usize, usize)>,
    issue: Option<String>,
    outcome: AstGraphRelationOutcome,
}
impl AstOracleGraphReference {
    pub fn source_key(&self) -> Option<&str> {
        self.source_key.as_deref()
    }
    pub fn kind(&self) -> &str {
        &self.kind
    }
    pub fn outcome(&self) -> &AstGraphRelationOutcome {
        &self.outcome
    }
    pub fn lookup_key(&self) -> Option<&str> {
        self.lookup_key.as_deref()
    }
    pub fn association(&self) -> &AstGraphAssociation {
        &self.association
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AstOracleGraphResolution {
    association: AstGraphAssociation,
    prior: AstGraphRelationOutcome,
    outcome: AstGraphRelationOutcome,
    edges: Vec<AstGraphEdge>,
}
impl AstOracleGraphResolution {
    pub fn outcome(&self) -> &AstGraphRelationOutcome {
        &self.outcome
    }
    pub fn edges(&self) -> &[AstGraphEdge] {
        &self.edges
    }
    pub fn into_parts(self) -> (Vec<AstGraphEdge>, AstGraphRelationOutcome) {
        (self.edges, self.outcome)
    }
}

/// Collect before dropping the native document. No public DTO reconstructs a model.
pub fn collect_oracle_graph_rows(
    document: &AntlrSemanticDocument<'_, '_, '_, '_, '_>,
    batch: &AstSemanticGraphBatch,
    original: &Arc<SnapshotContext>,
) -> Result<AstOracleGraphRows, AstGraphError> {
    original.check()?;
    let p = document.parsed();
    if !std::ptr::eq(p.original_context(), original.as_ref()) {
        return Err(NativeNnError::RequestMismatch.into());
    }
    let identity = p.identity_document();
    if batch
        .association
        .relative_components
        .iter()
        .map(String::as_str)
        .ne(identity.relative_components().iter().copied())
        || batch.association.source_sha256
            != <[u8; 32]>::from(Sha256::digest(p.source().as_bytes()))
        || document.entities().len() != batch.nodes.len()
    {
        return Err(AstGraphError::NativeContract(
            "Oracle rows differ from retained Source",
        ));
    }
    for (entity, node) in document.entities().iter().zip(&batch.nodes) {
        original.check()?;
        if node.key
            != serde_json::to_string(&[entity.identity().uid().as_str(), entity.data_key()])?
            || node.label != entity.identity().label()
        {
            return Err(AstGraphError::NativeContract(
                "Oracle rows differ from native entity",
            ));
        }
    }
    let mut rows = AstOracleGraphRows {
        endpoints: Vec::new(),
        references: Vec::new(),
    };
    if p.observations().profile.grammar().value() != "antlr-plsql" {
        return Ok(rows);
    }
    for (i, node) in batch.nodes.iter().enumerate() {
        original.check()?;
        if let Ok(name) = oracle_targets::endpoint(document, i)
            && let Ok(lookup_key) = oracle_targets::key(&name)
        {
            rows.endpoints.push(AstOracleGraphEndpoint {
                association: batch.association.clone(),
                name,
                lookup_key,
                key: node.key.clone(),
                label: node.label.clone(),
                available: node.properties.get("available") == Some(&AstGraphValue::Bool(true)),
            });
        }
    }
    for outcome in &batch.outcomes {
        original.check()?;
        if !matches!(outcome.target, AstGraphRelationTarget::ObservedOnly) {
            continue;
        }
        let occurrence = p
            .observations()
            .occurrences
            .get(outcome.relation_index)
            .ok_or(AstGraphError::NativeContract(
                "Oracle outcome lacks occurrence",
            ))?;

        let crate::antlr_engine::semantic::CaptureBinding::Present(native_name) = &occurrence.name
        else {
            continue;
        };
        if p.observations().syntax.nodes[native_name.node_index].kind != "tableview_name" {
            continue;
        }
        let kind = occurrence.query.relation_type().value();
        if kind == "CALLS" {
            continue;
        }

        let rule = p
            .observations()
            .profile
            .target_rules()
            .value()
            .iter()
            .find(|(key, _)| key.value() == kind)
            .map(|(_, rule)| rule);
        let mut labels = BTreeSet::new();
        if let Some(rule) = rule {
            labels.extend(
                rule.value()
                    .labels()
                    .value()
                    .iter()
                    .map(|s| s.value().clone()),
            );
        }
        for entry in p.observations().profile.queries().value().iter() {
            let query = entry.value();
            if query.relation_type().value() == kind {
                labels.extend(
                    query
                        .target_labels()
                        .value()
                        .iter()
                        .map(|s| s.value().clone()),
                );
            }
        }
        if rule.is_none() && labels.is_empty() {
            continue;
        }
        if rule.is_some_and(|r| r.value().labels().value().is_empty()) {
            for entry in p.observations().profile.queries().value().iter() {
                for label in [
                    entry.value().graph_label().value(),
                    entry.value().value_label().value(),
                    entry.value().parent_label().value(),
                ] {
                    if !label.is_empty() {
                        labels.insert(label.clone());
                    }
                }
            }
        }
        let labels = labels.into_iter().collect();
        let mut reference = AstOracleGraphReference {
            association: batch.association.clone(),
            name: None,
            lookup_key: None,
            source_key: None,
            labels,
            kind: kind.clone(),
            data_key: occurrence.query.data_key().value().clone(),
            anchor: None,
            issue: None,
            outcome: outcome.clone(),
        };
        if let crate::antlr_engine::semantic::CaptureBinding::Present(capture) = &occurrence.name
            && let Some(OriginalProjection::Exact(anchor)) = &capture.original
        {
            reference.anchor = Some((anchor.start(), anchor.end()));
        }
        match oracle_targets::reference(document, outcome.relation_index) {
            Ok(proof) => {
                let node = batch
                    .nodes
                    .get(proof.source)
                    .ok_or(AstGraphError::NativeContract("Oracle source entity absent"))?;
                if node.properties.get("available") == Some(&AstGraphValue::Bool(true)) {
                    reference.source_key = Some(node.key.clone());
                } else {
                    reference.issue = Some("UnavailableSource".into());
                }
                match oracle_targets::key(&proof.name) {
                    Ok(key) => reference.lookup_key = Some(key),
                    Err(issue) => reference.issue = Some(format!("{issue:?}")),
                }
                reference.name = Some(proof.name);
            }
            Err(issue) => reference.issue = Some(format!("{issue:?}")),
        }
        if reference.anchor.is_none() {
            reference.issue = Some("NonExactOrigin".into());
        }
        rows.references.push(reference);
    }
    original.check()?;
    Ok(rows)
}

/// Unique eligible native name within the authorized same-job candidate set.
/// This factory does not replace final Raw/physical freshness and project pins.
pub fn resolve_oracle_graph_reference(
    reference: &AstOracleGraphReference,
    candidates: &[AstOracleGraphEndpoint],
    original: &Arc<SnapshotContext>,
) -> Result<AstOracleGraphResolution, AstGraphError> {
    original.check()?;
    let mut outcome = reference.outcome.clone();
    if !matches!(outcome.target, AstGraphRelationTarget::ObservedOnly) {
        return Err(AstGraphError::NativeContract(
            "Oracle reference is not deferred",
        ));
    }
    let mut edges = Vec::new();
    let mut keys = BTreeSet::new();
    let mut unavailable = false;
    let mut target_candidates = Vec::new();
    let mut issue = reference.issue.clone();
    if let Some(name) = &reference.name
        && oracle_targets::key(name).ok().as_deref() != reference.lookup_key()
    {
        return Err(AstGraphError::NativeContract(
            "Oracle reference name key differs",
        ));
    }
    for candidate in candidates {
        original.check()?;
        if oracle_targets::key(&candidate.name).ok().as_deref() != Some(candidate.lookup_key()) {
            return Err(AstGraphError::NativeContract(
                "Oracle candidate name key differs",
            ));
        }
        if Some(candidate.lookup_key()) != reference.lookup_key()
            || !reference.labels.contains(&candidate.label)
        {
            continue;
        }
        // Cross-language/grammar guesses cannot enter this catalog.
        let uid: Value = serde_json::from_str(&candidate.key)?;
        if uid.as_array().is_none_or(|a| a.len() != 2) {
            return Err(AstGraphError::NativeContract(
                "Oracle candidate lacks entity tuple key",
            ));
        }
        target_candidates.push(candidate);
        if candidate.available {
            keys.insert(candidate.key.clone());
        } else {
            unavailable = true;
        }
    }
    if unavailable {
        issue = Some("UnavailableTarget".into());
    }
    if reference.source_key.is_none() || reference.anchor.is_none() {
        issue.get_or_insert_with(|| "UnavailableSource".into());
    }
    outcome.target = if let Some(reason) = issue {
        AstGraphRelationTarget::GenericUnresolved { reason }
    } else {
        match keys.len() {
            0 => AstGraphRelationTarget::NotFound,
            1 => {
                let target = keys.first().expect("one key");
                let source = reference
                    .source_key
                    .as_ref()
                    .ok_or(AstGraphError::NativeContract(
                        "Oracle resolved source absent",
                    ))?;
                let (start, end) = reference.anchor.ok_or(AstGraphError::NativeContract(
                    "Oracle resolved anchor absent",
                ))?;
                if start >= end {
                    return Err(AstGraphError::NativeContract(
                        "Oracle empty reference anchor",
                    ));
                }
                edges.push(AstGraphEdge {
                    kind: reference.kind.clone(),
                    source_key: source.clone(),
                    target_key: target.clone(),
                    uid: serde_json::to_string(&(
                        &reference.kind,
                        source,
                        target,
                        &reference.data_key,
                        start,
                        end,
                    ))?,
                });
                AstGraphRelationTarget::Resolved {
                    key: target.clone(),
                }
            }
            _ => AstGraphRelationTarget::Ambiguous {
                keys: keys.into_iter().collect(),
            },
        }
    };
    let mut observation: Value = serde_json::from_str(&outcome.observation_json)?;
    observation["oracle_target"] = json!({"name":reference.name,"source_key":reference.source_key,
        "anchor":reference.anchor,"labels":reference.labels,"state":"SameJobTargetRule","candidates":target_candidates});
    outcome.observation_json = serde_json::to_string(&observation)?;
    original.check()?;
    Ok(AstOracleGraphResolution {
        association: reference.association.clone(),
        prior: reference.outcome.clone(),
        outcome,
        edges,
    })
}

/// Update exactly the deferred occurrence in its genuine observational FILE.
pub fn apply_oracle_graph_resolution(
    file: &mut AstGraphNode,
    resolution: &AstOracleGraphResolution,
    original: &Arc<SnapshotContext>,
) -> Result<(), AstGraphError> {
    original.check()?;
    let association = &resolution.association;
    let string = |key: &str| match file.properties.get(key) {
        Some(AstGraphValue::String(s)) => Some(s.as_str()),
        _ => None,
    };
    let digest: String = association
        .source_sha256
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let expected_key = serde_json::to_string(&json!([
        "file_observation",
        association.path,
        [string("lang"), string("grammar")]
    ]))?;
    if file.properties.get("is_file_observation") != Some(&AstGraphValue::Bool(true))
        || string("lang") != Some("plsql")
        || string("grammar") != Some("antlr-plsql")
        || string("relative_components_json")
            != Some(serde_json::to_string(&association.relative_components)?.as_str())
        || file.key != expected_key
        || string("path") != Some(association.path.as_str())
        || string("raw_version") != Some(association.raw_version.to_string().as_str())
        || string("source_sha256") != Some(digest.as_str())
        || string("physical_branch") != association.physical_branch.as_deref()
        || string("inherited_target") != association.inherited_target.as_deref()
    {
        return Err(AstGraphError::NativeContract(
            "Oracle resolution belongs to another FILE",
        ));
    }
    let mut outcomes: Vec<Value> = serde_json::from_str(
        string("resolution_outcomes_json")
            .ok_or(AstGraphError::NativeContract("Oracle FILE lacks outcomes"))?,
    )?;
    let prior = &resolution.prior;
    let indices: Vec<_> = outcomes
        .iter()
        .enumerate()
        .filter(|(_, o)| {
            o["relation_index"].as_u64() == u64::try_from(prior.relation_index).ok()
                && o["source_index"].as_u64()
                    == prior.source_index.and_then(|i| u64::try_from(i).ok())
                && o["capture_index"].as_u64()
                    == prior.capture_index.and_then(|i| u64::try_from(i).ok())
        })
        .map(|(i, _)| i)
        .collect();
    let [i] = indices.as_slice() else {
        return Err(AstGraphError::NativeContract(
            "Oracle FILE occurrence is not unique",
        ));
    };
    if outcomes[*i]["target"] != json!({"state":"ObservedOnly"})
        || outcomes[*i]["observation"] != serde_json::from_str::<Value>(&prior.observation_json)?
    {
        return Err(AstGraphError::NativeContract(
            "Oracle FILE observation differs from deferred reference",
        ));
    }
    outcomes[*i]["target"] = match &resolution.outcome.target {
        AstGraphRelationTarget::Resolved { key } => json!({"state":"Resolved","key":key}),
        AstGraphRelationTarget::Ambiguous { keys } => json!({"state":"Ambiguous","keys":keys}),
        AstGraphRelationTarget::NotFound => json!({"state":"NotFound"}),
        AstGraphRelationTarget::GenericUnresolved { reason } => {
            json!({"state":"Unresolved","reason":reason})
        }
        _ => {
            return Err(AstGraphError::NativeContract(
                "Oracle resolution target is ineligible",
            ));
        }
    };
    outcomes[*i]["observation"] = serde_json::from_str(&resolution.outcome.observation_json)?;
    let encoded = serde_json::to_string(&outcomes)?;
    original.check()?;
    file.properties.insert(
        "resolution_outcomes_json".into(),
        AstGraphValue::String(encoded),
    );
    Ok(())
}
