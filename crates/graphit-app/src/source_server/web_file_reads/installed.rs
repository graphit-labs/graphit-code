use super::*;
use graphit_hub::InstallationTarget;

pub(super) async fn content(
    state: &State,
    read: &InstalledHubReadAdmission<'_>,
    registry: &HubRegistry,
    cache: &ArtifactStore,
    (slot, input): (
        graphit_core::artifact_store::PublicationSlot,
        inputs::Installed,
    ),
    before: &(dyn Fn() -> io::Result<()> + Sync),
) -> io::Result<Value> {
    before()?;
    let policy = &state.policy.native_install.hub;
    let kind = input.kind.artifact_type();
    let version =
        ConcreteVersion::new(&input.version, policy.projection()).map_err(io::Error::other)?;
    let target = match input.scope {
        Scope::GlobalCache => InstallationTarget::GlobalCache,
        Scope::Project { .. } => InstallationTarget::for_artifact(
            input.binding.ok_or(io::ErrorKind::InvalidInput)?,
            kind,
        ),
    };
    let consumer = graphit_hub::registry::consumer_for_artifact(slot, kind);
    let document = registry.load()?;
    let previous = document
        .get_version_for_consumer(&consumer, kind, &input.id, &version)
        .and_then(|artifact| {
            artifact
                .installations
                .iter()
                .find(|entry| entry.target == target)
        })
        .cloned()
        .ok_or(io::ErrorKind::NotFound)?;
    let source = &previous.original_source;
    if source.origin() != SourceOrigin::HubCache
        || !source
            .directory()
            .as_path()
            .starts_with(state.policy.installation.root())
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let origin = source
        .remote_file_origin()
        .ok_or(io::ErrorKind::InvalidData)?;
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
    let subject = lease
        .user_subject()
        .map_err(denied)?
        .ok_or(io::ErrorKind::PermissionDenied)?;
    let transport = FileTransport::open(
        &lease,
        Some(subject),
        source.producer(),
        &state.native_http,
        state.native_allow_loopback_http,
    )
    .await?;
    transport.authorize(policy).await?;
    before()?;
    if !transport.verify_cache(source, cache, source.directory().as_path())? {
        return Err(io::ErrorKind::NotFound.into());
    }
    let content = transport.read_text_file(
        source.directory().as_path(),
        &input.path,
        state.policy.index,
        state.policy.content.max_output_bytes,
    )?;
    before()?;
    let current = registry.load()?;
    if current
        .get_version_for_consumer(&consumer, kind, &input.id, &version)
        .and_then(|artifact| {
            artifact
                .installations
                .iter()
                .find(|entry| entry.target == target)
        })
        != Some(&previous)
    {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    transport.check()?;
    lease.check().map_err(denied)?;
    before()?;
    Ok(
        json!({"status":"ok","source":"installed","scope":input.scope,"producer":source.producer(),"artifact_type":kind,"id":source.id(),"version":source.version(),"target":target,"content":content,"cache":"Verified"}),
    )
}
