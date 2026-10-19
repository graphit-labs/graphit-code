//! CLI inputs for the authenticated available-project catalog tool.
use crate::remote_file_install::client::Transport;
use graphit_core::project_identity::ProjectId;
use graphit_hub::policy::{HubResourcePolicy, RequestContext};
use rmcp::model::CallToolRequestParams;
use serde::{Deserialize, Serialize};
use std::io;

#[derive(clap::Args, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectsArguments {
    #[arg(long, value_enum, default_value = "broker")]
    pub transport: Transport,
    #[arg(long)]
    pub provider: String,
    #[arg(long)]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub profile: Option<String>,
    #[arg(long)]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub identity_provider: Option<String>,
    #[arg(long, conflicts_with = "name_prefix")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub id: Option<ProjectId>,
    #[arg(long, conflicts_with = "id")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub name_prefix: Option<String>,
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
    #[arg(long)]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub cursor: Option<String>,
}
fn non_null<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
impl ProjectsArguments {
    pub(crate) fn validate(&self, policy: &HubResourcePolicy) -> io::Result<()> {
        if self.limit == 0
            || self.limit > 1000
            || self.limit > policy.content().max_collection_entries
            || (self.id.is_some() && self.name_prefix.is_some())
            || (matches!(self.transport, Transport::Broker)
                && (self.profile.is_some() || self.identity_provider.is_some()))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        for value in std::iter::once(self.provider.as_str())
            .chain(self.profile.as_deref())
            .chain(self.identity_provider.as_deref())
            .chain(self.name_prefix.as_deref())
            .chain(self.cursor.as_deref())
        {
            if value.is_empty()
                || value.trim() != value
                || value.len() > policy.content().max_scalar_bytes
                || value.chars().any(char::is_control)
            {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        }
        if let Some(cursor) = super::cursor(self, None, policy)? {
            let shape = match self.transport {
                Transport::Broker => cursor.after.is_none() && cursor.broker.is_some(),
                Transport::Standalone => cursor.after.is_some() && cursor.broker.is_none(),
            };
            if !shape {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        }
        Ok(())
    }
}
pub fn prepare_projects(
    input: &ProjectsArguments,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    original.check()?;
    input.validate(policy).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            Failure {
                code: if super::is_restart(&error) {
                    "catalog_restart_required"
                } else {
                    "invalid_arguments"
                },
                exit: if super::is_restart(&error) { 1 } else { 2 },
            },
        )
    })?;
    let value = serde_json::to_value(input).map_err(io::Error::other)?;
    if serde_json::to_vec(&value).map_err(io::Error::other)?.len()
        > original.content_limits().max_input_bytes
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            Failure {
                code: "invalid_arguments",
                exit: 2,
            },
        ));
    }
    Ok(
        CallToolRequestParams::new("graphit_hub_projects").with_arguments(
            value
                .as_object()
                .ok_or(io::ErrorKind::InvalidInput)?
                .clone(),
        ),
    )
}

/// Available manifests of an explicit remote PROJECT; all filters are literal.
#[derive(clap::Args, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SearchArguments {
    #[arg(long, value_enum, default_value = "broker")]
    pub transport: Transport,
    #[arg(long)]
    pub provider: String,
    #[arg(long)]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub profile: Option<String>,
    #[arg(long)]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub identity_provider: Option<String>,
    #[arg(long)]
    pub producer: ProjectId,
    #[arg(long = "kind", value_parser = parse_artifact_type)]
    #[serde(
        default,
        rename = "artifact_type",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub kind: Option<graphit_hub::ArtifactType>,
    #[arg(long)]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub id: Option<String>,
    #[arg(long)]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub version: Option<String>,
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
    #[arg(long)]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub cursor: Option<String>,
}
fn parse_artifact_type(value: &str) -> Result<graphit_hub::ArtifactType, String> {
    graphit_hub::ArtifactType::ALL
        .into_iter()
        .find(|kind| kind.as_str() == value)
        .ok_or_else(|| "invalid artifact type".into())
}
impl SearchArguments {
    pub(crate) fn validate(&self, policy: &HubResourcePolicy) -> io::Result<()> {
        if self.limit == 0
            || self.limit > 1000
            || self.limit > policy.content().max_collection_entries
            || (matches!(self.transport, Transport::Broker)
                && (self.profile.is_some() || self.identity_provider.is_some()))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        for value in std::iter::once(self.provider.as_str())
            .chain(self.profile.as_deref())
            .chain(self.identity_provider.as_deref())
            .chain(self.id.as_deref())
            .chain(self.version.as_deref())
            .chain(self.cursor.as_deref())
        {
            if value.is_empty()
                || value.trim() != value
                || value.len() > policy.content().max_scalar_bytes
                || value.chars().any(char::is_control)
            {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        }
        if let Some(id) = &self.id {
            graphit_hub::ArtifactId::new(id, policy.projection())
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        }
        if let Some(version) = &self.version {
            graphit_hub::ConcreteVersion::new(version, policy.projection())
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        }
        super::artifacts::cursor(self.cursor.as_deref(), None, policy)?;
        Ok(())
    }
}
pub fn prepare_search(
    input: &SearchArguments,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    original.check()?;
    input.validate(policy).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            Failure {
                code: if super::is_restart(&error) {
                    "catalog_restart_required"
                } else {
                    "invalid_arguments"
                },
                exit: if super::is_restart(&error) { 1 } else { 2 },
            },
        )
    })?;
    let value = serde_json::to_value(input).map_err(io::Error::other)?;
    if serde_json::to_vec(&value).map_err(io::Error::other)?.len()
        > original.content_limits().max_input_bytes
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            Failure {
                code: "invalid_arguments",
                exit: 2,
            },
        ));
    }
    Ok(
        CallToolRequestParams::new("graphit_hub_search").with_arguments(
            value
                .as_object()
                .ok_or(io::ErrorKind::InvalidInput)?
                .clone(),
        ),
    )
}

#[derive(Debug)]
pub(crate) struct Failure {
    code: &'static str,
    exit: u8,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Hub catalog failed: {}", self.code)
    }
}
impl std::error::Error for Failure {}
pub fn cli_exit_code(error: &(dyn std::error::Error + 'static)) -> Option<u8> {
    let mut cause = Some(error);
    while let Some(error) = cause {
        if let Some(error) = error.downcast_ref::<Failure>() {
            return Some(error.exit);
        }
        cause = error
            .downcast_ref::<io::Error>()
            .and_then(io::Error::get_ref)
            .map(|e| e as &(dyn std::error::Error + 'static))
            .or_else(|| error.source());
    }
    None
}
pub(crate) fn delivery_failure(
    result: &rmcp::model::CallToolResult,
    guard: Option<&io::Error>,
) -> Failure {
    let code = result
        .structured_content
        .as_ref()
        .and_then(|v| v.get("code"))
        .and_then(serde_json::Value::as_str);
    let (code, exit) = match code {
        Some("request_cancelled") => ("request_cancelled", 130),
        Some("invalid_arguments") => ("invalid_arguments", 2),
        Some("catalog_restart_required") => ("catalog_restart_required", 1),
        Some("authentication_or_permission_denied") => ("authentication_or_permission_denied", 1),
        Some("request_deadline_expired") => ("request_deadline_expired", 1),
        Some("invalid_remote_catalog") => ("invalid_remote_catalog", 1),
        Some("catalog_too_large") => ("catalog_too_large", 1),
        Some("catalog_not_configured") => ("catalog_not_configured", 1),
        Some("catalog_not_found") => ("catalog_not_found", 1),
        _ if guard.is_some_and(|e| e.kind() == io::ErrorKind::Interrupted) => {
            ("request_cancelled", 130)
        }
        _ => ("catalog_delivery_failed", 1),
    };
    Failure { code, exit }
}
