//! Local PROJECT lifecycle and global metadata removal through captured current authority.
use super::*;
use crate::remote_file_install::lifecycle::{Arguments, Operation, Scope};
pub(in crate::source_server) mod global;
use graphit_core::artifact_store::ArtifactStore;
use graphit_hub::publication::{PublicationError, PublicationFailure};

pub(super) fn schema(operation: Operation) -> Value {
    let text = json!({"type":["string","null"]});
    let (kinds, scopes) = match operation {
        Operation::Remove => (
            json!({"type":"string","enum":ArtifactType::ALL}),
            json!([Scope::Project, Scope::GlobalCache, Scope::GlobalNative]),
        ),
        Operation::Recover | Operation::Forget => (file_kind_schema(), json!([Scope::Project])),
    };
    json!({"type":"object","properties":{
        "artifact_type":kinds,"scope":{"type":"string","enum":scopes},"id":{"type":"string"},"version":text,
        "project_dir":text,"project_version":text,"binding":text
    },"required":["artifact_type","scope","id"],"additionalProperties":false})
}

pub(super) async fn execute(
    state: &State,
    operation: Operation,
    input: Arguments,
    credential: &str,
    request: &RequestContext<'_>,
) -> io::Result<Value> {
    let mut outcome = json!({
        "status":"error","operation":operation.name(),"artifact_type":input.kind,"scope":input.scope,
        "id":input.id,"publication":"not_attempted"
    });
    let result = match input.validate(&state.policy.native_install.hub, operation) {
        Err(error) => Err(error),
        Ok(()) => match input.scope {
            Scope::Project => apply(state, operation, &input, credential, request, &mut outcome),
            Scope::GlobalCache | Scope::GlobalNative => {
                global::remove(state, &input, credential, request, &mut outcome).await
            }
        },
    };
    match result {
        Ok(()) => {
            outcome["status"] = "ok".into();
            Ok(outcome)
        }
        Err(cause) => Err(io::Error::new(
            cause.kind(),
            FileEffectFailure { outcome, cause },
        )),
    }
}

fn apply(
    state: &State,
    operation: Operation,
    input: &Arguments,
    credential: &str,
    request: &RequestContext<'_>,
    outcome: &mut Value,
) -> io::Result<()> {
    let policy = &state.policy.native_install.hub;
    input.validate(policy, operation)?;
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
    let selected = projects
        .projects
        .get(index)
        .ok_or(io::ErrorKind::NotFound)?;
    apply_selected(state, operation, input, selected, request, outcome, &before)
}

/// Both ingresses retain their own authority; this kernel owns the local journal.
pub(in crate::source_server) fn apply_selected(
    state: &State,
    operation: Operation,
    input: &Arguments,
    selected: &(
        graphit_core::project_paths::ProjectPaths,
        graphit_core::project_identity::ProjectId,
        graphit_core::version_context::VersionContext,
    ),
    request: &RequestContext<'_>,
    outcome: &mut Value,
    authorize: &dyn Fn() -> io::Result<()>,
) -> io::Result<()> {
    let policy = &state.policy.native_install.hub;
    input.validate(policy, operation)?;
    let live = state.auth.installation().live;
    authorize()?;
    if !matches!(live.owner(), StoreOwner::Installation { .. })
        || live.owner_root() != state.policy.installation.root()
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let (paths, project_id, version) = selected;
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
        authorize()?;
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
    let kind = input.kind;
    let id = ArtifactId::new(&input.id, policy.projection()).map_err(io::Error::other)?;
    let binding = input.binding.unwrap_or(state.policy.binding);
    let target = InstallationTarget::for_artifact(binding, kind);
    let key = target.key(kind, id.as_str())?;
    let consumer = graphit_hub::registry::consumer_for_artifact(scope.publication_slot(), kind);
    outcome["project"] = project_id.to_string().into();
    outcome["project_version"] = version.version().into();
    outcome["binding"] = serde_json::to_value(binding).map_err(io::Error::other)?;
    let registry = HubRegistry::project(paths, &identity, policy.hub())?;
    let core = ArtifactStore::new(scope.clone(), policy.artifacts());
    let coordinator = ScopedCoordinator::new(
        registry,
        core.clone(),
        NativePath::new(paths.root().to_owned())?,
    )?;
    let mut writer = coordinator
        .writer(request)
        .map_err(|error| observed(error, outcome))?;
    current()?;
    let intent = writer.snapshot().intent();
    if intent.is_some_and(|intent| intent.key != key || intent.consumer != consumer) {
        outcome["blocked_by_pending"] = true.into();
        return Err(io::ErrorKind::WouldBlock.into());
    }
    // Holding the existing writers makes this a stable pending-key observation, not a new lock.
    let physical = core.load().map_err(io::Error::other)?;
    if physical
        .as_ref()
        .and_then(|snapshot| snapshot.pending())
        .is_some_and(|pending| {
            pending.key().is_some_and(|pending_key| {
                pending_key != &key || pending.publication_slot() != Some(scope.publication_slot())
            }) || (pending.key().is_none() && intent.is_none())
        })
    {
        outcome["blocked_by_pending"] = true.into();
        return Err(io::ErrorKind::WouldBlock.into());
    }
    let selected = writer
        .snapshot()
        .get_for_consumer(&consumer, kind, id.as_str())
        .or_else(|| intent.and_then(|intent| intent.desired.as_ref().or(intent.old.as_ref())))
        .and_then(|artifact| {
            artifact
                .installations
                .iter()
                .find(|entry| entry.target == target)
        });
    if let Some(selected) = selected {
        if input
            .version
            .as_deref()
            .is_some_and(|version| selected.original_source.version().as_str() != version)
        {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        outcome["version"] = selected.original_source.version().as_str().into();
    } else if intent.is_none() {
        return Err(io::ErrorKind::NotFound.into());
    } else if input.version.is_some() {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    current()?;
    let result = match operation {
        Operation::Remove => writer.remove(&key),
        Operation::Recover => writer.recover(),
        Operation::Forget => writer.forget(&key),
    }
    .map_err(|error| observed(error, outcome))?;
    outcome["publication"] = json!({"change":format!("{:?}",result.change),"cleanup_pending":result.cleanup_pending.is_some()});
    outcome["possible_parents"] =
        serde_json::to_value(result.possible_parents).map_err(io::Error::other)?;
    outcome["forget_preview"] =
        serde_json::to_value(result.forget_preview).map_err(io::Error::other)?;
    if let Some(cleanup) = result.cleanup_pending {
        return Err(io::Error::other(PublicationCleanup(cleanup)));
    }
    current()
}

fn observed(error: Box<PublicationError>, outcome: &mut Value) -> io::Error {
    outcome["publication"] = json!({
        "committed":error.committed,"hub_pending":error.hub_intent.is_some(),
        "core_pending":error.core_pending.is_some(),"observation_failed":error.observation_failure.is_some()
    });
    if let Some(intent) = &error.hub_intent
        && let graphit_hub::registry::LogicalGoal::Forget { preview, .. } = &intent.goal
    {
        match serde_json::to_value(preview) {
            Ok(preview) => outcome["forget_preview"] = preview,
            Err(_) => outcome["publication"]["observation_failed"] = true.into(),
        }
    }
    let kind = match &error.failure {
        PublicationFailure::Local(error) => error.kind(),
        PublicationFailure::Physical(error) => error.kind(),
        PublicationFailure::Projection(_) => io::ErrorKind::InvalidInput,
    };
    io::Error::new(kind, error)
}
