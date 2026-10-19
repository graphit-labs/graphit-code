//! Public manifest metadata only; current transport authority remains separate.
use super::{Catalog, client::SearchArguments};
use crate::remote_file_install::FileTransport;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_core::project_identity::ProjectId;
use graphit_hub::{
    ArtifactId, ArtifactType, policy::HubResourcePolicy, remote_metadata::RemoteArtifactManifest,
};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Serialize)]
pub(crate) struct Artifact {
    pub artifact_type: ArtifactType,
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub author: Option<String>,
    pub latest: Option<String>,
    pub versions: Vec<String>,
}
impl From<&RemoteArtifactManifest> for Artifact {
    fn from(manifest: &RemoteArtifactManifest) -> Self {
        Self {
            artifact_type: manifest.artifact_type(),
            id: manifest.id().as_str().into(),
            name: manifest.name().into(),
            description: manifest.description().map(str::to_owned),
            tags: manifest.tags().to_vec(),
            author: manifest.author().map(str::to_owned),
            latest: manifest.latest().map(|v| v.as_str().into()),
            versions: manifest
                .versions()
                .iter()
                .map(|v| v.version().as_str().into())
                .collect(),
        }
    }
}
#[derive(Serialize)]
pub(crate) struct ArtifactPage {
    pub producer: ProjectId,
    pub artifacts: Vec<Artifact>,
    pub next_cursor: Option<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Position {
    pub artifact_type: ArtifactType,
    pub id: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    v: u8,
    binding: String,
    after: Position,
}
pub(crate) fn cursor(
    raw: Option<&str>,
    expected: Option<&str>,
    policy: &HubResourcePolicy,
) -> io::Result<Option<Position>> {
    let Some(raw) = raw else { return Ok(None) };
    if raw.len() > policy.content().max_scalar_bytes {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let parsed: Cursor =
        serde_json::from_slice(&bytes).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    ArtifactId::new(&parsed.after.id, policy.projection())
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    if parsed.v != 1 || expected.is_some_and(|binding| binding != parsed.binding) {
        return Err(super::restart());
    }
    Ok(Some(parsed.after))
}
pub(crate) fn next_cursor(
    binding: String,
    after: Position,
    policy: &HubResourcePolicy,
) -> io::Result<String> {
    let bytes = serde_json::to_vec(&Cursor {
        v: 1,
        binding,
        after,
    })
    .map_err(io::Error::other)?;
    let encoded = URL_SAFE_NO_PAD.encode(bytes);
    if encoded.len() > policy.content().max_scalar_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    Ok(encoded)
}
pub(crate) async fn search(
    catalog: Catalog<'_, '_>,
    input: &SearchArguments,
) -> io::Result<ArtifactPage> {
    input.validate(catalog.policy)?;
    (catalog.before)()?;
    let transport = FileTransport::open(
        catalog.lease,
        catalog.subject,
        input.producer,
        catalog.http,
        catalog.allow_loopback_http,
    )
    .await?;
    transport
        .catalog(input, catalog.policy, catalog.before)
        .await
}
