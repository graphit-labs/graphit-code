//! Explicit remote topology and descriptor digest. This record confers no authority.
use crate::{ProjectionLimits, SourceRef, remote_storage::release_prefix};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RemoteNativeRecord {
    provider_name: String,
    bucket: String,
    region: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    endpoint: Option<String>,
    prefix: String,
    root_digest: [u8; 32],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawRemoteNativeRecord {
    provider_name: String,
    bucket: String,
    region: String,
    endpoint: Option<String>,
    prefix: String,
    root_digest: [u8; 32],
}
impl RawRemoteNativeRecord {
    pub(crate) fn restore(self, limits: ProjectionLimits) -> io::Result<RemoteNativeRecord> {
        RemoteNativeRecord::new(
            &self.provider_name,
            &self.bucket,
            &self.region,
            self.endpoint.as_deref(),
            &self.prefix,
            self.root_digest,
            limits,
        )
    }
}

fn invalid() -> io::Error {
    io::ErrorKind::InvalidData.into()
}

impl RemoteNativeRecord {
    pub fn new(
        provider_name: &str,
        bucket: &str,
        region: &str,
        endpoint: Option<&str>,
        prefix: &str,
        root_digest: [u8; 32],
        limits: ProjectionLimits,
    ) -> io::Result<Self> {
        validate_fields(provider_name, bucket, region, endpoint, prefix, limits)?;
        Ok(Self {
            provider_name: provider_name.into(),
            bucket: bucket.into(),
            region: region.into(),
            endpoint: endpoint.map(str::to_owned),
            prefix: prefix.into(),
            root_digest,
        })
    }
    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }
    pub fn bucket(&self) -> &str {
        &self.bucket
    }
    pub fn region(&self) -> &str {
        &self.region
    }
    pub fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }
    pub fn prefix(&self) -> &str {
        &self.prefix
    }
    pub fn root_digest(&self) -> &[u8; 32] {
        &self.root_digest
    }

    pub(crate) fn validate(&self, limits: ProjectionLimits) -> io::Result<()> {
        validate_fields(
            &self.provider_name,
            &self.bucket,
            &self.region,
            self.endpoint.as_deref(),
            &self.prefix,
            limits,
        )
    }
    pub(crate) fn validate_source(
        &self,
        source: &SourceRef,
        limits: ProjectionLimits,
    ) -> io::Result<()> {
        self.validate(limits)?;
        let release = release_prefix(
            source.producer(),
            source.artifact_type(),
            source.id(),
            source.version(),
            &self.root_digest,
        )?;
        if self.prefix.len()
            + usize::from(!self.prefix.is_empty())
            + release.len()
            + "native.json".len()
            > 1024
        {
            return Err(invalid());
        }
        Ok(())
    }
}

fn validate_fields(
    provider_name: &str,
    bucket: &str,
    region: &str,
    endpoint: Option<&str>,
    prefix: &str,
    limits: ProjectionLimits,
) -> io::Result<()> {
    for value in [provider_name, bucket, region] {
        limits.identity(value).map_err(|_| invalid())?;
    }
    if provider_name.contains(['/', '\\', ':'])
        || !(3..=63).contains(&bucket.len())
        || !bucket
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
        || !bucket.as_bytes()[0].is_ascii_alphanumeric()
        || !bucket.as_bytes()[bucket.len() - 1].is_ascii_alphanumeric()
        || bucket.contains("..")
        || bucket.contains(".-")
        || bucket.contains("-.")
        || bucket.parse::<std::net::Ipv4Addr>().is_ok()
        || !region
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(invalid());
    }
    if let Some(endpoint) = endpoint {
        limits.identity(endpoint).map_err(|_| invalid())?;
        let url = url::Url::parse(endpoint).map_err(|_| invalid())?;
        if !matches!(url.scheme(), "https" | "http")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
            || endpoint.contains(['\\'])
            || (url.as_str() != endpoint && url.as_str() != format!("{endpoint}/"))
        {
            return Err(invalid());
        }
    }
    if !prefix.is_empty() {
        limits.identity(prefix).map_err(|_| invalid())?;
        if prefix
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
            || prefix.contains(['\\', ':'])
        {
            return Err(invalid());
        }
    }
    if prefix.len() + usize::from(!prefix.is_empty()) + "native.json".len() > 1024 {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
