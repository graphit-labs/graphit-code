//! Existing catalog kernels with a current Role service and independent remote authority.
use super::*;
use crate::remote_file_install::client::Transport;
use crate::remote_hub_catalog::{
    self as catalog,
    client::{ProjectsArguments, SearchArguments},
};

enum Query {
    Projects(ProjectsArguments),
    Search(SearchArguments),
}
pub(super) async fn read<'a>(
    state: &State,
    authority: &'a ProjectRecordAuthority<'_>,
    grant: &'a VerifiedRoleGrant<'a>,
    operation: Operation,
    input: Option<Value>,
    original: &'a RequestContext<'a>,
) -> ToolResult {
    let policy = &state.policy.source.native_install.hub;
    let query = match operation {
        Operation::HubProjects => Query::Projects(parse(input)?),
        Operation::HubSearch => Query::Search(parse(input)?),
        _ => return Err(error("unknown_tool")),
    };
    let (transport, provider, profile, identity_provider) = match &query {
        Query::Projects(q) => {
            q.validate(policy).map_err(catalog_failure)?;
            (
                q.transport,
                q.provider.as_str(),
                q.profile.as_deref(),
                q.identity_provider.as_deref(),
            )
        }
        Query::Search(q) => {
            q.validate(policy).map_err(catalog_failure)?;
            (
                q.transport,
                q.provider.as_str(),
                q.profile.as_deref(),
                q.identity_provider.as_deref(),
            )
        }
    };
    let read = authority
        .admit_read(grant, original, ProjectReadModule::Hub)
        .map_err(|_| error("authentication_or_permission_denied"))?;
    original.check().map_err(io_failure)?;
    read.recheck()
        .map_err(|_| error("authentication_or_permission_denied"))?;
    let issuer = state.auth.issuer();
    let mut service = match transport {
        Transport::Broker => issuer.capture_role_hub_service(grant, provider),
        Transport::Standalone => {
            issuer.capture_role_standalone_s3_service(grant, provider, profile)
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
        Some(v) => service
            .native_credential(v)
            .await
            .map_err(service_failure)?,
        None => service.credential().map_err(service_failure)?,
    };
    let mut identity = identity_provider
        .map(|provider| issuer.capture_role_hub_service(grant, provider))
        .transpose()
        .map_err(service_failure)?;
    let identity_verifier = if let Some(identity) = identity.as_mut() {
        if identity.mode().map_err(service_failure)? != ServiceMode::BrokerNative {
            return Err(error("wrong_service_mode"));
        }
        Some(
            state
                .hub_verifiers
                .as_ref()
                .ok_or_else(|| error("hub_transport_unavailable"))?
                .get(identity)
                .map_err(|_| error("hub_verifier_failure"))?,
        )
    } else {
        None
    };
    let identity_lease = match (identity.as_mut(), identity_verifier.as_deref()) {
        (Some(identity), Some(verifier)) => Some(
            identity
                .native_credential(verifier)
                .await
                .map_err(service_failure)?,
        ),
        _ => None,
    };
    let subject = match &identity_lease {
        Some(identity) => identity.user_subject().map_err(service_failure)?,
        None => lease.user_subject().map_err(service_failure)?,
    };
    let before = || -> io::Result<()> {
        original.check()?;
        read.recheck()
            .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
        if let Some(identity) = &identity_lease {
            identity.check().map_err(io::Error::other)?;
        }
        Ok(())
    };
    before().map_err(io_failure)?;
    let capture = catalog::Catalog {
        lease: &lease,
        subject,
        policy,
        http: state
            .hub_http
            .as_ref()
            .ok_or_else(|| error("hub_transport_unavailable"))?,
        allow_loopback_http: state.hub_allow_loopback_http,
        before: &before,
    };
    let mut value = match query {
        Query::Projects(q) => serde_json::to_value(
            catalog::projects(&capture, &q)
                .await
                .map_err(catalog_failure)?,
        ),
        Query::Search(q) => serde_json::to_value(
            catalog::artifacts::search(capture, &q)
                .await
                .map_err(catalog_failure)?,
        ),
    }
    .map_err(|_| error("response_encoding_failed"))?;
    value["status"] = "ok".into();
    if serde_json::to_vec(&value)
        .map_err(|_| error("response_encoding_failed"))?
        .len()
        > policy.content().max_output_bytes
    {
        return Err(error("response_limit"));
    }
    lease.check().map_err(service_failure)?;
    before().map_err(io_failure)?;
    Ok(value)
}
fn catalog_failure(cause: io::Error) -> Value {
    if catalog::is_restart(&cause) {
        return error("catalog_restart_required");
    }
    match cause.kind() {
        io::ErrorKind::InvalidInput => error("invalid_arguments"),
        io::ErrorKind::InvalidData => error("invalid_remote_catalog"),
        io::ErrorKind::NotFound => error("catalog_not_found"),
        io::ErrorKind::FileTooLarge => error("catalog_too_large"),
        _ => io_failure(cause),
    }
}
