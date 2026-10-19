//! Installed Hub reads under the actual Role grant and independent origin authority.
mod content;
mod remote;
mod types;
use super::{SelectedProject, State, ToolResult, catalog::Operation, error, parse};
use crate::remote_file_install::FileTransport;
use graphit_auth::{
    ProjectReadModule, ProjectRecordAuthority, ServiceMode, ServiceUseError, VerifiedRoleGrant,
};
use graphit_core::{
    artifact_store::StoreOwner,
    project_identity::{ProjectId, ProjectIdentityStore},
};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, RemoteFileOrigin,
    policy::RequestContext,
    registry::{HubRegistry, HubScope},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Scope {
    Project,
    GlobalInstallation,
}
impl Scope {
    fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::GlobalInstallation => "global_installation",
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct List {
    scope: Scope,
    #[serde(default, deserialize_with = "optional")]
    artifact_type: Option<ArtifactType>,
    #[serde(default, deserialize_with = "optional")]
    id: Option<String>,
    #[serde(default, deserialize_with = "optional")]
    version: Option<String>,
    #[serde(default)]
    offset: usize,
    limit: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Show {
    scope: Scope,
    artifact_type: ArtifactType,
    id: String,
    version: String,
}
fn optional<'de, T: Deserialize<'de>, D: serde::Deserializer<'de>>(
    value: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(value).map(Some)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Keep verified authority, binding and request context explicit"
)]
pub(super) async fn read<'a>(
    state: &State,
    authority: &'a ProjectRecordAuthority<'_>,
    grant: &'a VerifiedRoleGrant<'a>,
    selected: &SelectedProject,
    effective_binding: graphit_hub::NativeBinding,
    operation: Operation,
    input: Option<Value>,
    original: &'a RequestContext<'a>,
) -> ToolResult {
    if operation == Operation::HubTypePath {
        return types::read(
            state,
            authority,
            grant,
            selected,
            effective_binding,
            input,
            original,
        );
    }
    if matches!(operation, Operation::HubProjects | Operation::HubSearch) {
        return remote::read(state, authority, grant, operation, input, original).await;
    }
    if operation == Operation::HubContent {
        return content::read(
            state,
            authority,
            grant,
            selected,
            effective_binding,
            input,
            original,
        )
        .await;
    }
    let policy = &state.policy.source.native_install.hub;
    let show = operation == Operation::HubShow;
    let query = if show {
        let v: Show = parse(input)?;
        List {
            scope: v.scope,
            artifact_type: Some(v.artifact_type),
            id: Some(v.id),
            version: Some(v.version),
            offset: 0,
            limit: policy.content().max_collection_entries,
        }
    } else {
        parse::<List>(input)?
    };
    if query.limit == 0
        || query.limit > policy.content().max_collection_entries
        || query.offset.checked_add(query.limit).is_none()
    {
        return Err(error("invalid_arguments"));
    }
    if let Some(id) = &query.id {
        ArtifactId::new(id, policy.projection()).map_err(|_| error("invalid_arguments"))?;
    }
    let version = query
        .version
        .as_deref()
        .map(|v| ConcreteVersion::new(v, policy.projection()))
        .transpose()
        .map_err(|_| error("invalid_arguments"))?;
    let admission = authority
        .admit_read(grant, original, ProjectReadModule::Hub)
        .map_err(|_| error("authentication_or_permission_denied"))?;
    let before = || {
        original.check().map_err(io_failure)?;
        admission
            .recheck()
            .map_err(|_| error("authentication_or_permission_denied"))
    };
    before()?;
    let registry = registry(state, selected, query.scope)?;
    let document = registry.load().map_err(io_failure)?;
    let mut entries = Vec::new();
    let mut origins = BTreeMap::new();
    let mut matched = 0usize;
    let mut next_offset = None;
    for (kind, id, artifact) in document.entries() {
        if query.artifact_type.is_some_and(|wanted| wanted != kind)
            || query.id.as_deref().is_some_and(|wanted| wanted != id)
        {
            continue;
        }
        for installed in &artifact.installations {
            original.check().map_err(io_failure)?;
            let source = &installed.original_source;
            if version
                .as_ref()
                .is_some_and(|wanted| source.version() != wanted)
                || (installed.target != InstallationTarget::GlobalCache
                    && !installed.target.includes(effective_binding))
            {
                continue;
            }
            if matched < query.offset {
                matched += 1;
                continue;
            }
            if entries.len() == query.limit {
                next_offset = Some(matched);
                break;
            }
            if let Some(origin) = source.remote_file_origin() {
                let key =
                    serde_json::to_vec(origin).map_err(|_| error("response_encoding_failed"))?;
                origins
                    .entry(key)
                    .or_insert_with(|| (origin, BTreeSet::new()))
                    .1
                    .insert(source.producer());
            }
            entries.push(json!({"artifact_type":kind,"id":id,"version":source.version(),"producer":source.producer(),"target":installed.target,"origin":source.origin(),"alias":installed.alias,"entry_name":installed.entry_name,"display":installed.display}));
            matched += 1;
        }
        if next_offset.is_some() {
            break;
        }
    }
    for (origin, producers) in origins.values() {
        before()?;
        authorize_origin(state, grant, origin, producers).await?;
    }
    before()?;
    if show && entries.is_empty() {
        return Ok(json!({"scope":query.scope.as_str(),"kind":"artifact_not_installed"}));
    }
    Ok(json!({"scope":query.scope.as_str(),"entries":entries,"next_offset":next_offset}))
}

async fn authorize_origin<'a>(
    state: &State,
    grant: &'a VerifiedRoleGrant<'a>,
    origin: &RemoteFileOrigin,
    producers: &BTreeSet<ProjectId>,
) -> Result<(), Value> {
    let issuer = state.auth.issuer();
    let mut service = match origin {
        RemoteFileOrigin::Broker { provider } => issuer.capture_role_hub_service(grant, provider),
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
    let http = state
        .hub_http
        .as_ref()
        .ok_or_else(|| error("hub_transport_unavailable"))?;
    if matches!(origin, RemoteFileOrigin::Standalone { .. }) {
        let first = *producers
            .first()
            .ok_or_else(|| error("invalid_installed_origin"))?;
        let transport = FileTransport::open(
            &lease,
            lease.user_subject().map_err(service_failure)?,
            first,
            http,
            state.hub_allow_loopback_http,
        )
        .await
        .map_err(io_failure)?;
        for producer in producers {
            transport
                .authorize_producer(*producer, &state.policy.source.native_install.hub)
                .await
                .map_err(io_failure)?;
        }
        transport.check().map_err(io_failure)?;
    } else {
        for producer in producers {
            let transport = FileTransport::open(
                &lease,
                lease.user_subject().map_err(service_failure)?,
                *producer,
                http,
                state.hub_allow_loopback_http,
            )
            .await
            .map_err(io_failure)?;
            transport
                .authorize(&state.policy.source.native_install.hub)
                .await
                .map_err(io_failure)?;
        }
    }
    lease.check().map_err(service_failure)
}
fn io_failure(cause: io::Error) -> Value {
    match cause.kind() {
        io::ErrorKind::PermissionDenied => error("authentication_or_permission_denied"),
        io::ErrorKind::Interrupted | io::ErrorKind::TimedOut => {
            error("request_cancelled_or_expired")
        }
        _ => json!({"code":"hub_read_failed","io_kind":format!("{:?}",cause.kind())}),
    }
}
fn service_failure(cause: ServiceUseError) -> Value {
    match cause {
        ServiceUseError::Auth(_) => error("authentication_or_permission_denied"),
        ServiceUseError::Store(_) => error("auth_store_failed"),
        ServiceUseError::Broker(_) => error("broker_credentials_failed"),
        ServiceUseError::Profile(_) => error("profile_invalid"),
        ServiceUseError::Disabled => error("hub_service_disabled"),
        ServiceUseError::MissingCredential => error("service_credential_missing"),
        ServiceUseError::LoginRequired => error("service_login_required"),
        ServiceUseError::NeedsRefresh => error("service_refresh_required"),
        ServiceUseError::WrongMode => error("wrong_service_mode"),
    }
}

fn registry(state: &State, selected: &SelectedProject, scope: Scope) -> Result<HubRegistry, Value> {
    let policy = &state.policy.source.native_install.hub;
    match scope {
        Scope::Project => {
            let identity = ProjectIdentityStore::at(&selected.0)
                .load()
                .map_err(io_failure)?
                .ok_or_else(|| error("project_identity_missing"))?;
            if identity.id() != selected.1 {
                return Err(error("project_identity_changed"));
            }
            HubRegistry::project(&selected.0, &identity, policy.hub()).map_err(io_failure)
        }
        Scope::GlobalInstallation => {
            let live = state.auth.installation().live;
            let StoreOwner::Installation { installation_id } = live.owner() else {
                return Err(error("authentication_or_permission_denied"));
            };
            let registry = &state.policy.source.registry;
            if live.owner_root() != state.policy.source.installation.root()
                || registry.path().parent() != Some(live.owner_root())
                || !matches!(registry.scope(),HubScope::Global{installation_id:id} if id==installation_id.as_str())
            {
                return Err(error("authentication_or_permission_denied"));
            }
            Ok(registry.clone())
        }
    }
}
