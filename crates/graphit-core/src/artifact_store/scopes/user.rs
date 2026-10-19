use super::*;
use sha2::{Digest, Sha256};

/// Literal identity data. Neither this value nor its namespace key grants USER access.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawIdentity")]
pub struct UserIdentity {
    issuer: String,
    subject: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawIdentity {
    issuer: String,
    subject: String,
}

impl TryFrom<RawIdentity> for UserIdentity {
    type Error = io::Error;
    fn try_from(raw: RawIdentity) -> io::Result<Self> {
        Self::new(raw.issuer, raw.subject)
    }
}

impl UserIdentity {
    pub fn new(issuer: String, subject: String) -> io::Result<Self> {
        if issuer.is_empty() || subject.is_empty() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self { issuer, subject })
    }
    pub fn issuer(&self) -> &str {
        &self.issuer
    }
    pub fn subject(&self) -> &str {
        &self.subject
    }
    pub(super) fn validate(&self, maximum: usize) -> io::Result<()> {
        if self.issuer.is_empty() || self.subject.is_empty() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if self
            .issuer
            .len()
            .checked_add(self.subject.len())
            .is_none_or(|n| n > maximum)
        {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        Ok(())
    }
    /// Portable lowercase key for the exact pair, never an authentication claim.
    pub fn namespace_key(&self) -> String {
        let mut digest = Sha256::new();
        digest.update(b"graphit-user-namespace-v1\0");
        for part in [&self.issuer, &self.subject] {
            digest.update((part.len() as u64).to_be_bytes());
            digest.update(part.as_bytes());
        }
        let mut key = String::with_capacity(64);
        for byte in digest.finalize() {
            key.push(char::from(b"0123456789abcdef"[(byte >> 4) as usize]));
            key.push(char::from(b"0123456789abcdef"[(byte & 15) as usize]));
        }
        key
    }
}

impl CapturedPublicationScope {
    /// Capture a USER namespace under an existing installation, without creating it.
    pub fn user(
        installation: &InstallationPaths,
        installation_id: InstallationId,
        identity: UserIdentity,
        limits: ScopeLimits,
        original: &Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Self> {
        original.check()?;
        identity.validate(limits.path_bytes)?;
        let root = installation.global_path(&["users", &identity.namespace_key()])?;
        let runtime = root.join("runtime");
        let anchor = NativeBirthAnchor::capture(
            installation.root(),
            original,
            limits.path_bytes,
            limits.directory_entries,
        )?;
        Self::capture(
            StoreOwner::User {
                installation_id,
                identity,
            },
            installation.brand().clone(),
            CapturePaths {
                owner_root: installation.root(),
                storage_root: installation.root(),
                runtime_root: &runtime,
                allowed: [root],
            },
            limits,
            original,
            anchor,
        )
    }

    /// The exact principal namespace; this selection supplies no permission.
    pub fn user_root(&self) -> io::Result<&Path> {
        if !matches!(self.owner(), StoreOwner::User { .. }) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.runtime_root()
            .parent()
            .ok_or_else(|| io::ErrorKind::InvalidData.into())
    }

    pub(crate) fn user_selection(&self, relative: &Path) -> io::Result<PathBuf> {
        crate::file_io::validate_relative(relative, true)?;
        let root = self
            .user_root()?
            .strip_prefix(self.owner_root())
            .map_err(|_| io::ErrorKind::InvalidData)?;
        Ok(if relative.as_os_str().is_empty() {
            root.to_path_buf()
        } else {
            root.join(relative)
        })
    }

    pub(crate) fn validate_record_selection(&self, relative: &Path) -> io::Result<()> {
        crate::file_io::validate_relative(relative, true)?;
        match self.owner() {
            StoreOwner::Project { .. } => Ok(()),
            StoreOwner::User { .. } => {
                let root = self.user_selection(Path::new(""))?;
                if relative.starts_with(root) {
                    Ok(())
                } else {
                    Err(io::ErrorKind::InvalidInput.into())
                }
            }
            StoreOwner::Installation { .. } => Err(io::ErrorKind::InvalidInput.into()),
        }
    }
}
