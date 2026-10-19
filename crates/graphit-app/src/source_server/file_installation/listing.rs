//! Paged local metadata. Reading the registry never hydrates a payload.
use super::*;
use graphit_hub::registry::HubScope;
use std::collections::{BTreeMap, BTreeSet};

pub(super) async fn execute(
    state: &State,
    input: ListArguments,
    credential: &str,
    request: &RequestContext<'_>,
) -> io::Result<Value> {
    let policy = &state.policy.native_install.hub;
    if input.limit == 0
        || input.limit > policy.content().max_collection_entries
        || input.offset.checked_add(input.limit).is_none()
        || input
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
    if let Some(id) = &input.id {
        ArtifactId::new(id, policy.projection()).map_err(io::Error::other)?;
    }
    let version = input
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
    let issuer = state.auth.issuer();
    let mut identity = input
        .identity_provider
        .as_deref()
        .map(|provider| issuer.capture_hub_service(&caller, provider))
        .transpose()
        .map_err(denied)?;
    let verifier = if let Some(identity) = identity.as_mut() {
        if identity.mode().map_err(denied)? != ServiceMode::BrokerNative {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Some(
            state
                .native_verifiers
                .as_ref()
                .ok_or(io::ErrorKind::NotConnected)?
                .get(identity)
                .map_err(io::Error::other)?,
        )
    } else {
        None
    };
    let identity_lease = match (identity.as_mut(), verifier.as_deref()) {
        (Some(identity), Some(verifier)) => {
            Some(identity.native_credential(verifier).await.map_err(denied)?)
        }
        _ => None,
    };
    let live = state.auth.installation().live;
    let before = || -> io::Result<()> {
        request.check()?;
        if issuer
            .authenticate_runtime(credential, request)
            .map_err(denied)?
            .authorize(Permission::Read(Module::Hub))
            .map_err(denied)?
            != Obligation::Unconditional
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        if let Some(identity) = &identity_lease {
            identity.check().map_err(denied)?;
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
    let (registry, consumer) = if let Some((paths, id, version)) = project {
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
        let scope =
            CapturedPublicationScope::project(paths, *id, state.policy.scope, request.snapshot())?;
        (
            HubRegistry::project(paths, &identity, policy.hub())?,
            scope.publication_slot(),
        )
    } else {
        let registry = state.policy.registry.clone();
        if !matches!(registry.scope(),HubScope::Global {installation_id:id} if id==installation_id.as_str())
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        (
            registry,
            graphit_core::artifact_store::PublicationSlot::Scope,
        )
    };
    let document = registry.load()?;
    let mut entries = Vec::new();
    let mut origins = BTreeMap::new();
    let mut matched = 0usize;
    let mut next_offset = None;
    for (kind, id, artifact) in document.entries() {
        if artifact.consumer != graphit_hub::registry::consumer_for_artifact(consumer, kind) {
            continue;
        }
        if input
            .kind
            .is_some_and(|requested| requested.artifact_type() != kind)
            || input.id.as_ref().is_some_and(|requested| requested != id)
        {
            continue;
        }
        for installed in &artifact.installations {
            before()?;
            let source = &installed.original_source;
            if version
                .as_ref()
                .is_some_and(|version| source.version() != version)
                || input
                    .binding
                    .is_some_and(|binding| !installed.target.includes(binding))
            {
                continue;
            }
            if matched < input.offset {
                matched += 1;
                continue;
            }
            if entries.len() == input.limit {
                next_offset = Some(matched);
                break;
            }
            if let Some(origin) = source.remote_file_origin() {
                let key = serde_json::to_vec(origin).map_err(io::Error::other)?;
                origins
                    .entry(key)
                    .or_insert_with(|| (origin, BTreeSet::new()))
                    .1
                    .insert(source.producer());
            }
            entries.push(json!({"artifact_type":kind,"id":id,"version":source.version(),"producer":source.producer(),"target":installed.target,"origin":source.remote_file_origin(),"alias":installed.alias,"entry_name":installed.entry_name,"display":installed.display}));
            matched += 1;
        }
        if next_offset.is_some() {
            break;
        }
    }
    if let Some((paths, id, version)) = project
        && !state.live_projects.is_active(paths, *id, version)?
    {
        return Err(io::ErrorKind::NotFound.into());
    }
    for (origin, producers) in origins.values() {
        before()?;
        authorize_origin(
            state,
            origin,
            producers,
            credential,
            identity_lease.as_ref(),
            request,
        )
        .await?;
    }
    before()?;
    let value =
        json!({"status":"ok","global":input.global,"entries":entries,"next_offset":next_offset});
    if serde_json::to_vec(&value).map_err(io::Error::other)?.len()
        > policy.content().max_output_bytes
    {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    before()?;
    // Authorization can await the remote origin after the project snapshot was captured.
    if let Some((paths, id, version)) = project
        && !state.live_projects.is_active(paths, *id, version)?
    {
        return Err(io::ErrorKind::NotFound.into());
    }
    Ok(value)
}

async fn authorize_origin(
    state: &State,
    origin: &RemoteFileOrigin,
    producers: &BTreeSet<graphit_core::project_identity::ProjectId>,
    credential: &str,
    identity: Option<&graphit_auth::ServiceCredentialLease<'_>>,
    request: &RequestContext<'_>,
) -> io::Result<()> {
    let issuer = state.auth.issuer();
    let caller = issuer
        .authenticate_runtime(credential, request)
        .map_err(denied)?;
    let mut service = match origin {
        RemoteFileOrigin::Broker { provider } => issuer.capture_hub_service(&caller, provider),
        RemoteFileOrigin::Standalone { provider, profile } => {
            issuer.capture_standalone_s3_service(&caller, provider, profile.as_deref())
        }
    }
    .map_err(denied)?;
    let verifier = if service.mode().map_err(denied)? == ServiceMode::BrokerNative {
        Some(
            state
                .native_verifiers
                .as_ref()
                .ok_or(io::ErrorKind::NotConnected)?
                .get(&mut service)
                .map_err(io::Error::other)?,
        )
    } else {
        None
    };
    let lease = match verifier.as_deref() {
        Some(verifier) => service.native_credential(verifier).await.map_err(denied)?,
        None => service.credential().map_err(denied)?,
    };
    let subject = || match origin {
        RemoteFileOrigin::Standalone { .. } => match identity {
            Some(identity) => identity.user_subject().map_err(denied),
            None => lease.user_subject().map_err(denied),
        },
        RemoteFileOrigin::Broker { .. } => lease.user_subject().map_err(denied),
    };
    if matches!(origin, RemoteFileOrigin::Standalone { .. }) {
        let first = *producers.first().ok_or(io::ErrorKind::InvalidInput)?;
        let transport = FileTransport::open(
            &lease,
            subject()?,
            first,
            &state.native_http,
            state.native_allow_loopback_http,
        )
        .await?;
        for producer in producers {
            transport
                .authorize_producer(*producer, &state.policy.native_install.hub)
                .await?;
        }
        transport.check()?;
    } else {
        // Each Broker STS capability remains bound to its own project.
        for producer in producers {
            let transport = FileTransport::open(
                &lease,
                subject()?,
                *producer,
                &state.native_http,
                state.native_allow_loopback_http,
            )
            .await?;
            transport
                .authorize(&state.policy.native_install.hub)
                .await?;
        }
    }
    lease.check().map_err(denied)?;
    Ok(())
}
