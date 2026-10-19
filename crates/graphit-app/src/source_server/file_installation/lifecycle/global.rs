//! Global lifecycle is metadata-only; neither remote storage nor payloads are opened.
use super::*;
use graphit_core::artifact_store::StoreOwner;
use graphit_hub::{ConcreteVersion, registry::HubScope};

pub(super) async fn remove(
    state: &State,
    input: &Arguments,
    credential: &str,
    request: &RequestContext<'_>,
    outcome: &mut Value,
) -> io::Result<()> {
    let issuer = state.auth.issuer();
    let caller = issuer.authenticate_runtime(credential, request);
    request.check()?;
    let caller = caller.map_err(denied)?;
    if input.scope == Scope::GlobalNative {
        let (binding, revision) = caller.native_provider().map_err(denied)?;
        let verifier = state
            .native_verifiers
            .as_ref()
            .ok_or(io::ErrorKind::NotConnected)?
            .get_native(&binding, revision)
            .map_err(denied)?;
        let native = caller.authenticate_native(&verifier).await;
        request.check()?;
        let native = native.map_err(denied)?;
        let before = || {
            request.check()?;
            for permission in [
                Permission::Configure,
                Permission::ManageCache,
                Permission::External(ExternalEffect::PublishedFilesystem),
            ] {
                let admitted = native.authorize_native_installation(permission);
                request.check()?;
                admitted.map_err(denied)?;
            }
            request.check()
        };
        apply(state, input, request, outcome, &before)
    } else {
        let session = issuer.bootstrap_admin(&caller).map_err(denied)?;
        let parent = issuer.admin_parent(&caller, &session).map_err(denied)?;
        let before = || {
            request.check()?;
            for permission in [
                Permission::Configure,
                Permission::ManageCache,
                Permission::External(ExternalEffect::PublishedFilesystem),
            ] {
                let admitted = parent.authorize(permission);
                request.check()?;
                admitted.map_err(denied)?;
            }
            request.check()
        };
        apply(state, input, request, outcome, &before)
    }
}
pub(in crate::source_server) fn apply(
    state: &State,
    input: &Arguments,
    request: &RequestContext<'_>,
    outcome: &mut Value,
    authorize: &dyn Fn() -> io::Result<()>,
) -> io::Result<()> {
    let live = state.auth.installation().live;
    let registry = &state.policy.registry;
    let StoreOwner::Installation { installation_id } = live.owner() else {
        return Err(io::ErrorKind::PermissionDenied.into());
    };
    if live.owner_root() != state.policy.installation.root()
        || live.brand() != state.policy.installation.brand()
        || registry.path().parent() != Some(state.policy.installation.root())
        || registry.scope()
            != &(HubScope::Global {
                installation_id: installation_id.as_str().to_owned(),
            })
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let current = || {
        authorize()?;
        live.revalidate_bounded(&ScopeContext::from_snapshot(request.snapshot()))?;
        request.check()
    };
    current()?;
    let policy = &state.policy.native_install.hub;
    let version = ConcreteVersion::new(
        input
            .version
            .as_deref()
            .ok_or(io::ErrorKind::InvalidInput)?,
        policy.projection(),
    )
    .map_err(|cause| io::Error::new(io::ErrorKind::InvalidInput, cause))?;
    let target = match input.scope {
        Scope::GlobalCache => InstallationTarget::GlobalCache,
        Scope::GlobalNative => InstallationTarget::GlobalNative,
        Scope::Project => return Err(io::ErrorKind::InvalidInput.into()),
    };
    let mut writer = registry.writer(
        request
            .snapshot()
            .required_deadline()?
            .saturating_duration_since(Instant::now()),
        request.cancellation(),
    )?;
    current()?;
    let selected = writer
        .snapshot()
        .get_version(input.kind, &input.id, &version)
        .ok_or(io::ErrorKind::NotFound)?;
    if selected.installations.len() != 1 || selected.installations[0].target != target {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    outcome["version"] = version.as_str().into();
    outcome["producer"] = selected.installations[0]
        .original_source
        .producer()
        .to_string()
        .into();
    outcome["payload_preserved"] = true.into();
    current()?;
    // A failed save may follow the native replacement; retain uncertainty until observed.
    outcome["publication"] = json!({"change":"Unknown","committed":null});
    match writer.remove_metadata_version(input.kind, &input.id, &version) {
        Ok(changed) => {
            outcome["publication"] = json!({"change":if changed {"MetadataRemoved"} else {"VerifiedUnchanged"},"committed":changed});
        }
        Err(cause) => {
            if current().is_ok() {
                match registry.load() {
                    Ok(document) if current().is_ok() => {
                        let removed = document
                            .get_version(input.kind, &input.id, &version)
                            .is_none();
                        outcome["publication"] = json!({"change":if removed {"MetadataRemoved"} else {"NotCommitted"},"committed":removed});
                    }
                    _ => outcome["observation_failed"] = true.into(),
                }
            } else {
                outcome["observation_failed"] = true.into();
            }
            return Err(cause);
        }
    }
    current()
}
