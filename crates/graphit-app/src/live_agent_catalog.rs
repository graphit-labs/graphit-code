//! Live selection metadata. A catalogue entry never grants access to its source.
#![cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use crate::{
    knowledge_ask_service::AskAvailability,
    knowledge_live::{ArtifactSelection, ProjectSelection},
    reference_explorer::{self, Failure, Identity},
    task_server::TaskServer,
};
use axum::{
    Json,
    response::{IntoResponse, Response},
};
use graphit_auth::{
    AuthError, ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ScopeCapture,
    broker::project_access::{
        BrokerProjectAccess, BrokerProjectCaller, BrokerProjectError, BrokerProjectScope,
        BrokerRecordRead,
    },
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, StoreOwner},
    project_identity::ProjectIdentityStore,
};
use graphit_hub::{
    ArtifactType, InstallationTarget, NativeBinding, SourceRef,
    policy::RequestContext,
    registry::{HubScope, InstalledBackend, InstalledPayload},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Serialize)]
struct Catalog {
    entries: Vec<Entry>,
    errors: Vec<String>,
    projects: Vec<ProjectSelection>,
    bindings: Vec<Binding>,
}

#[derive(Serialize)]
struct Entry {
    id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
    #[serde(rename = "type")]
    artifact_type: String,
    #[serde(rename = "ref")]
    reference: ArtifactSelection,
    #[serde(rename = "sourceLabel")]
    source_label: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    unavailable: Option<&'static str>,
}

#[derive(Serialize)]
struct Binding {
    binding: NativeBinding,
    available: bool,
    model_selection: &'static str,
}

/// Registry namespace and complete provenance tuple; this is not a PROJECT grant.
pub(crate) fn selection(source: &SourceRef) -> ArtifactSelection {
    ArtifactSelection {
        source: "hub".into(),
        project_id: Some(source.producer().to_string()),
        instance_id: None,
        project_kind: None,
        artifact_type: source.artifact_type().as_str().into(),
        id: source.id().as_str().into(),
        version: source.version().as_str().into(),
    }
}

pub(crate) fn failure(error: Failure) -> Box<Response> {
    let (status, code) = error.response();
    Box::new((status, Json(serde_json::json!({"error":{"code":code}}))).into_response())
}

fn binding(binding: NativeBinding, ask: Option<&AskAvailability>) -> Binding {
    use graphit_ai::cli_kiro::KiroEngine;
    let engine = ask
        .and_then(AskAvailability::service)
        .and_then(|service| service.kiro_engine());
    Binding {
        binding,
        available: matches!(
            (binding, engine),
            (NativeBinding::KiroV3, Some(KiroEngine::V3))
        ),
        model_selection: "explicit",
    }
}

/// Read registry metadata and fresh, separately admitted PROJECT selections.
/// Encode once while the actual admissions and Source observations are retained.
pub(crate) async fn read<'a>(
    tasks: &'a TaskServer,
    source: Option<&'a crate::source_server::SourceServer>,
    identity: Identity<'a>,
    backend: &'a BrokerProjectAccess,
    original: &'a RequestContext<'a>,
) -> Result<Vec<u8>, Box<Response>> {
    let snapshot = tasks
        .record_snapshot()
        .map_err(|error| Box::new(error.into_response()))?;
    let resources = tasks.record_resources(&snapshot);
    resources.check(original).map_err(failure)?;
    let (caller, verifier, provider) = identity;
    let hub = resources
        .auth
        .issuer()
        .admit_web_installed_hub_read(
            caller,
            verifier,
            original,
            provider,
            resources.auth.installation(),
        )
        .map_err(|error| failure(error.into()))?;
    let knowledge = match resources.auth.issuer().admit_web_imported_knowledge_read(
        caller,
        verifier,
        original,
        provider,
        resources.auth.installation(),
    ) {
        Ok(admission) => Some(admission),
        Err(AuthError::Denied) => None,
        Err(error) => return Err(failure(error.into())),
    };
    let imported_ast = if source.is_some() {
        match resources.auth.issuer().admit_web_imported_ast_read(
            caller,
            verifier,
            original,
            provider,
            resources.auth.installation(),
        ) {
            Ok(admission) => Some(admission),
            Err(AuthError::Denied) => None,
            Err(error) => return Err(failure(error.into())),
        }
    } else {
        None
    };
    let ast_composed = source.is_some();
    let ast_server = source.map(crate::source_server::SourceServer::graph_server);
    let mut captures = Vec::with_capacity(resources.projects.len());
    for (index, selected) in resources.projects.iter().enumerate() {
        resources.check(original).map_err(failure)?;
        if !resources.is_active(selected).map_err(failure)? {
            continue;
        }
        let scope = CapturedPublicationScope::project(
            &selected.0,
            selected.1,
            resources.policy.scope,
            original.snapshot(),
        )
        .map_err(|error| failure(Failure::Physical(error)))?;
        let binding = scope
            .binding()
            .map_err(|error| failure(Failure::Physical(error)))?;
        captures.push((index, scope, binding));
    }
    let authority = ProjectRecordAuthority::new(resources.auth.issuer());
    let mut admissions = Vec::new();
    let mut heads = BTreeMap::new();
    let mut metadata = BTreeMap::new();
    let mut hub_projects = BTreeSet::new();
    let mut knowledge_projects = BTreeSet::new();
    let mut ast_projects = BTreeSet::new();
    let mut ast_heads = BTreeMap::new();
    let mut choices = BTreeMap::new();
    for (index, scope, binding) in &captures {
        let selected = &resources.projects[*index];
        resources.require_active(selected).map_err(failure)?;
        for module in [
            ProjectReadModule::Hub,
            ProjectReadModule::Knowledge,
            ProjectReadModule::Ast,
        ] {
            if module == ProjectReadModule::Ast && ast_server.is_none() {
                continue;
            }
            let admission = match authority
                .admit_broker_read(BrokerRecordRead {
                    caller: BrokerProjectCaller::Web(caller),
                    verifier,
                    backend,
                    original,
                    scope: BrokerProjectScope {
                        installation: resources.auth.installation(),
                        project: ProjectCapture {
                            scope: ScopeCapture {
                                live: scope,
                                record: binding,
                            },
                            version: &selected.2,
                        },
                        providers: &[],
                    },
                    provider,
                    module,
                })
                .await
            {
                Ok(admission) => admission,
                Err(
                    BrokerProjectError::Denied | BrokerProjectError::Authority(AuthError::Denied),
                ) => continue,
                Err(error) => return Err(failure(error.into())),
            };
            let knowledge = module == ProjectReadModule::Knowledge;
            if knowledge {
                let head = graphit_data::knowledge_embeddings::capture_native_knowledge_context(
                    original.snapshot(),
                    &admission,
                    resources.policy.read.native,
                )
                .await
                .map_err(|error| failure(Failure::Physical(std::io::Error::other(error))))?;
                heads.insert(*index, head);
                knowledge_projects.insert(*index);
            } else if module == ProjectReadModule::Ast {
                let server = ast_server.as_ref().expect("composed AST service");
                let observed = match server
                    .open_project_search(selected, &admission, original)
                    .await
                {
                    Ok(Some((capture, pin))) => (capture, Some(pin)),
                    Ok(None) => (
                        server
                            .capture_project_read(selected, &admission, original)
                            .map_err(|error| failure(Failure::Physical(error)))?,
                        None,
                    ),
                    Err(error) => {
                        original
                            .check()
                            .map_err(|cause| failure(Failure::Physical(cause)))?;
                        if error.kind() == std::io::ErrorKind::NotConnected
                            || matches!(
                                error
                                    .get_ref()
                                    .and_then(|cause| cause
                                        .downcast_ref::<crate::project_graph::ReadError>()),
                                Some(crate::project_graph::ReadError::Disabled)
                            )
                        {
                            continue;
                        }
                        return Err(failure(Failure::Physical(error)));
                    }
                };
                ast_heads.insert(*index, observed);
                ast_projects.insert(*index);
            } else {
                hub_projects.insert(*index);
            }
            if !metadata.contains_key(index) {
                admission.recheck().map_err(|error| failure(error.into()))?;
                let identity = ProjectIdentityStore::at(&selected.0)
                    .load()
                    .map_err(|error| failure(Failure::Physical(error)))?
                    .ok_or_else(|| failure(Failure::Invalid("project_identity_unavailable")))?;
                if identity.id() != selected.1 {
                    return Err(failure(Failure::Invalid("project_identity_changed")));
                }
                let display = identity.metadata();
                for value in [display.name.as_str(), display.description.as_str()] {
                    if !value.is_empty() {
                        reference_explorer::text(value, resources.policy.content)
                            .map_err(|code| failure(Failure::Invalid(code)))?;
                    }
                }
                admission.recheck().map_err(|error| failure(error.into()))?;
                metadata.insert(*index, (display.name.clone(), display.description.clone()));
            }
            choices
                .entry((selected.1.to_string(), selected.2.version().to_owned()))
                .and_modify(|admitted: &mut bool| *admitted |= knowledge)
                .or_insert(knowledge);
            admissions.push((*index, module, admission));
        }
    }
    if choices.len() > resources.policy.content.max_collection_entries {
        return Err(failure(Failure::Invalid("response_limit")));
    }
    let ask = tasks
        .ask()
        .map_err(|error| failure(Failure::Physical(error)))?;
    let capability = binding(resources.policy.binding, ask.as_deref());
    let mut catalog = Catalog {
        entries: Vec::new(),
        errors: Vec::new(),
        projects: Vec::new(),
        bindings: vec![capability],
    };
    for ((project_id, project_version), knowledge) in choices {
        for value in [project_id.as_str(), project_version.as_str()] {
            reference_explorer::text(value, resources.policy.content)
                .map_err(|code| failure(Failure::Invalid(code)))?;
        }
        if knowledge {
            let selected = knowledge_projects
                .iter()
                .find(|index| {
                    let selected = &resources.projects[**index];
                    selected.1.to_string() == project_id && selected.2.version() == project_version
                })
                .expect("admitted Knowledge selection");
            let display = metadata.get(selected).expect("admitted project metadata");
            let published = heads.get(selected).and_then(Option::as_ref).is_some();
            let name = format!("Knowledge · {} ({project_version})", display.0);
            reference_explorer::text(&name, resources.policy.content)
                .map_err(|code| failure(Failure::Invalid(code)))?;
            catalog.entries.push(Entry {
                id: format!("project:knowledge:{project_id}@{project_version}"),
                name,
                description: (!display.1.is_empty()).then(|| display.1.clone()),
                project_name: Some(display.0.clone()),
                tags: None,
                artifact_type: "knowledge".into(),
                reference: ArtifactSelection {
                    source: "project".into(),
                    project_id: Some(project_id.clone()),
                    instance_id: None,
                    project_kind: None,
                    artifact_type: "knowledge".into(),
                    id: project_id.clone(),
                    version: project_version.clone(),
                },
                source_label: "Project",
                unavailable: (!published).then_some("project_knowledge_source_unavailable"),
            });
        }
        if let Some(index) = ast_projects.iter().find(|index| {
            let selected = &resources.projects[**index];
            selected.1.to_string() == project_id && selected.2.version() == project_version
        }) {
            let display = metadata.get(index).expect("admitted AST project metadata");
            let name = format!("AST · {} ({project_version})", display.0);
            reference_explorer::text(&name, resources.policy.content)
                .map_err(|code| failure(Failure::Invalid(code)))?;
            catalog.entries.push(Entry {
                id: format!("project:ast:{project_id}@{project_version}"),
                name,
                description: (!display.1.is_empty()).then(|| display.1.clone()),
                project_name: Some(display.0.clone()),
                tags: None,
                artifact_type: "ast".into(),
                reference: ArtifactSelection {
                    source: "project".into(),
                    project_id: Some(project_id.clone()),
                    instance_id: None,
                    project_kind: None,
                    artifact_type: "ast".into(),
                    id: project_id.clone(),
                    version: project_version.clone(),
                },
                source_label: "Project",
                unavailable: None,
            });
        }
        catalog.projects.push(ProjectSelection {
            project_id,
            project_version,
        });
    }
    if let Some(policy) = &resources.policy.imported_knowledge {
        let owner = hub.installation();
        let StoreOwner::Installation { installation_id } = owner.live.owner() else {
            return Err(failure(AuthError::Denied.into()));
        };
        if !matches!(policy.registry.scope(), HubScope::Global { installation_id: id } if id == installation_id.as_str())
            || policy.registry.path().parent() != Some(owner.live.owner_root())
        {
            return Err(failure(AuthError::Denied.into()));
        }
        hub.recheck().map_err(|error| failure(error.into()))?;
        resources.check(original).map_err(failure)?;
        let document = policy
            .registry
            .load()
            .map_err(|error| failure(Failure::Physical(error)))?;
        for (kind, _, entry) in document.entries() {
            let text = matches!(
                kind,
                ArtifactType::Agent | ArtifactType::Skill | ArtifactType::Rule
            );
            if !text && !matches!(kind, ArtifactType::Knowledge | ArtifactType::Ast) {
                continue;
            }
            let ambiguous = text
                && entry
                    .installations
                    .iter()
                    .filter(|installed| installed.target.includes(resources.policy.binding))
                    .take(2)
                    .count()
                    > 1;
            for (index, installed) in entry.installations.iter().enumerate() {
                resources.check(original).map_err(failure)?;
                if text {
                    if !installed.target.includes(resources.policy.binding) {
                        continue;
                    }
                } else if installed.target != InstallationTarget::GlobalNative
                    || !matches!(
                        installed.backend,
                        InstalledBackend::Native | InstalledBackend::RemoteNative(_)
                    )
                {
                    continue;
                }
                let source = &installed.original_source;
                if source.artifact_type() != kind || source.id() != &entry.id {
                    return Err(failure(Failure::Invalid("invalid_hub_registry")));
                }
                let unavailable = if text {
                    if ambiguous {
                        Some("ambiguous_installed_binding")
                    } else if !source
                        .directory()
                        .as_path()
                        .starts_with(owner.live.owner_root())
                        && !hub_projects.iter().any(|index| {
                            let (paths, id, _) = &resources.projects[*index];
                            source.producer() == *id
                                && source.directory().as_path().starts_with(paths.root())
                        })
                    {
                        Some("project_hub_access_unavailable")
                    } else if matches!(
                        installed.payload,
                        InstalledPayload::Original {
                            selection_binding: None
                        }
                    ) {
                        Some("text_selection_unavailable")
                    } else if !catalog.bindings[0].available {
                        Some("live_binding_unavailable")
                    } else {
                        None
                    }
                } else if installed.alias.is_some()
                    || installed.entry_name.is_some()
                    || !matches!(
                        installed.payload,
                        InstalledPayload::Original {
                            selection_binding: None
                        }
                    )
                {
                    Some("unsupported_import_binding")
                } else if kind == ArtifactType::Ast {
                    if !ast_composed {
                        Some("live_ast_service_unavailable")
                    } else if imported_ast.is_none() {
                        Some("ast_read_not_admitted")
                    } else {
                        None
                    }
                } else if knowledge.is_none() {
                    Some("knowledge_read_not_admitted")
                } else {
                    None
                };
                let selected = selection(source);
                let display = installed.display.as_ref();
                let description = display.and_then(|display| display.description());
                let tags = display.map(|display| display.tags());
                if let Some(tags) = tags {
                    if tags.len() > resources.policy.content.max_collection_entries {
                        return Err(failure(Failure::Invalid("response_limit")));
                    }
                    for tag in tags {
                        reference_explorer::text(tag, resources.policy.content)
                            .map_err(|code| failure(Failure::Invalid(code)))?;
                    }
                }
                if let Some(description) = description.filter(|value| !value.is_empty()) {
                    reference_explorer::text(description, resources.policy.content)
                        .map_err(|code| failure(Failure::Invalid(code)))?;
                }
                let name = installed
                    .alias
                    .as_deref()
                    .or(installed.entry_name.as_deref())
                    .or_else(|| display.map(|display| display.name()))
                    .unwrap_or_else(|| source.id().basename());
                let id = format!(
                    "{}:{}@{}:{index}",
                    kind.as_str(),
                    entry.id.as_str(),
                    source.version().as_str()
                );
                for value in [
                    id.as_str(),
                    name,
                    selected.id.as_str(),
                    selected.version.as_str(),
                ] {
                    reference_explorer::text(value, resources.policy.content)
                        .map_err(|code| failure(Failure::Invalid(code)))?;
                }
                if catalog.entries.len() == resources.policy.content.max_collection_entries {
                    return Err(failure(Failure::Invalid("response_limit")));
                }
                catalog.entries.push(Entry {
                    id,
                    name: name.to_owned(),
                    description: description.map(str::to_owned),
                    tags: tags.map(<[String]>::to_vec),
                    project_name: hub_projects.iter().find_map(|index| {
                        let selected = &resources.projects[*index];
                        (selected.1 == source.producer()).then(|| metadata[index].0.clone())
                    }),
                    artifact_type: kind.as_str().to_owned(),
                    reference: selected,
                    source_label: "Installed Hub",
                    unavailable,
                });
            }
        }
    } else {
        catalog.errors.push("hub_registry_unavailable".into());
    }
    hub.recheck().map_err(|error| failure(error.into()))?;
    if let Some(admission) = &imported_ast {
        admission.recheck().map_err(|error| failure(error.into()))?;
    }
    if let Some(admission) = &knowledge {
        admission.recheck().map_err(|error| failure(error.into()))?;
    }
    for (index, module, admission) in &admissions {
        admission.recheck().map_err(|error| failure(error.into()))?;
        resources
            .require_active(&resources.projects[*index])
            .map_err(failure)?;
        if *module == ProjectReadModule::Ast
            && let Some((capture, graph)) = ast_heads.get(index)
        {
            ast_server
                .as_ref()
                .expect("composed AST service")
                .recheck_project_search(capture, admission, original)
                .map_err(|error| failure(Failure::Physical(error)))?;
            if let Some(graph) = graph {
                graph
                    .revalidate()
                    .map_err(|error| failure(Failure::Physical(error)))?;
            }
        }
        if *module == ProjectReadModule::Knowledge
            && let Some(head) = heads.get(index).and_then(Option::as_ref)
        {
            head.recheck(original.snapshot(), admission)
                .map_err(|error| failure(Failure::Physical(std::io::Error::other(error))))?;
        }
    }
    resources.check(original).map_err(failure)?;
    let bytes = reference_explorer::encode(
        &catalog,
        original.snapshot(),
        resources.stop,
        resources.policy.content.max_output_bytes,
    )
    .map_err(failure)?;
    hub.recheck().map_err(|error| failure(error.into()))?;
    if let Some(admission) = &knowledge {
        admission.recheck().map_err(|error| failure(error.into()))?;
    }
    for (index, module, admission) in &admissions {
        admission.recheck().map_err(|error| failure(error.into()))?;
        resources
            .require_active(&resources.projects[*index])
            .map_err(failure)?;
        if *module == ProjectReadModule::Ast
            && let Some((capture, graph)) = ast_heads.get(index)
        {
            ast_server
                .as_ref()
                .expect("composed AST service")
                .recheck_project_search(capture, admission, original)
                .map_err(|error| failure(Failure::Physical(error)))?;
            if let Some(graph) = graph {
                graph
                    .revalidate()
                    .map_err(|error| failure(Failure::Physical(error)))?;
            }
        }
        if *module == ProjectReadModule::Knowledge
            && let Some(head) = heads.get(index).and_then(Option::as_ref)
        {
            head.recheck(original.snapshot(), admission)
                .map_err(|error| failure(Failure::Physical(std::io::Error::other(error))))?;
        }
    }
    resources.check(original).map_err(failure)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::native_document_file::NativeDocumentFileLimits;

    #[test]
    fn capability_requires_the_current_configured_kiro_engine() {
        let directory = tempfile::tempdir().unwrap();
        // Only program discovery is exercised; no vendor CLI is executed.
        let program = std::env::current_exe().unwrap();
        let native = NativeDocumentFileLimits::new(
            1024 * 1024,
            graphit_core::native_path::MAX_PATH_BYTES,
            64,
            64 * 1024,
        )
        .unwrap();
        let ask = AskAvailability::capture(
            graphit_ai::cli_provider::CliCompletionProvider::KiroV3,
            program.as_os_str(),
            Vec::new(),
            directory.path(),
            native,
        )
        .unwrap();
        let current = binding(NativeBinding::KiroV3, Some(&ask));
        assert!(current.available);
        assert_eq!(current.model_selection, "explicit");
        assert!(!binding(NativeBinding::Codex, Some(&ask)).available);
        assert!(!binding(NativeBinding::KiroV3, None).available);
    }
}
