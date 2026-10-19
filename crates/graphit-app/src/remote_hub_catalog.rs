//! Available remote Hub PROJECT metadata; discovery grants no PROJECT or payload access.
pub(crate) mod artifacts;
pub mod client;
mod standalone;
use crate::remote_file_install::client::Transport;
use crate::remote_file_install::{access, wait};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_auth::{
    ServiceBindingView, ServiceCredentialLease, ServicePurpose, VerifiedUserSubject,
};
use graphit_core::project_identity::ProjectId;
use graphit_hub::policy::HubResourcePolicy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fmt, io};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Project {
    pub id: ProjectId,
    pub name: String,
}
#[derive(Serialize)]
pub(crate) struct ProjectPage {
    pub projects: Vec<Project>,
    pub next_cursor: Option<String>,
}
#[derive(Debug)]
pub(crate) struct Restart;
impl fmt::Display for Restart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("catalog_restart_required")
    }
}
impl std::error::Error for Restart {}
pub(crate) fn restart() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, Restart)
}
pub(crate) fn is_restart(error: &io::Error) -> bool {
    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(error) = cause {
        if error.is::<Restart>() {
            return true;
        }
        cause = error
            .downcast_ref::<io::Error>()
            .and_then(io::Error::get_ref)
            .map(|e| e as &(dyn std::error::Error + 'static))
            .or_else(|| error.source());
    }
    false
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    v: u8,
    binding: String,
    after: Option<ProjectId>,
    broker: Option<String>,
}
pub(crate) fn binding(value: impl Serialize) -> io::Result<String> {
    let bytes = serde_json::to_vec(&value).map_err(io::Error::other)?;
    Ok(URL_SAFE_NO_PAD.encode(Sha256::digest(bytes)))
}
fn cursor(
    input: &client::ProjectsArguments,
    expected: Option<&str>,
    policy: &HubResourcePolicy,
) -> io::Result<Option<Cursor>> {
    let Some(raw) = &input.cursor else {
        return Ok(None);
    };
    if raw.len() > policy.content().max_scalar_bytes {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let parsed: Cursor =
        serde_json::from_slice(&bytes).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    if parsed.v != 1 || expected.is_some_and(|expected| parsed.binding != expected) {
        return Err(restart());
    }
    Ok(Some(parsed))
}
fn next_cursor(
    binding: &str,
    after: Option<ProjectId>,
    broker: Option<String>,
    policy: &HubResourcePolicy,
) -> io::Result<String> {
    let bytes = serde_json::to_vec(&Cursor {
        v: 1,
        binding: binding.into(),
        after,
        broker,
    })
    .map_err(io::Error::other)?;
    let value = URL_SAFE_NO_PAD.encode(bytes);
    if value.len() > policy.content().max_scalar_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    Ok(value)
}
fn check_name(name: &str, policy: &HubResourcePolicy) -> io::Result<()> {
    if name.is_empty()
        || name.trim() != name
        || name.len() > policy.content().max_scalar_bytes
        || name.chars().any(char::is_control)
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(())
}
pub(crate) struct Catalog<'a, 'request> {
    pub lease: &'a ServiceCredentialLease<'request>,
    pub subject: Option<VerifiedUserSubject<'a>>,
    pub policy: &'a HubResourcePolicy,
    pub http: &'a reqwest::Client,
    pub allow_loopback_http: bool,
    pub before: &'a (dyn Fn() -> io::Result<()> + Sync),
}
pub(crate) async fn projects(
    catalog: &Catalog<'_, '_>,
    input: &client::ProjectsArguments,
) -> io::Result<ProjectPage> {
    let Catalog {
        lease,
        subject,
        policy,
        http,
        allow_loopback_http,
        before,
    } = catalog;
    let allow_loopback_http = *allow_loopback_http;
    input.validate(policy)?;
    before()?;
    lease.check().map_err(io::Error::other)?;
    match (input.transport, lease.binding().map_err(io::Error::other)?) {
        (
            Transport::Broker,
            ServiceBindingView::Broker {
                purpose: ServicePurpose::HubRead,
                ..
            },
        ) => {
            broker(
                lease,
                subject.as_ref(),
                input,
                policy,
                http,
                allow_loopback_http,
                *before,
            )
            .await
        }
        (Transport::Standalone, ServiceBindingView::StandaloneS3 { .. }) => {
            standalone::projects(
                lease,
                subject.as_ref(),
                input,
                policy,
                allow_loopback_http,
                *before,
            )
            .await
        }
        _ => Err(io::ErrorKind::PermissionDenied.into()),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrokerPage {
    v: u8,
    subject: String,
    authorization_revision: String,
    projects: Vec<Project>,
    next_cursor: Option<String>,
}
async fn broker(
    lease: &ServiceCredentialLease<'_>,
    subject: Option<&VerifiedUserSubject<'_>>,
    input: &client::ProjectsArguments,
    policy: &HubResourcePolicy,
    http: &reqwest::Client,
    allow_loopback_http: bool,
    before: &(dyn Fn() -> io::Result<()> + Sync),
) -> io::Result<ProjectPage> {
    let ServiceBindingView::Broker {
        purpose: ServicePurpose::HubRead,
        endpoint,
        ..
    } = lease.binding().map_err(io::Error::other)?
    else {
        return Err(io::ErrorKind::PermissionDenied.into());
    };
    crate::remote_file_install::storage_origin(endpoint, allow_loopback_http)?;
    let scope = binding((
        "broker",
        &input.provider,
        endpoint,
        input.id,
        &input.name_prefix,
        input.limit,
    ))?;
    let previous = cursor(input, Some(&scope), policy)?;
    if previous
        .as_ref()
        .is_some_and(|c| c.after.is_some() || c.broker.is_none())
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut url =
        url::Url::parse(endpoint).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    url.set_path("/v1/hub/catalog/projects");
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("limit", &input.limit.to_string());
        if let Some(id) = input.id {
            query.append_pair("id", &id.to_string());
        }
        if let Some(prefix) = &input.name_prefix {
            query.append_pair("name_prefix", prefix);
        }
        if let Some(token) = previous.as_ref().and_then(|c| c.broker.as_deref()) {
            query.append_pair("cursor", token);
        }
    }
    let original = lease.original().map_err(io::Error::other)?;
    let operation = async {
        before()?;
        lease.check().map_err(io::Error::other)?;
        let mut request = http.get(url);
        if let Some(credential) = lease.credential().map_err(io::Error::other)? {
            request = request.bearer_auth(credential);
        }
        let mut response = request.send().await.map_err(|e| {
            io::Error::new(
                if e.is_timeout() {
                    io::ErrorKind::TimedOut
                } else {
                    io::ErrorKind::ConnectionAborted
                },
                e.without_url(),
            )
        })?;
        before()?;
        lease.check().map_err(io::Error::other)?;
        if !response.status().is_success() {
            return Err(match response.status() {
                reqwest::StatusCode::CONFLICT => restart(),
                reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                    io::ErrorKind::PermissionDenied.into()
                }
                reqwest::StatusCode::BAD_REQUEST => io::ErrorKind::InvalidInput.into(),
                reqwest::StatusCode::NOT_FOUND => io::ErrorKind::NotFound.into(),
                _ => io::Error::other("Hub catalog service failed"),
            });
        }
        if response
            .content_length()
            .is_some_and(|n| n > policy.content().max_input_bytes as u64)
        {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| {
            io::Error::new(
                if e.is_timeout() {
                    io::ErrorKind::TimedOut
                } else {
                    io::ErrorKind::ConnectionAborted
                },
                e.without_url(),
            )
        })? {
            before()?;
            lease.check().map_err(io::Error::other)?;
            if chunk.len() > policy.content().max_input_bytes.saturating_sub(bytes.len()) {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            bytes.try_reserve(chunk.len()).map_err(io::Error::other)?;
            bytes.extend_from_slice(&chunk);
        }
        let page: BrokerPage = serde_json::from_slice(&bytes)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        if page.v != 1
            || page.projects.len() > input.limit
            || page.subject.is_empty()
            || page.subject.len() > policy.content().max_scalar_bytes
            || page.subject.chars().any(char::is_control)
            || page.authorization_revision.parse::<u64>().is_err()
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if let Some(subject) = subject {
            subject.check().map_err(io::Error::other)?;
            if page.subject
                != format!(
                    "{}|{}",
                    subject.issuer().map_err(io::Error::other)?,
                    subject.subject().map_err(io::Error::other)?
                )
            {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
        }
        let mut ids = BTreeSet::new();
        for project in &page.projects {
            check_name(&project.name, policy)?;
            if !ids.insert(project.id)
                || input.id.is_some_and(|id| id != project.id)
                || input
                    .name_prefix
                    .as_ref()
                    .is_some_and(|prefix| !project.name.starts_with(prefix))
            {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        let next = page
            .next_cursor
            .map(|token| {
                if token.is_empty()
                    || token.len() > policy.content().max_scalar_bytes
                    || token.chars().any(char::is_control)
                    || previous.as_ref().and_then(|c| c.broker.as_ref()) == Some(&token)
                {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                next_cursor(&scope, None, Some(token), policy)
            })
            .transpose()?;
        before()?;
        lease.check().map_err(io::Error::other)?;
        Ok(ProjectPage {
            projects: page.projects,
            next_cursor: next,
        })
    };
    wait(original.snapshot(), operation).await
}

#[cfg(test)]
mod tests;
