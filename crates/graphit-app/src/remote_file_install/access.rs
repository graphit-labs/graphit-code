//! Current standalone Hub ACL capture shared by FILE reads and catalog discovery.
use super::wait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use futures_util::TryStreamExt;
use graphit_auth::{ServiceCredentialLease, VerifiedUserSubject};
use graphit_core::project_identity::ProjectId;
use graphit_hub::policy::HubResourcePolicy;
use object_store::{ObjectStore, ObjectStoreExt, path::Path};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, io, sync::Arc};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantDocument {
    v: u32,
    projects: Vec<Selector>,
}
#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum Selector {
    Id { id: ProjectId },
    Prefix { name_prefix: String },
    All { all: bool },
}
pub(crate) struct Access {
    selectors: Vec<Selector>,
    pub revision: String,
}
impl Access {
    pub fn allows(&self, id: ProjectId, name: &str) -> bool {
        self.selectors.iter().any(|s| match s {
            Selector::Id { id: selected } => *selected == id,
            Selector::Prefix { name_prefix } => name.starts_with(name_prefix),
            Selector::All { all } => *all,
        })
    }
    pub fn needs_discovery(&self) -> bool {
        self.selectors
            .iter()
            .any(|s| !matches!(s, Selector::Id { .. }))
    }
    pub fn ids(&self) -> BTreeSet<ProjectId> {
        self.selectors
            .iter()
            .filter_map(|s| match s {
                Selector::Id { id } => Some(*id),
                _ => None,
            })
            .collect()
    }
}
pub(crate) fn key(prefix: &str, relative: &str) -> io::Result<Path> {
    let value = if prefix.is_empty() {
        relative.to_owned()
    } else {
        format!("{prefix}/{relative}")
    };
    if value.len() > 1024 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Path::parse(value).map_err(io::Error::other)
}
pub(crate) async fn document(
    lease: &ServiceCredentialLease<'_>,
    store: &Arc<dyn ObjectStore>,
    prefix: &str,
    relative: &str,
    max: usize,
    before: &(dyn Fn() -> io::Result<()> + Sync),
) -> io::Result<Option<Vec<u8>>> {
    before()?;
    lease.check().map_err(io::Error::other)?;
    let original = lease.original().map_err(io::Error::other)?;
    let response = match wait(original.snapshot(), store.get(&key(prefix, relative)?)).await {
        Ok(value) => value,
        Err(error)
            if error.get_ref().is_some_and(|e| {
                matches!(
                    e.downcast_ref::<object_store::Error>(),
                    Some(object_store::Error::NotFound { .. })
                )
            }) =>
        {
            before()?;
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    before()?;
    if response.meta.size > max as u64 {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let mut bytes = Vec::new();
    let mut stream = response.into_stream();
    while let Some(chunk) = wait(original.snapshot(), stream.try_next()).await? {
        before()?;
        lease.check().map_err(io::Error::other)?;
        if chunk.len() > max.saturating_sub(bytes.len()) {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        bytes.try_reserve(chunk.len()).map_err(io::Error::other)?;
        bytes.extend_from_slice(&chunk);
    }
    before()?;
    lease.check().map_err(io::Error::other)?;
    Ok(Some(bytes))
}
pub(crate) async fn capture(
    lease: &ServiceCredentialLease<'_>,
    subject: Option<&VerifiedUserSubject<'_>>,
    store: &Arc<dyn ObjectStore>,
    prefix: &str,
    policy: &HubResourcePolicy,
    before: &(dyn Fn() -> io::Result<()> + Sync),
) -> io::Result<Access> {
    before()?;
    lease.check().map_err(io::Error::other)?;
    let mut keys = BTreeSet::from(["v1/global/projects.json".to_owned()]);
    if let Some(subject) = subject {
        subject.check().map_err(io::Error::other)?;
        let issuer = URL_SAFE_NO_PAD.encode(subject.issuer().map_err(io::Error::other)?);
        let user = URL_SAFE_NO_PAD.encode(subject.subject().map_err(io::Error::other)?);
        keys.insert("v1/authenticated/projects.json".into());
        keys.insert(format!("v1/users/{issuer}/{user}/projects.json"));
        for team in subject.teams().map_err(io::Error::other)? {
            keys.insert(format!(
                "v1/teams/{issuer}/{}/projects.json",
                URL_SAFE_NO_PAD.encode(team)
            ));
        }
    } else {
        keys.insert("v1/anonymous/projects.json".into());
    }
    let mut hash = Sha256::new();
    let mut selectors = Vec::new();
    for path in keys {
        hash.update((path.len() as u64).to_le_bytes());
        hash.update(path.as_bytes());
        let bytes = document(
            lease,
            store,
            prefix,
            &path,
            policy.content().max_input_bytes,
            before,
        )
        .await?
        .unwrap_or_default();
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(&bytes);
        // Missing and zero-byte levels contribute nothing; other current grants survive.
        if bytes.is_empty() {
            continue;
        }
        let doc: GrantDocument = serde_json::from_slice(&bytes)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        if doc.v != 1 || doc.projects.len() > policy.content().max_collection_entries {
            return Err(io::ErrorKind::InvalidData.into());
        }
        for selector in &doc.projects {
            if matches!(selector, Selector::All { all: false })
                || matches!(selector, Selector::Prefix { name_prefix } if name_prefix.is_empty() || name_prefix.chars().any(char::is_control))
            {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        selectors.extend(doc.projects);
    }
    before()?;
    lease.check().map_err(io::Error::other)?;
    if let Some(subject) = subject {
        subject.check().map_err(io::Error::other)?;
    }
    Ok(Access {
        selectors,
        revision: URL_SAFE_NO_PAD.encode(hash.finalize()),
    })
}
