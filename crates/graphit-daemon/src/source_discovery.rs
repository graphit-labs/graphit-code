//! Current Source service address data; authentication remains with the live issuer.
use crate::auth_startup::DaemonAuth;
use graphit_core::{
    file_io::SnapshotContext, local_files, project_paths::InstallationPaths,
    runtime_key::RuntimeKeyError, tcp_listener::BoundListener,
};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt, fs, io,
    net::SocketAddr,
    path::{Path, PathBuf},
};

const NAME: &str = "mcp.json";

fn validate_byte_limit(max_bytes: usize) -> Result<(), DiscoveryError> {
    if max_bytes == 0 || max_bytes.checked_add(1).is_none() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
    }
    Ok(())
}

/// Observed data, neither an authentication grant nor proof of service liveness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryRecord {
    pid: u32,
    address: SocketAddr,
}
impl DiscoveryRecord {
    pub fn pid(&self) -> u32 {
        self.pid
    }
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    fn validate(&self) -> Result<(), DiscoveryError> {
        if self.pid == 0 || self.address.port() == 0 || !self.address.ip().is_loopback() {
            return Err(DiscoveryError::InvalidRecord);
        }
        Ok(())
    }
}

pub enum DiscoveryError {
    Operation(io::Error),
    Runtime(RuntimeKeyError),
    Json(serde_json::Error),
    InvalidRecord,
}
impl fmt::Display for DiscoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Operation(_) => "source discovery filesystem operation failed",
            Self::Runtime(_) => "source discovery service owner is unavailable",
            Self::Json(_) | Self::InvalidRecord => "invalid source service discovery",
        })
    }
}
impl fmt::Debug for DiscoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}
impl Error for DiscoveryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(error) => Some(error),
            Self::Runtime(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::InvalidRecord => None,
        }
    }
}
impl From<io::Error> for DiscoveryError {
    fn from(error: io::Error) -> Self {
        Self::Operation(error)
    }
}
impl From<RuntimeKeyError> for DiscoveryError {
    fn from(error: RuntimeKeyError) -> Self {
        Self::Runtime(error)
    }
}
impl From<serde_json::Error> for DiscoveryError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

fn owned_directory(
    auth: &DaemonAuth,
    original: &SnapshotContext,
) -> Result<PathBuf, DiscoveryError> {
    original.check()?;
    let runtime = auth.runtime().check(original)?;
    Ok(runtime.root().join("daemon"))
}

/// Keep installation-local clients and runtime credentials on loopback, even
/// when the explicit deployment listener accepts connections on all interfaces.
pub fn local_address(listener: &BoundListener) -> io::Result<SocketAddr> {
    let address = listener.address();
    match address.ip() {
        std::net::IpAddr::V4(ip) if ip.is_unspecified() => Ok(SocketAddr::new(
            std::net::Ipv4Addr::LOCALHOST.into(),
            address.port(),
        )),
        std::net::IpAddr::V6(ip) if ip.is_unspecified() => Ok(SocketAddr::new(
            std::net::Ipv6Addr::LOCALHOST.into(),
            address.port(),
        )),
        ip if ip.is_loopback() => Ok(address),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Source listener requires loopback or an explicit wildcard address",
        )),
    }
}

/// Publish after the caller has initialized its actual Source server and bound listener.
/// The supplied owner keeps the native singleton held throughout this operation.
pub fn publish_discovery(
    auth: &DaemonAuth,
    listener: &BoundListener,
    max_bytes: usize,
    original: &SnapshotContext,
) -> Result<DiscoveryRecord, DiscoveryError> {
    original.check()?;
    validate_byte_limit(max_bytes)?;
    let record = DiscoveryRecord {
        pid: std::process::id(),
        address: local_address(listener)?,
    };
    record.validate()?;
    let directory = owned_directory(auth, original)?;
    let bytes = serde_json::to_vec(&record)?;
    if bytes.len() > max_bytes {
        return Err(io::Error::from(io::ErrorKind::FileTooLarge).into());
    }
    fs::create_dir_all(&directory)?;
    original.check()?;
    #[cfg(unix)]
    let permissions = {
        use std::os::unix::fs::PermissionsExt;
        Some(fs::Permissions::from_mode(0o600))
    };
    #[cfg(not(unix))]
    let permissions = None;
    local_files::atomic_replace(
        &directory.join(NAME),
        &bytes,
        permissions,
        original.cancellation(),
    )?;
    Ok(record)
}

fn read_record(
    directory: &Path,
    max_bytes: usize,
    original: &SnapshotContext,
) -> Result<Option<DiscoveryRecord>, DiscoveryError> {
    original.check()?;
    let bytes = local_files::read_regular(&directory.join(NAME), max_bytes)?;
    let record = bytes
        .map(|(bytes, _)| serde_json::from_slice::<DiscoveryRecord>(&bytes))
        .transpose()?;
    if let Some(record) = &record {
        record.validate()?;
    }
    original.check()?;
    Ok(record)
}

/// Observe current discovery data without creating a service, key or directory.
pub fn read_discovery(
    installation: &InstallationPaths,
    max_bytes: usize,
    original: &SnapshotContext,
) -> Result<Option<DiscoveryRecord>, DiscoveryError> {
    original.check()?;
    validate_byte_limit(max_bytes)?;
    let path = installation.global_path(&["daemon"])?;
    read_record(&path, max_bytes, original)
}

/// After workers join, remove this service's matching discovery before removing its key.
/// A replaced record is preserved. The owner retains its singleton; Drop performs no I/O.
pub fn remove_discovery(
    auth: &DaemonAuth,
    expected: &DiscoveryRecord,
    max_bytes: usize,
    original: &SnapshotContext,
) -> Result<bool, DiscoveryError> {
    original.check()?;
    validate_byte_limit(max_bytes)?;
    expected.validate()?;
    if expected.pid != std::process::id() {
        return Err(DiscoveryError::InvalidRecord);
    }
    let directory = owned_directory(auth, original)?;
    if read_record(&directory, max_bytes, original)?.as_ref() != Some(expected) {
        return Ok(false);
    }
    original.check()?;
    fs::remove_file(directory.join(NAME))?;
    Ok(true)
}
