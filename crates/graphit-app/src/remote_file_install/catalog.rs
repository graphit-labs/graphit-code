//! Current manifest pages from the existing admitted FILE transport, without payload reads.
use super::*;
use crate::remote_hub_catalog::{
    self,
    artifacts::{Artifact, ArtifactPage, Position},
    client::SearchArguments,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_hub::remote_metadata::MetadataError;
use object_store::list::{PaginatedListOptions, PaginatedListStore};

impl FileTransport<'_, '_> {
    async fn catalog_current(
        &self,
        policy: &HubResourcePolicy,
        revision: &str,
        before: &(dyn Fn() -> io::Result<()> + Sync),
    ) -> io::Result<()> {
        before()?;
        if self.authorize(policy).await? != revision {
            return Err(remote_hub_catalog::restart());
        }
        before()?;
        self.check()
    }
    async fn catalog_manifest(
        &self,
        kind: ArtifactType,
        id: &ArtifactId,
        policy: &HubResourcePolicy,
        revision: &str,
        before: &(dyn Fn() -> io::Result<()> + Sync),
    ) -> io::Result<Option<RemoteArtifactManifest>> {
        self.catalog_current(policy, revision, before).await?;
        let relative = registry_key(self.producer, kind, id)?;
        let limits =
            crate::native_publication::metadata_limits(policy.content(), policy.projection());
        let store: Arc<dyn ObjectStore> = self.store.clone();
        let Some(bytes) = access::document(
            self.lease,
            &store,
            &self.prefix,
            &relative,
            limits.bytes,
            &|| {
                before()?;
                self.check()
            },
        )
        .await?
        else {
            return Ok(None);
        };
        let original = self.lease.original().map_err(io::Error::other)?;
        let manifest =
            RemoteArtifactManifest::decode(&bytes, limits, original).map_err(|error| {
                io::Error::new(
                    match error {
                        MetadataError::Cancelled => io::ErrorKind::Interrupted,
                        MetadataError::Deadline => io::ErrorKind::TimedOut,
                        MetadataError::Limit => io::ErrorKind::FileTooLarge,
                        MetadataError::InvalidPolicy | MetadataError::InvalidDocument => {
                            io::ErrorKind::InvalidData
                        }
                    },
                    error,
                )
            })?;
        if manifest.producer() != self.producer
            || manifest.artifact_type() != kind
            || manifest.id() != id
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.catalog_current(policy, revision, before).await?;
        Ok(Some(manifest))
    }
    pub(crate) async fn catalog(
        &self,
        input: &SearchArguments,
        policy: &HubResourcePolicy,
        before: &(dyn Fn() -> io::Result<()> + Sync),
    ) -> io::Result<ArtifactPage> {
        input.validate(policy)?;
        if input.producer != self.producer {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        before()?;
        let revision = self.authorize(policy).await?;
        let location = match self.lease.binding().map_err(io::Error::other)? {
            ServiceBindingView::Broker {
                provider,
                provider_revision,
                endpoint,
                resource,
                ..
            } => {
                let broker = self
                    .broker
                    .as_ref()
                    .ok_or(io::ErrorKind::PermissionDenied)?;
                remote_hub_catalog::binding((
                    "broker",
                    provider,
                    provider_revision,
                    endpoint,
                    resource,
                    &broker.bucket,
                    &broker.region,
                    &broker.endpoint,
                    &self.prefix,
                    &revision,
                ))?
            }
            ServiceBindingView::StandaloneS3 {
                provider,
                profile,
                provider_revision,
            } => {
                let view = self.lease.standalone_s3().map_err(io::Error::other)?;
                remote_hub_catalog::binding((
                    "standalone",
                    provider,
                    profile,
                    provider_revision,
                    &view.location.bucket,
                    &view.location.region,
                    &view.location.endpoint,
                    &self.prefix,
                    &revision,
                ))?
            }
            _ => return Err(io::ErrorKind::PermissionDenied.into()),
        };
        let binding = remote_hub_catalog::binding((
            location,
            &input.provider,
            &input.profile,
            &input.identity_provider,
            input.producer,
            input.kind,
            &input.id,
            &input.version,
            input.limit,
        ))?;
        let previous = crate::remote_hub_catalog::artifacts::cursor(
            input.cursor.as_deref(),
            Some(&binding),
            policy,
        )?;
        if previous.as_ref().is_some_and(|position| {
            input
                .kind
                .is_some_and(|kind| kind != position.artifact_type)
                || input.id.as_ref().is_some_and(|id| *id != position.id)
        }) {
            return Err(remote_hub_catalog::restart());
        }
        let previous_key = previous
            .as_ref()
            .map(|position| {
                let id = ArtifactId::new(&position.id, policy.projection())
                    .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
                registry_key(self.producer, position.artifact_type, &id)
            })
            .transpose()?;
        let mut kinds = ArtifactType::ALL;
        kinds.sort_by_key(|kind| kind.as_str());
        let mut artifacts = Vec::new();
        let mut last = None;
        let mut more = false;
        'kinds: for kind in kinds
            .into_iter()
            .filter(|kind| input.kind.is_none_or(|selected| selected == *kind))
        {
            if previous
                .as_ref()
                .is_some_and(|position| kind.as_str() < position.artifact_type.as_str())
            {
                continue;
            }
            if let Some(id) = &input.id {
                let id = ArtifactId::new(id, policy.projection())
                    .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
                let key = registry_key(self.producer, kind, &id)?;
                if previous_key.as_ref().is_some_and(|after| key <= *after) {
                    continue;
                }
                if let Some(manifest) = self
                    .catalog_manifest(kind, &id, policy, &revision, before)
                    .await?
                    && include(&manifest, input, &mut artifacts, &mut last)
                {
                    more = true;
                    break;
                }
                continue;
            }
            let relative_prefix =
                format!("v1/projects/{}/registry/{}/", self.producer, kind.as_str());
            let prefix = format!("{}/", self.key(relative_prefix.trim_end_matches('/'))?);
            let offset = if previous
                .as_ref()
                .is_some_and(|position| position.artifact_type == kind)
            {
                previous_key
                    .as_ref()
                    .map(|key| self.key(key).map(|key| key.to_string()))
                    .transpose()?
            } else {
                None
            };
            let mut token = None;
            let mut scanned = offset.clone();
            loop {
                self.catalog_current(policy, &revision, before).await?;
                let options = PaginatedListOptions {
                    max_keys: Some(input.limit),
                    offset: offset.clone(),
                    page_token: token.clone(),
                    ..Default::default()
                };
                let page = wait(
                    self.lease.original().map_err(io::Error::other)?.snapshot(),
                    self.store.list_paginated(Some(&prefix), options),
                )
                .await?;
                before()?;
                self.check()?;
                if !page.result.common_prefixes.is_empty()
                    || page.result.objects.len() > input.limit
                {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                for object in page.result.objects {
                    let key = object.location.as_ref();
                    if scanned.as_ref().is_some_and(|after| key <= after.as_str()) {
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    scanned = Some(key.into());
                    let encoded = key
                        .strip_prefix(&prefix)
                        .and_then(|key| key.strip_suffix(".json"))
                        .ok_or(io::ErrorKind::InvalidData)?;
                    let id = String::from_utf8(
                        URL_SAFE_NO_PAD
                            .decode(encoded)
                            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?,
                    )
                    .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
                    let id = ArtifactId::new(&id, policy.projection())
                        .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
                    if self.key(&registry_key(self.producer, kind, &id)?)?.as_ref() != key {
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    if let Some(manifest) = self
                        .catalog_manifest(kind, &id, policy, &revision, before)
                        .await?
                        && include(&manifest, input, &mut artifacts, &mut last)
                    {
                        more = true;
                        break 'kinds;
                    }
                }
                match page.page_token {
                    None => break,
                    Some(next) if next.is_empty() || token.as_ref() == Some(&next) => {
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    Some(next) => token = Some(next),
                }
            }
        }
        self.catalog_current(policy, &revision, before).await?;
        let next_cursor = if more {
            Some(crate::remote_hub_catalog::artifacts::next_cursor(
                binding,
                last.ok_or(io::ErrorKind::InvalidData)?,
                policy,
            )?)
        } else {
            None
        };
        Ok(ArtifactPage {
            producer: self.producer,
            artifacts,
            next_cursor,
        })
    }
}
fn include(
    manifest: &RemoteArtifactManifest,
    input: &SearchArguments,
    artifacts: &mut Vec<Artifact>,
    last: &mut Option<Position>,
) -> bool {
    if input.version.as_ref().is_some_and(|version| {
        !manifest
            .versions()
            .iter()
            .any(|candidate| candidate.version().as_str() == version)
    }) {
        return false;
    }
    if artifacts.len() == input.limit {
        return true;
    }
    *last = Some(Position {
        artifact_type: manifest.artifact_type(),
        id: manifest.id().as_str().into(),
    });
    artifacts.push(Artifact::from(manifest));
    false
}
