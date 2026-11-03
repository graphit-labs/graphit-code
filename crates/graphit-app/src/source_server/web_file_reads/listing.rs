use super::*;
use graphit_core::project_identity::ProjectId;
use graphit_hub::{ArtifactType, InstallationTarget};
use std::collections::{BTreeMap, BTreeSet};

fn file_kind(kind: ArtifactType) -> bool {
    matches!(
        kind,
        ArtifactType::Skill
            | ArtifactType::Command
            | ArtifactType::Workflow
            | ArtifactType::Agent
            | ArtifactType::Rule
            | ArtifactType::Mcp
            | ArtifactType::Language
            | ArtifactType::Power
    )
}
pub(super) async fn read(
    state: &State,
    read: &InstalledHubReadAdmission<'_>,
    registry: &HubRegistry,
    (slot, input): (graphit_core::artifact_store::PublicationSlot, inputs::List),
    before: &(dyn Fn() -> io::Result<()> + Sync),
) -> io::Result<Value> {
    before()?;
    let version = input
        .version
        .as_deref()
        .map(|value| ConcreteVersion::new(value, state.policy.native_install.hub.projection()))
        .transpose()
        .map_err(io::Error::other)?;
    let document = registry.load()?;
    let mut entries = Vec::new();
    let mut origins: BTreeMap<Vec<u8>, (&RemoteFileOrigin, BTreeSet<ProjectId>)> = BTreeMap::new();
    let mut matched = 0usize;
    let mut next_offset = None;
    for (kind, id, artifact) in document.entries() {
        if artifact.consumer != graphit_hub::registry::consumer_for_artifact(slot, kind)
            || !file_kind(kind)
            || input
                .kind
                .is_some_and(|requested| requested.artifact_type() != kind)
            || input.id.as_ref().is_some_and(|requested| requested != id)
        {
            continue;
        }
        for installed in &artifact.installations {
            before()?;
            let source = &installed.original_source;
            if matches!(input.scope, Scope::GlobalCache)
                && installed.target != InstallationTarget::GlobalCache
                || version
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
                origins
                    .entry(serde_json::to_vec(origin).map_err(io::Error::other)?)
                    .or_insert_with(|| (origin, BTreeSet::new()))
                    .1
                    .insert(source.producer());
            }
            entries.push(json!({"artifact_type":kind,"id":id,"version":source.version(),"producer":source.producer(),"target":installed.target,"source_origin":source.origin(),"origin":source.remote_file_origin(),"alias":installed.alias,"entry_name":installed.entry_name,"display":installed.display}));
            matched += 1;
        }
        if next_offset.is_some() {
            break;
        }
    }
    for (origin, producers) in origins.values() {
        authorize_origin(state, read, origin, producers, before).await?;
    }
    before()?;
    Ok(json!({"status":"ok","scope":input.scope,"entries":entries,"next_offset":next_offset}))
}
async fn authorize_origin(
    state: &State,
    read: &InstalledHubReadAdmission<'_>,
    origin: &RemoteFileOrigin,
    producers: &BTreeSet<ProjectId>,
    before: &(dyn Fn() -> io::Result<()> + Sync),
) -> io::Result<()> {
    let issuer = state.auth.issuer();
    let mut service = match origin {
        RemoteFileOrigin::Broker { provider } => {
            issuer.capture_installed_hub_service(read, provider)
        }
        RemoteFileOrigin::Standalone { provider, profile } => {
            issuer.capture_installed_hub_standalone_s3_service(read, provider, profile.as_deref())
        }
    }
    .map_err(denied)?;
    let lease = service.credential().map_err(denied)?;
    let subject = || {
        lease
            .user_subject()
            .map_err(denied)?
            .ok_or_else(|| io::Error::from(io::ErrorKind::PermissionDenied))
    };
    let policy = &state.policy.native_install.hub;
    if matches!(origin, RemoteFileOrigin::Standalone { .. }) {
        let first = *producers.first().ok_or(io::ErrorKind::InvalidData)?;
        before()?;
        let transport = FileTransport::open(
            &lease,
            Some(subject()?),
            first,
            &state.native_http,
            state.native_allow_loopback_http,
        )
        .await?;
        for producer in producers {
            before()?;
            transport.authorize_producer(*producer, policy).await?;
        }
        transport.check()?;
    } else {
        for producer in producers {
            before()?;
            let transport = FileTransport::open(
                &lease,
                Some(subject()?),
                *producer,
                &state.native_http,
                state.native_allow_loopback_http,
            )
            .await?;
            transport.authorize(policy).await?;
            transport.check()?;
        }
    }
    lease.check().map_err(denied)?;
    before()?;
    Ok(())
}
