//! Canonical local instructions use the same PROJECT publication writer as FILE releases.
use super::file_installation::{FileEffectFailure, ProjectionOptions};
use super::*;
use crate::local_file_install::Arguments;
use graphit_auth::{ExternalEffect, Permission};
use graphit_core::{
    artifact_store::{ArtifactStore, CapturedPublicationScope, ScopeContext, StoreOwner},
    native_path::NativePath,
    project_identity::ProjectIdentityStore,
};
use graphit_hub::{
    ArtifactId, ConcreteVersion, InstallationTarget, SourceOrigin, SourceRef,
    publication::{PublicationChange, PublicationError, ScopedCoordinator},
    registry::HubRegistry,
};

pub(super) fn definition() -> Tool {
    let text = json!({"type":["string","null"]});
    Tool::new(crate::local_file_install::TOOL,
        "Install or reapply canonical local agent/skill/command/workflow instructions into an authorized PROJECT. Source provenance is separate from the physical copy/link mode; removal preserves the original directory.",
        json!({"type":"object","properties":{
            "source_dir":{"type":"string"},"producer":{"type":"string"},
            "artifact_type":{"type":"string","enum":["agent","skill","command","workflow","rule","mcp","language","power"]},
            "id":{"type":"string"},"version":{"type":"string"},"project_dir":{"type":"string"},
            "project_version":text,"binding":text,"alias":text,"entry_name":text,
            "mode":{"type":"string","enum":["copy","link"],"description":"Rendered agents, skills, commands and workflows require copy; original sources may use link. Omission preserves an existing mode or defaults to copy."}
        },"required":["source_dir","producer","artifact_type","id","version","project_dir"],"additionalProperties":false}).as_object().expect("static local schema").clone())
}
fn denied(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, error)
}
pub(super) fn execute(
    state: &State,
    input: Value,
    credential: &str,
    request: &RequestContext<'_>,
) -> io::Result<Value> {
    let mut outcome = json!({"status":"error","operation":"install_local","source":"local","publication":"not_attempted"});
    let result = (|| {
        let mut input: Arguments = serde_json::from_value(input)
            .map_err(|cause| io::Error::new(io::ErrorKind::InvalidInput, cause))?;
        install(state, &mut input, credential, request, &mut outcome)
    })();
    match result {
        Ok(()) => {
            outcome["status"] = "ok".into();
            Ok(outcome)
        }
        Err(cause) => Err(FileEffectFailure::observed(outcome, cause)),
    }
}
fn install(
    state: &State,
    input: &mut Arguments,
    credential: &str,
    request: &RequestContext<'_>,
    outcome: &mut Value,
) -> io::Result<()> {
    let policy = &state.policy.native_install.hub;
    input.validate(policy)?;
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
        request.check()
    };
    before()?;
    if !matches!(live.owner(), StoreOwner::Installation { .. })
        || live.owner_root() != state.policy.installation.root()
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let projects = state.live_projects.snapshot()?;
    let index = super::select_project(
        &projects.projects,
        &std::fs::canonicalize(&input.project_dir)?,
        input.project_version.as_deref(),
    )
    .map_err(io::Error::other)?;
    let (paths, project_id, version) = projects
        .projects
        .get(index)
        .ok_or(io::ErrorKind::NotFound)?;
    if !live.allowed_roots().any(|root| root == paths.root()) || version.scope() != paths.root() {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let identity = ProjectIdentityStore::at(paths)
        .load()?
        .ok_or(io::ErrorKind::NotFound)?;
    if identity.id() != *project_id {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let scope = CapturedPublicationScope::project(
        paths,
        *project_id,
        state.policy.scope,
        request.snapshot(),
    )?;
    let current = || -> io::Result<()> {
        before()?;
        scope.revalidate_bounded(&ScopeContext::from_snapshot(request.snapshot()))?;
        if !state.live_projects.is_active(paths, *project_id, version)? {
            return Err(io::ErrorKind::NotFound.into());
        }
        if ProjectIdentityStore::at(paths).load()?.as_ref() != Some(&identity) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        request.check()
    };
    current()?;
    let source_root = std::fs::canonicalize(&input.source_dir)?;
    if !std::fs::symlink_metadata(&input.source_dir)?
        .file_type()
        .is_dir()
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let producer = input.producer.parse().map_err(io::Error::other)?;
    let kind = input.kind.artifact_type();
    let source = SourceRef::local_link(
        NativePath::new(source_root)?,
        producer,
        kind,
        ArtifactId::new(&input.id, policy.projection()).map_err(io::Error::other)?,
        ConcreteVersion::new(&input.version, policy.projection()).map_err(io::Error::other)?,
        policy.projection(),
    )
    .map_err(io::Error::other)?;
    let registry = HubRegistry::project(paths, &identity, policy.hub())?;
    let binding = input.binding.unwrap_or(state.policy.binding);
    let target = InstallationTarget::for_artifact(binding, kind);
    let previous = registry
        .load()?
        .get_for_consumer(
            &graphit_hub::registry::consumer_for_artifact(scope.publication_slot(), kind),
            kind,
            source.id().as_str(),
        )
        .and_then(|artifact| {
            artifact
                .installations
                .iter()
                .find(|installation| installation.target == target)
        })
        .cloned();
    if let Some(previous) = &previous {
        if previous.original_source.origin() != SourceOrigin::LocalLink
            || previous.original_source.producer() != producer
            || previous.original_source.directory() != source.directory()
        {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        if input.alias.is_none() {
            input.alias = previous.alias.clone();
        }
        if input.entry_name.is_none() {
            input.entry_name = previous.entry_name.clone();
        }
    }
    outcome["producer"] = producer.to_string().into();
    outcome["id"] = input.id.clone().into();
    outcome["version"] = input.version.clone().into();
    outcome["artifact_type"] = serde_json::to_value(kind).map_err(io::Error::other)?;
    outcome["project"] = project_id.to_string().into();
    outcome["project_version"] = version.version().into();
    outcome["binding"] = serde_json::to_value(binding).map_err(io::Error::other)?;
    let core = ArtifactStore::new(scope.clone(), policy.artifacts());
    let coordinator =
        ScopedCoordinator::new(registry, core, NativePath::new(paths.root().to_owned())?)?;
    current()?;
    let result = file_installation::publish(
        state,
        ProjectionOptions {
            binding: input.binding,
            alias: input.alias.as_deref(),
            entry_name: input.entry_name.as_deref(),
            mode: input.mode,
        },
        &source,
        &coordinator,
        (None, previous.as_ref()),
        request,
        &current,
    );
    match result {
        Ok(publication) => {
            outcome["publication"] = format!("{:?}", publication.change).into();
            outcome["possible_parents"] =
                serde_json::to_value(&publication.possible_parents).map_err(io::Error::other)?;
            if publication.change == PublicationChange::Unsupported {
                return Err(io::ErrorKind::Unsupported.into());
            }
            if let Some(cleanup) = publication.cleanup_pending {
                outcome["cleanup_pending"] = true.into();
                return Err(file_installation::cleanup_failure(cleanup));
            }
        }
        Err(error) => {
            if let Some(publication) = error
                .get_ref()
                .and_then(|error| error.downcast_ref::<PublicationError>())
            {
                outcome["publication"] = json!({"committed":publication.committed,"hub_pending":publication.hub_intent.is_some(),"core_pending":publication.core_pending.is_some()});
            }
            return Err(error);
        }
    }
    current()
}
