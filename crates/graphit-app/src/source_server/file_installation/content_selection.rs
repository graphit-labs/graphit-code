//! Installed metadata selects a bound FILE origin; it never grants remote access.
use super::*;
use crate::remote_file_install::client::InstalledContentArguments;
use graphit_auth::{Obligation, ProjectReadModule, ProjectRecordAuthority};
use graphit_core::module_settings::Module;
use graphit_fs::NodeKind;
use graphit_hub::registry::HubScope;

pub(super) fn installed(
    state: &State,
    input: InstalledContentArguments,
    credential: &str,
    request: &RequestContext<'_>,
) -> io::Result<(InstallArguments, String, SourceRef)> {
    if input
        .project_dir
        .as_ref()
        .is_some_and(|path| !path.is_absolute())
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    if input.global
        && (input.project_dir.is_some()
            || input.project_version.is_some()
            || input.binding.is_some())
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let policy = &state.policy.native_install.hub;
    let kind = input.kind.artifact_type();
    let id = ArtifactId::new(&input.id, policy.projection()).map_err(io::Error::other)?;
    let requested_version = input
        .version
        .as_deref()
        .map(|version| ConcreteVersion::new(version, policy.projection()))
        .transpose()
        .map_err(io::Error::other)?;
    let caller = state
        .auth
        .issuer()
        .authenticate_runtime(credential, request)
        .map_err(denied)?;
    let live = state.auth.installation();
    let before = || -> io::Result<()> {
        request.check()?;
        if caller
            .authorize(Permission::Read(Module::Hub))
            .map_err(denied)?
            != Obligation::Unconditional
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }

        request.check()
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
    let scope = project
        .map(|(paths, id, _)| PublicationScope::project(paths, *id))
        .transpose()?;
    let authority = ProjectRecordAuthority::new(state.auth.issuer());
    let admission = project
        .zip(scope.as_ref())
        .map(|((_, _, version), scope)| {
            authority.admit_authenticated_read(
                &caller,
                request,
                ScopeInput {
                    installation: live,
                    project: Some(ProjectSelection { scope, version }),
                    providers: &[],
                    binding: state.policy.binding,
                    global_version: "",
                },
                ProjectReadModule::Hub,
            )
        })
        .transpose()
        .map_err(denied)?;
    let (registry, target, project_dir, project_version, binding) = if let Some((
        paths,
        project_id,
        version,
    )) = project
    {
        if version.scope() != paths.root() {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let identity = ProjectIdentityStore::at(paths)
            .load()?
            .ok_or(io::ErrorKind::NotFound)?;
        if identity.id() != *project_id {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let binding = input.binding.unwrap_or(state.policy.binding);
        (
            HubRegistry::project(paths, &identity, policy.hub())?,
            InstallationTarget::for_artifact(binding, kind),
            Some(paths.root().to_owned()),
            Some(version.version().to_owned()),
            Some(binding),
        )
    } else {
        let registry = state.policy.registry.clone();
        if !matches!(registry.scope(), HubScope::Global { installation_id: id } if id == installation_id.as_str())
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        (registry, InstallationTarget::GlobalCache, None, None, None)
    };
    before()?;
    let consumer = graphit_hub::registry::consumer_for_artifact(
        scope.as_ref().map_or(
            graphit_core::artifact_store::PublicationSlot::Scope,
            |scope| scope.publication_slot(),
        ),
        kind,
    );
    let document = registry.load()?;
    let artifact = match &requested_version {
        Some(version) => document
            .get_version_for_consumer(&consumer, kind, id.as_str(), version)
            .ok_or(io::ErrorKind::NotFound)?,
        None => match document.get_for_consumer(&consumer, kind, id.as_str()) {
            Some(artifact) => artifact,
            None => {
                return Err(if document
                    .entries_for_consumer(&consumer)
                    .any(|(entry_kind, entry_id, _)| entry_kind == kind && entry_id == id.as_str())
                {
                    io::ErrorKind::InvalidInput
                } else {
                    io::ErrorKind::NotFound
                }
                .into());
            }
        },
    };
    let installed = artifact
        .installations
        .iter()
        .find(|installed| {
            installed.target == target
                && requested_version
                    .as_ref()
                    .is_none_or(|version| installed.original_source.version() == version)
        })
        .ok_or(io::ErrorKind::NotFound)?;
    let source = &installed.original_source;
    let (transport, provider, profile, identity_provider) = match source
        .remote_file_origin()
        .ok_or(io::ErrorKind::InvalidData)?
    {
        RemoteFileOrigin::Broker { provider } => {
            if input.identity_provider.is_some() {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            (Transport::Broker, provider.clone(), None, None)
        }
        RemoteFileOrigin::Standalone { provider, profile } => (
            Transport::Standalone,
            provider.clone(),
            profile.clone(),
            input.identity_provider,
        ),
    };
    let path = match input.path {
        Some(path) => selected_path(Path::new(&path), policy.content().max_scalar_bytes)?,
        None => {
            let relative = match &installed.payload {
                InstalledPayload::Original if project_dir.is_some() => {
                    let mode = match installed.backend {
                        InstalledBackend::Physical { mode, .. } => mode,
                        _ => ArtifactMode::Copy(CopyPolicy::FollowLinks),
                    };
                    let planner = graphit_hub::ProjectionPlanner::new(
                        NativePath::new(project_dir.clone().ok_or(io::ErrorKind::InvalidData)?)?,
                        policy.projection(),
                        mode,
                    );
                    let projection = planner
                        .plan(ProjectionRequest {
                            binding: binding.ok_or(io::ErrorKind::InvalidData)?,
                            source,
                            alias: installed.alias.as_deref(),
                            entry_name: installed.entry_name.as_deref(),
                        })
                        .map_err(io::Error::other)?;
                    let selected = match &projection {
                        graphit_hub::Projection::Physical(plan)
                            if plan.kind() == NodeKind::File =>
                        {
                            plan.selected()
                        }
                        graphit_hub::Projection::DynamicRule(plan) => plan.selected(),
                        _ => return Err(io::ErrorKind::InvalidInput.into()),
                    };
                    selected
                        .strip_prefix(source.directory().as_path())
                        .map_err(|_| io::ErrorKind::InvalidData)?
                        .to_owned()
                }
                InstalledPayload::Derived(payload)
                    if payload.original_selection.kind() == NodeKind::File =>
                {
                    payload.original_selection.relative().to_owned()
                }
                _ if kind == ArtifactType::Skill => PathBuf::from("SKILL.md"),
                _ if input.global && kind == ArtifactType::Agent => PathBuf::from("AGENT.md"),
                _ => return Err(io::ErrorKind::InvalidInput.into()),
            };
            selected_path(&relative, policy.content().max_scalar_bytes)?
        }
    };
    let selection = RemoteArguments {
        transport,
        provider,
        profile,
        identity_provider,
        producer: source.producer().to_string(),
        kind: input.kind,
        id: source.id().as_str().to_owned(),
        version: source.version().as_str().to_owned(),
        exact: true,
    };
    selection.validate(policy)?;
    if let Some((paths, id, version)) = project
        && !state.live_projects.is_active(paths, *id, version)?
    {
        return Err(io::ErrorKind::NotFound.into());
    }
    if let Some(admission) = &admission {
        admission.recheck().map_err(denied)?;
    }
    before()?;
    Ok((
        InstallArguments {
            selection,
            global: input.global,
            project_dir,
            project_version,
            binding,
            alias: installed.alias.clone(),
            entry_name: installed.entry_name.clone(),
            mode: None,
        },
        path,
        source.clone(),
    ))
}

fn selected_path(path: &Path, maximum: usize) -> io::Result<String> {
    let selection = PayloadSelection::new(path.to_owned(), NodeKind::File)?;
    let value = selection
        .relative()
        .to_str()
        .ok_or(io::ErrorKind::InvalidData)?;
    if value.is_empty() || value.len() > maximum {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(value.to_owned())
}
