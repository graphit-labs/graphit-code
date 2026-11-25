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

impl PublicationScope {
    /// Select a USER namespace using the exact issuer/subject pair as data.
    pub fn user(
        installation: &InstallationPaths,
        installation_id: InstallationId,
        identity: UserIdentity,
    ) -> io::Result<Self> {
        let root = installation.global_path(&["users", &identity.namespace_key()])?;
        Ok(Self {
            owner: StoreOwner::User {
                installation_id,
                identity,
            },
            brand: installation.brand().clone(),
            owner_root: installation.root().to_owned(),
            runtime_root: root.join("runtime"),
            destination_roots: vec![root],
        })
    }

    pub fn user_root(&self) -> io::Result<&Path> {
        if !matches!(self.owner(), StoreOwner::User { .. }) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.runtime_root()
            .parent()
            .ok_or_else(|| io::ErrorKind::InvalidData.into())
    }
}
