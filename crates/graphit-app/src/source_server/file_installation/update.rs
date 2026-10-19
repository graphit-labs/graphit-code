//! Sequential composition of existing remote publishers for a captured target.
use super::*;
use crate::remote_file_install::client::{Kind, UpdateAllArguments};
use graphit_hub::{
    NativeBinding,
    registry::{HubDocument, HubScope},
};

pub(super) fn schema() -> Value {
    let text = json!({"type":["string","null"]});
    let mut schema = install_schema();
    schema["required"] = json!(["global"]);
    schema["properties"]["identity_provider"] = text.clone();
    schema["oneOf"] = json!([install_schema(),{"type":"object","properties":{
        "global":{"type":"boolean"},"project_dir":text,"project_version":text,
        "binding":text,"identity_provider":text
    },"required":["global"],"additionalProperties":false}]);
    schema
}

pub(super) fn sync_schema() -> Value {
    json!({"type":"object","properties":{
        "global":{"type":"boolean"},
        "project_dir":{"type":["string","null"]},"project_version":{"type":["string","null"]},
        "binding":{"type":["string","null"]},"identity_provider":{"type":["string","null"]}
    },"required":["global"],"additionalProperties":false})
}
pub(in crate::source_server) async fn sync(
    state: &State,
    input: UpdateAllArguments,
    credential: &str,
    request: &RequestContext<'_>,
) -> io::Result<Value> {
    execute_mode(state, input, credential, request, true).await
}
pub(super) async fn execute(
    state: &State,
    input: UpdateAllArguments,
    credential: &str,
    request: &RequestContext<'_>,
) -> io::Result<Value> {
    execute_mode(state, input, credential, request, false).await
}
async fn execute_mode(
    state: &State,
    input: UpdateAllArguments,
    credential: &str,
    request: &RequestContext<'_>,
    reapply: bool,
) -> io::Result<Value> {
    if input
        .project_dir
        .as_ref()
        .is_some_and(|path| !path.is_absolute())
        || input.global
            && (input.project_dir.is_some()
                || input.project_version.is_some()
                || input.binding.is_some())
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let policy = &state.policy.native_install.hub;
    let issuer = state.auth.issuer();
    let caller = issuer
        .authenticate_runtime(credential, request)
        .map_err(denied)?;
    let session = issuer.bootstrap_admin(&caller).map_err(denied)?;
    let parent = issuer.admin_parent(&caller, &session).map_err(denied)?;
    let live = state.auth.installation().live;
    let before = || -> io::Result<()> {
        request.check()?;
        for permission in [
            Permission::Configure,
            Permission::ManageCache,
            Permission::External(ExternalEffect::PublishedFilesystem),
        ] {
            parent.authorize(permission).map_err(denied)?;
        }
        live.revalidate_bounded(&ScopeContext::from_snapshot(request.snapshot()))?;
        Ok(())
    };
    before()?;
    let StoreOwner::Installation { installation_id } = live.owner() else {
        return Err(io::ErrorKind::PermissionDenied.into());
    };
    if live.owner_root() != state.policy.installation.root() {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let projects = state.live_projects.snapshot()?;
    let project = if input.global {
        None
    } else {
        let index = match input.project_dir.as_deref() {
            Some(root) => super::super::select_project(
                &projects.projects,
                &std::fs::canonicalize(root)?,
                input.project_version.as_deref(),
            )
            .map_err(io::Error::other)?,
            None if input.project_version.is_none() => {
                projects.current.ok_or(io::ErrorKind::NotFound)?
            }
            None => return Err(io::ErrorKind::InvalidInput.into()),
        };
        Some(
            projects
                .projects
                .get(index)
                .ok_or(io::ErrorKind::NotFound)?,
        )
    };
    let (registry, scope) = if let Some((paths, id, version)) = project {
        if !live.allowed_roots().any(|root| root == paths.root()) || version.scope() != paths.root()
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let identity = ProjectIdentityStore::at(paths)
            .load()?
            .ok_or(io::ErrorKind::NotFound)?;
        if identity.id() != *id {
            return Err(io::ErrorKind::InvalidData.into());
        }
        (
            HubRegistry::project(paths, &identity, policy.hub())?,
            Some(CapturedPublicationScope::project(
                paths,
                *id,
                state.policy.scope,
                request.snapshot(),
            )?),
        )
    } else {
        let registry = state.policy.registry.clone();
        if !matches!(registry.scope(), HubScope::Global { installation_id: id } if id == installation_id.as_str())
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        (registry, None)
    };
    let current = || -> io::Result<()> {
        before()?;
        if let Some(scope) = &scope {
            scope.revalidate_bounded(&ScopeContext::from_snapshot(request.snapshot()))?;
        }
        if let Some((paths, id, version)) = project
            && !state.live_projects.is_active(paths, *id, version)?
        {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(())
    };
    current()?;
    let document = registry.load()?;
    let binding = input.binding.unwrap_or(state.policy.binding);
    let consumer = scope.as_ref().map_or(
        graphit_core::artifact_store::PublicationSlot::Scope,
        |scope| scope.publication_slot(),
    );
    let candidates: Vec<_> = if reapply {
        installations(&document, input.global, input.binding, consumer, true).collect()
    } else {
        remote_installations(&document, input.global, binding, consumer).collect()
    };
    let mut outcome = json!({"status":"ok","operation":if reapply {"sync"} else {"update_all"},"global":input.global,"items":candidates.iter().map(|(kind,id,installed)| {
        json!({"artifact_type":kind,"id":id,"producer":installed.original_source.producer(),"previous_version":installed.original_source.version(),"target":installed.target,"status":"not_attempted"})
    }).collect::<Vec<_>>()});
    // The concrete response must fit before starting any item; no corpus quota is imposed.
    if serde_json::to_vec(&outcome)
        .map_err(io::Error::other)?
        .len()
        > policy.content().max_output_bytes
    {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    for (index, (kind, id, installed)) in candidates.into_iter().enumerate() {
        let binding = if input.global {
            binding
        } else {
            input
                .binding
                .filter(|selected| {
                    InstallationTarget::for_artifact(*selected, kind) == installed.target
                })
                .or_else(|| {
                    NativeBinding::ALL.into_iter().find(|selected| {
                        InstallationTarget::for_artifact(*selected, kind) == installed.target
                    })
                })
                .ok_or(io::ErrorKind::InvalidData)?
        };
        let result = async {
            current()?;
            let observed = registry.load()?;
            if !observed.get_version_for_consumer(&graphit_hub::registry::consumer_for_artifact(consumer, kind), kind, id, installed.original_source.version())
                .is_some_and(|artifact| artifact.installations.iter().any(|entry| entry.same_source(installed)))
            {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            if reapply && matches!(installed.backend, InstalledBackend::Native) {
                let kind = match kind {
                    ArtifactType::Ast => graphit_hub::native_package::PackageKind::Ast,
                    ArtifactType::Knowledge => graphit_hub::native_package::PackageKind::Knowledge,
                    _ => return Err(io::ErrorKind::InvalidData.into()),
                };
                crate::native_install::admit(installed.original_source.directory().as_path(), kind,
                    &crate::native_install::InstallResources {
                        auth: &state.auth, installation: &state.policy.installation,
                        registry: &registry, policy,
                        graph_policy: state.policy.native_install.graph,
                    }, request).await.map_err(io::Error::other)?;
                current()?;
                return Ok(json!({"status":"ok","operation":"verify_native","version":installed.original_source.version(),"admission":"verified","publication":"unchanged"}));
            }
            if reapply && matches!(installed.original_source.origin(), graphit_hub::SourceOrigin::LocalLink | graphit_hub::SourceOrigin::Resources) {
                if input.global {
                    let path = installed.original_source.directory().as_path();
                    let snapshot = SourceSnapshot::capture(path, graphit_fs::NodeKind::Directory,
                        &policy.snapshot_policy(CopyPolicy::RejectFileLinks, [])?, request.snapshot())?;
                    snapshot.revalidate_content(request.snapshot())?;
                    current()?;
                    return Ok(json!({"status":"ok","operation":"verify_cache","version":installed.original_source.version(),"cache":"verified","publication":"unchanged"}));
                }
                let (paths, _, version) = project.ok_or(io::ErrorKind::InvalidInput)?;
                let kind: crate::local_file_install::Kind = serde_json::from_value(json!(kind))
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                return super::super::local_file_install::execute(state, json!(crate::local_file_install::Arguments {
                    source_dir: installed.original_source.directory().as_path().to_owned(),
                    producer: installed.original_source.producer().to_string(), kind,
                    id: id.to_owned(), version: installed.original_source.version().as_str().to_owned(),
                    project_dir: paths.root().to_owned(), project_version: Some(version.version().to_owned()),
                    binding: Some(binding), alias: installed.alias.clone(), entry_name: installed.entry_name.clone(), mode: None,
                }), credential, request);
            }
            match &installed.backend {
                InstalledBackend::RemoteNative(record) => {
                    super::super::native_installation::update(
                        state,
                        crate::native_hub_install::Selection {
                            provider: record.provider_name().to_owned(),
                            producer: installed.original_source.producer(),
                            artifact_type: kind,
                            id: id.to_owned(),
                            version: installed.original_source.version().as_str().to_owned(),
                        },
                        installed,
                        credential,
                        request,
                    ).await
                }
                _ => {
                    let origin = installed.original_source.remote_file_origin().ok_or(io::ErrorKind::InvalidData)?;
                    let (transport, provider, profile, identity_provider) = match origin {
                        RemoteFileOrigin::Broker { provider } => (Transport::Broker, provider.clone(), None, None),
                        RemoteFileOrigin::Standalone { provider, profile } => (Transport::Standalone, provider.clone(), profile.clone(), input.identity_provider.clone()),
                    };
                    let kind: Kind = serde_json::from_value(json!(kind)).map_err(io::Error::other)?;
                    let mut item = InstallArguments {
                        selection: RemoteArguments { transport, provider, profile, identity_provider,
                            producer: installed.original_source.producer().to_string(), kind, id: id.to_owned(), version: if reapply { installed.original_source.version().as_str().to_owned() } else { "latest".into() }, exact: reapply },
                        global: input.global,
                        project_dir: project.map(|(paths, _, _)| paths.root().to_owned()),
                        project_version: project.map(|(_, _, version)| version.version().to_owned()),
                        binding: (!input.global).then_some(binding),
                        alias: installed.alias.clone(), entry_name: installed.entry_name.clone(), mode: None,
                    };
                    let mut effects = json!({"status":"error","cache":"not_attempted","publication":"not_attempted"});
                    match install(state, &mut item, &Operation::Update, credential, request, &mut effects).await {
                        Ok(()) => { effects["status"] = "ok".into(); Ok(effects) }
                        Err(cause) => {
                            if effects["cache"] == "in_progress" { effects["cache"] = "not_confirmed".into(); }
                            Err(FileEffectFailure::observed(effects, cause))
                        }
                    }
                }
            }
        }.await;
        match result {
            Ok(effects) => {
                outcome["items"][index]["status"] = "ok".into();
                outcome["items"][index]["outcome"] = effects;
            }
            Err(cause) => {
                outcome["status"] = "error".into();
                outcome["items"][index]["status"] = "error".into();
                outcome["items"][index]["code"] = error_code(&cause).into();
                if let Some(effect) = cause
                    .get_ref()
                    .and_then(|error| error.downcast_ref::<FileEffectFailure>())
                {
                    outcome["items"][index]["outcome"] = effect.outcome.clone();
                }
                if let Some(effect) = cause.get_ref().and_then(|error| {
                    error.downcast_ref::<crate::native_hub_install::InstallationFailure>()
                }) {
                    outcome["items"][index]["registry"] = json!(effect.registry);
                }
                return Err(FileEffectFailure::observed(outcome, cause));
            }
        }
    }
    if let Err(cause) = current() {
        outcome["status"] = "error".into();
        outcome["code"] = error_code(&cause).into();
        return Err(FileEffectFailure::observed(outcome, cause));
    }
    if serde_json::to_vec(&outcome)
        .map_err(io::Error::other)?
        .len()
        > policy.content().max_output_bytes
    {
        outcome["status"] = "error".into();
        return Err(FileEffectFailure::observed(
            outcome,
            io::ErrorKind::FileTooLarge.into(),
        ));
    }
    Ok(outcome)
}

pub(in crate::source_server) fn remote_installations(
    document: &HubDocument,
    global: bool,
    binding: NativeBinding,
    consumer: graphit_core::artifact_store::PublicationSlot,
) -> impl Iterator<Item = (ArtifactType, &str, &Installation)> {
    installations(document, global, Some(binding), consumer, false)
}
fn installations(
    document: &HubDocument,
    global: bool,
    binding: Option<NativeBinding>,
    consumer: graphit_core::artifact_store::PublicationSlot,
    reapply: bool,
) -> impl Iterator<Item = (ArtifactType, &str, &Installation)> {
    document
        .entries()
        .filter(move |(kind, _, artifact)| {
            artifact.consumer == graphit_hub::registry::consumer_for_artifact(consumer, *kind)
        })
        .flat_map(move |(kind, id, artifact)| {
            artifact
                .installations
                .iter()
                .filter(move |installed| {
                    let target = if global {
                        matches!(
                            installed.target,
                            InstallationTarget::GlobalCache | InstallationTarget::GlobalNative
                        )
                    } else {
                        binding.map_or_else(
                            || {
                                NativeBinding::ALL.into_iter().any(|selected| {
                                    installed.target
                                        == InstallationTarget::for_artifact(selected, kind)
                                })
                            },
                            |binding| {
                                installed.target == InstallationTarget::for_artifact(binding, kind)
                            },
                        )
                    };
                    // The bundled Coordinator is rendered by its native publisher, with hooks/MCP.
                    let coordinator = kind == ArtifactType::Agent
                        && id == crate::project_coordinator_install::ARTIFACT_ID
                        && installed.original_source.producer().to_string()
                            == crate::project_coordinator_install::PRODUCER;
                    target
                        && !coordinator
                        && (installed.original_source.remote_file_origin().is_some()
                            || matches!(installed.backend, InstalledBackend::RemoteNative(_))
                            || reapply && matches!(installed.backend, InstalledBackend::Native)
                            || reapply
                                && matches!(
                                    installed.original_source.origin(),
                                    graphit_hub::SourceOrigin::LocalLink
                                        | graphit_hub::SourceOrigin::Resources
                                ))
                })
                .map(move |installed| (kind, id, installed))
        })
}

pub(in crate::source_server) fn error_code(error: &io::Error) -> &'static str {
    match error.kind() {
        io::ErrorKind::PermissionDenied => "authentication_or_permission_denied",
        io::ErrorKind::Interrupted | io::ErrorKind::TimedOut => "request_cancelled_or_expired",
        io::ErrorKind::NotFound => "installation_or_release_not_found",
        io::ErrorKind::AlreadyExists => "installation_changed",
        _ => "update_failed",
    }
}
