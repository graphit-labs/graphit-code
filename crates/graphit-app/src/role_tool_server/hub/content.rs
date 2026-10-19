//! Exact installed FILE content; metadata or a source pathname cannot grant access.
use super::*;
use graphit_core::artifact_store::{ArtifactStore, CapturedPublicationScope, PayloadSelection};
use graphit_hub::policy::HubResourcePolicy;
use std::path::PathBuf;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    scope: Scope,
    artifact_type: ArtifactType,
    id: String,
    version: String,
    path: String,
}
pub(super) async fn read<'a>(
    state: &State,
    authority: &'a ProjectRecordAuthority<'_>,
    grant: &'a VerifiedRoleGrant<'a>,
    selected: &SelectedProject,
    effective_binding: graphit_hub::NativeBinding,
    input: Option<Value>,
    original: &'a RequestContext<'a>,
) -> ToolResult {
    let input: Input = parse(input)?;
    let policy = &state.policy.source.native_install.hub;
    validate(&input, policy)?;
    let id =
        ArtifactId::new(&input.id, policy.projection()).map_err(|_| error("invalid_arguments"))?;
    let version = ConcreteVersion::new(&input.version, policy.projection())
        .map_err(|_| error("invalid_arguments"))?;
    let read = authority
        .admit_read(grant, original, ProjectReadModule::Hub)
        .map_err(|_| error("authentication_or_permission_denied"))?;
    let before = || {
        original.check().map_err(io_failure)?;
        read.recheck()
            .map_err(|_| error("authentication_or_permission_denied"))
    };
    before()?;
    let registry = registry(state, selected, input.scope)?;
    let document = registry.load().map_err(io_failure)?;
    let target = match input.scope {
        Scope::Project => InstallationTarget::for_artifact(effective_binding, input.artifact_type),
        Scope::GlobalInstallation => InstallationTarget::GlobalCache,
    };
    let installed = document
        .get_version(input.artifact_type, id.as_str(), &version)
        .and_then(|artifact| {
            artifact
                .installations
                .iter()
                .find(|installation| installation.target == target)
        })
        .ok_or_else(|| error("artifact_not_installed"))?;
    let source = &installed.original_source;
    let content = if let Some(origin) = source.remote_file_origin() {
        let issuer = state.auth.issuer();
        let mut service = match origin {
            RemoteFileOrigin::Broker { provider } => {
                issuer.capture_role_hub_service(grant, provider)
            }
            RemoteFileOrigin::Standalone { provider, profile } => {
                issuer.capture_role_standalone_s3_service(grant, provider, profile.as_deref())
            }
        }
        .map_err(service_failure)?;
        let verifier = if service.mode().map_err(service_failure)? == ServiceMode::BrokerNative {
            Some(
                state
                    .hub_verifiers
                    .as_ref()
                    .ok_or_else(|| error("hub_transport_unavailable"))?
                    .get(&mut service)
                    .map_err(|_| error("hub_verifier_failure"))?,
            )
        } else {
            None
        };
        let lease = match verifier.as_deref() {
            Some(verifier) => service
                .native_credential(verifier)
                .await
                .map_err(service_failure)?,
            None => service.credential().map_err(service_failure)?,
        };
        let transport = FileTransport::open(
            &lease,
            lease.user_subject().map_err(service_failure)?,
            source.producer(),
            state
                .hub_http
                .as_ref()
                .ok_or_else(|| error("hub_transport_unavailable"))?,
            state.hub_allow_loopback_http,
        )
        .await
        .map_err(io_failure)?;
        before()?;
        transport.authorize(policy).await.map_err(io_failure)?;
        before()?;
        let installation = &state.policy.source.installation;
        let live = state.auth.installation().live;
        let StoreOwner::Installation { installation_id } = live.owner() else {
            return Err(error("authentication_or_permission_denied"));
        };
        if live.owner_root() != installation.root() {
            return Err(error("authentication_or_permission_denied"));
        }
        // Cache receipts belong to the installation-only publication scope,
        // not the authentication realm's additional trusted PROJECT roots.
        let scope = CapturedPublicationScope::installation(
            installation,
            installation_id.clone(),
            [installation.root().to_owned()],
            state.policy.source.scope,
            original.snapshot(),
        )
        .map_err(io_failure)?;
        let cache = ArtifactStore::new(scope, policy.artifacts());
        if !transport
            .verify_cache(source, &cache, source.directory().as_path())
            .map_err(io_failure)?
        {
            return Err(error("installed_file_cache_unavailable"));
        }
        let content = transport
            .read_text_file(
                source.directory().as_path(),
                &input.path,
                state.policy.source.index,
                policy.content().max_output_bytes,
            )
            .map_err(io_failure)?;
        transport.check().map_err(io_failure)?;
        content
    } else {
        local(state, &read, selected, source, &input, original)?
    };
    let current = registry.load().map_err(io_failure)?;
    let current = current
        .get_version(input.artifact_type, id.as_str(), &version)
        .and_then(|artifact| {
            artifact
                .installations
                .iter()
                .find(|installation| installation.target == target)
        });
    if current != Some(installed) {
        return Err(error("installed_artifact_changed"));
    }
    before()?;
    Ok(
        json!({"scope":input.scope.as_str(),"artifact_type":input.artifact_type,"id":id,"version":version,"content":content}),
    )
}
fn validate(input: &Input, policy: &HubResourcePolicy) -> Result<(), Value> {
    if matches!(
        input.artifact_type,
        ArtifactType::Ast | ArtifactType::Knowledge
    ) || input.path.is_empty()
        || input.path.contains('\\')
        || input.path.len() > policy.content().max_scalar_bytes
    {
        return Err(error("invalid_arguments"));
    }
    PayloadSelection::new(PathBuf::from(&input.path), graphit_fs::NodeKind::File)
        .map_err(|_| error("invalid_arguments"))?;
    Ok(())
}

fn local(
    state: &State,
    read: &graphit_auth::ProjectReadAdmission<'_>,
    selected: &SelectedProject,
    source: &graphit_hub::SourceRef,
    input: &Input,
    original: &RequestContext<'_>,
) -> Result<Value, Value> {
    use graphit_core::native_document_file::CapturedProjectDocumentDirectory;
    use graphit_hub::{SourceOrigin, SourceRef};
    if source.producer() != selected.1 {
        return Err(error("authentication_or_permission_denied"));
    }
    match source.origin() {
        SourceOrigin::Resources => {
            let expected = SourceRef::resources(
                &selected.0,
                selected.1,
                input.artifact_type,
                source.id().clone(),
                source.version().clone(),
                state.policy.source.native_install.hub.projection(),
            )
            .map_err(|_| error("invalid_installed_source"))?;
            if source != &expected {
                return Err(error("invalid_installed_source"));
            }
        }
        SourceOrigin::LocalLink => {}
        SourceOrigin::HubCache => return Err(error("installed_file_content_unavailable")),
    }
    let relative = source
        .directory()
        .as_path()
        .strip_prefix(selected.0.root())
        .map_err(|_| error("authentication_or_permission_denied"))?
        .join(&input.path);
    let parent = relative
        .parent()
        .ok_or_else(|| error("invalid_arguments"))?;
    let capture = read
        .project()
        .map_err(|_| error("authentication_or_permission_denied"))?;
    let directory = CapturedProjectDocumentDirectory::capture(
        capture.scope.live,
        parent,
        state.policy.source.index,
        original.snapshot(),
    )
    .map_err(io_failure)?;
    let observed = directory
        .read(
            relative
                .file_name()
                .ok_or_else(|| error("invalid_arguments"))?,
            original.snapshot(),
        )
        .map_err(io_failure)?;
    let value = crate::remote_file_install::text::value(
        std::path::Path::new(&input.path),
        observed
            .bytes()
            .ok_or_else(|| error("artifact_file_absent"))?,
        state
            .policy
            .source
            .native_install
            .hub
            .content()
            .max_output_bytes,
    )
    .map_err(io_failure)?;
    directory
        .revalidate(original.snapshot())
        .map_err(io_failure)?;
    read.recheck()
        .map_err(|_| error("authentication_or_permission_denied"))?;
    original.check().map_err(io_failure)?;
    Ok(value)
}
