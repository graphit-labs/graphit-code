//! Bounded, pure version selection from validated remote catalog data.
//!
//! The borrowed result is identity data, never authorization or a payload lease.
#[cfg(test)]
mod tests;

use crate::{
    ConcreteVersion,
    policy::RequestContext,
    remote_metadata::{RemoteArtifactManifest, RemoteVersion},
};
use semver::Version;
use std::{cmp::Ordering, error::Error, fmt, io};

const SCAN_BATCH: usize = 32;

/// Transport normalization belongs to the caller; exact identities stay literal.
#[derive(Clone, Copy, Debug)]
pub enum VersionSelector<'a> {
    Latest,
    Exact(&'a ConcreteVersion),
    ReleasePrefix { major: u64, minor: Option<u64> },
}

/// Current request capacities, independently of the policy used to decode data.
#[derive(Clone, Copy, Debug)]
pub struct VersionSelectionLimits {
    pub max_versions: usize,
    pub max_version_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VersionSelectionError {
    InvalidPolicy,
    Limit,
    Cancelled,
    Deadline,
    LatestUnavailable,
    VersionNotFound,
    AmbiguousVersion,
}

impl fmt::Display for VersionSelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidPolicy => "invalid version selection policy",
            Self::Limit => "version selection limit exceeded",
            Self::Cancelled => "version selection cancelled",
            Self::Deadline => "version selection deadline exceeded",
            Self::LatestUnavailable => "published latest version unavailable",
            Self::VersionNotFound => "requested version not found",
            Self::AmbiguousVersion => "release version is ambiguous",
        })
    }
}
impl Error for VersionSelectionError {}

fn check(original: &RequestContext<'_>) -> Result<(), VersionSelectionError> {
    original.remaining().map_err(|error| match error.kind() {
        io::ErrorKind::Interrupted => VersionSelectionError::Cancelled,
        _ => VersionSelectionError::Deadline,
    })?;
    Ok(())
}

/// Return the exact member borrowed from `manifest`, without mutating its order.
///
/// Prefixes consider stable full SemVer releases (with at most one lowercase `v`).
/// Different identities tied at the winning precedence are ambiguous. All input
/// version lengths are checked before any match or SemVer parsing is accepted.
pub fn select_version<'m>(
    manifest: &'m RemoteArtifactManifest,
    selector: VersionSelector<'_>,
    limits: VersionSelectionLimits,
    original: &RequestContext<'_>,
) -> Result<&'m RemoteVersion, VersionSelectionError> {
    check(original)?;
    let result = select_checked(manifest, selector, limits, original);
    check(original)?;
    result
}

fn select_checked<'m>(
    manifest: &'m RemoteArtifactManifest,
    selector: VersionSelector<'_>,
    limits: VersionSelectionLimits,
    original: &RequestContext<'_>,
) -> Result<&'m RemoteVersion, VersionSelectionError> {
    if limits.max_versions == 0 || limits.max_version_bytes == 0 {
        return Err(VersionSelectionError::InvalidPolicy);
    }
    if manifest.versions().len() > limits.max_versions
        || matches!(selector, VersionSelector::Exact(value) if value.as_str().len() > limits.max_version_bytes)
        || manifest
            .latest()
            .is_some_and(|value| value.as_str().len() > limits.max_version_bytes)
    {
        return Err(VersionSelectionError::Limit);
    }
    for (index, version) in manifest.versions().iter().enumerate() {
        if index % SCAN_BATCH == 0 {
            check(original)?;
        }
        if version.version().as_str().len() > limits.max_version_bytes {
            return Err(VersionSelectionError::Limit);
        }
    }

    let exact = match selector {
        VersionSelector::Latest => Some(
            manifest
                .latest()
                .ok_or(VersionSelectionError::LatestUnavailable)?,
        ),
        VersionSelector::Exact(version) => Some(version),
        VersionSelector::ReleasePrefix { .. } => None,
    };
    let mut selected = None;
    let mut winning_release: Option<Version> = None;
    let mut ambiguous = false;
    for (index, version) in manifest.versions().iter().enumerate() {
        if index % SCAN_BATCH == 0 {
            check(original)?;
        }
        if let Some(exact) = exact {
            if version.version() == exact {
                selected = Some(version);
            }
        } else if let VersionSelector::ReleasePrefix { major, minor } = selector {
            let identity = version.version().as_str();
            if let Ok(release) = Version::parse(identity.strip_prefix('v').unwrap_or(identity))
                && release.pre.is_empty()
                && release.major == major
                && minor.is_none_or(|minor| release.minor == minor)
            {
                match winning_release
                    .as_ref()
                    .map(|best| release.cmp_precedence(best))
                {
                    None | Some(Ordering::Greater) => {
                        selected = Some(version);
                        winning_release = Some(release);
                        ambiguous = false;
                    }
                    Some(Ordering::Equal) => ambiguous = true,
                    Some(Ordering::Less) => {}
                }
            }
        }
    }
    if ambiguous {
        Err(VersionSelectionError::AmbiguousVersion)
    } else {
        selected.ok_or(VersionSelectionError::VersionNotFound)
    }
}
