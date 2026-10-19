//! Resolve and stage current FILE releases through the real service lease and S3 SDK.
pub(crate) mod access;
mod cache;
mod catalog;
pub mod client;
pub mod lifecycle;
pub(crate) mod storage;
pub(crate) mod text;

use futures_util::TryStreamExt;
use graphit_auth::{
    ServiceBindingView, ServiceCredentialLease, ServicePurpose, VerifiedUserSubject,
};
use graphit_core::{file_io::SnapshotContext, project_identity::ProjectId};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion,
    file_payload::stage_file_payload,
    policy::{HubResourcePolicy, ResourceSetting},
    remote_metadata::RemoteArtifactManifest,
    remote_storage::{registry_key, release_prefix},
    remote_version::{VersionSelectionLimits, VersionSelector, select_version},
};
use object_store::{ObjectStore, ObjectStoreExt, path::Path as ObjectPath};
use sha2::{Digest, Sha256};
use std::{future::Future, io, path::Path, sync::Arc};
use tempfile::{NamedTempFile, TempDir};
use tokio::io::AsyncWriteExt;

use crate::native_storage_credentials::{BrokerStorage, broker_credentials};

pub(crate) struct FileSelection<'a> {
    pub producer: ProjectId,
    pub kind: ArtifactType,
    pub id: &'a ArtifactId,
    pub version: VersionSelector<'a>,
}
pub(crate) struct ResolvedFile {
    pub manifest: RemoteArtifactManifest,
    pub version: ConcreteVersion,
    pub archive_sha256: [u8; 32],
}

pub(crate) struct FileTransport<'a, 'request> {
    lease: &'a ServiceCredentialLease<'request>,
    subject: Option<VerifiedUserSubject<'a>>,
    producer: ProjectId,
    store: Arc<object_store::aws::AmazonS3>,
    prefix: String,
    broker: Option<BrokerStorage>,
    expires_at: Option<std::time::Instant>,
}

impl<'a, 'request> FileTransport<'a, 'request> {
    /// The handler constructs `subject` from the same actual caller as `lease`.
    /// Neither identity nor transport alone grants access to a remote project.
    pub(crate) async fn open(
        lease: &'a ServiceCredentialLease<'request>,
        subject: Option<VerifiedUserSubject<'a>>,
        producer: ProjectId,
        client: &reqwest::Client,
        allow_loopback_http: bool,
    ) -> io::Result<Self> {
        lease.check().map_err(io::Error::other)?;
        if let Some(subject) = &subject {
            subject.check().map_err(io::Error::other)?;
        }
        let (store, prefix, broker, expires_at) = match lease.binding().map_err(io::Error::other)? {
            ServiceBindingView::Broker {
                purpose: ServicePurpose::HubRead,
                ..
            } => {
                // This current SQL/STS producer checks the real principal and project.
                // Failure never switches to standalone documents or unsigned storage.
                let broker = broker_credentials(lease, producer, client, allow_loopback_http)
                    .await
                    .map_err(io::Error::from)?;
                (
                    Arc::clone(&broker.store),
                    broker.prefix.clone(),
                    Some(broker),
                    None,
                )
            }
            ServiceBindingView::StandaloneS3 { .. } => {
                let deadline = lease
                    .original()
                    .map_err(io::Error::other)?
                    .snapshot()
                    .required_deadline()?;
                let storage = storage::open(
                    lease,
                    allow_loopback_http,
                    deadline,
                    deadline.saturating_duration_since(std::time::Instant::now()),
                )
                .await?;
                (storage.store, storage.prefix, None, storage.expires_at)
            }
            _ => return Err(io::ErrorKind::PermissionDenied.into()),
        };
        let transport = Self {
            lease,
            subject,
            producer,
            store,
            prefix,
            broker,
            expires_at,
        };
        transport.check()?;
        Ok(transport)
    }
    pub(crate) fn check(&self) -> io::Result<()> {
        if self
            .expires_at
            .is_some_and(|expiry| std::time::Instant::now() >= expiry)
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        self.lease.check().map_err(io::Error::other)?;
        if let Some(subject) = &self.subject {
            subject.check().map_err(io::Error::other)?;
        }
        if let Some(broker) = &self.broker {
            broker.check().map_err(io::Error::from)?;
        }
        Ok(())
    }
    fn key(&self, relative: &str) -> io::Result<ObjectPath> {
        access::key(&self.prefix, relative)
    }
    async fn small(&self, key: &str, max: usize) -> io::Result<Vec<u8>> {
        self.check()?;
        let original = self.lease.original().map_err(io::Error::other)?;
        let store: Arc<dyn ObjectStore> = self.store.clone();
        let bytes = crate::remote_native_mount::read_small(
            &store,
            &self.key(key)?,
            max,
            original.snapshot(),
        )
        .await
        .map_err(io::Error::other)?;
        self.check()?;
        Ok(bytes)
    }

    /// Fetch applicable current grants before any catalog/payload object.
    pub(crate) async fn authorize(&self, policy: &HubResourcePolicy) -> io::Result<String> {
        self.authorize_producer(self.producer, policy).await
    }

    /// Reuse the same standalone capture while checking each producer's current ACL.
    /// Broker credentials remain bound to exactly the project they were issued for.
    pub(crate) async fn authorize_producer(
        &self,
        producer: ProjectId,
        policy: &HubResourcePolicy,
    ) -> io::Result<String> {
        self.check()?;
        if let Some(broker) = &self.broker {
            return if producer == self.producer {
                Ok(broker.authorization_revision.clone())
            } else {
                Err(io::ErrorKind::PermissionDenied.into())
            };
        }
        let store: Arc<dyn ObjectStore> = self.store.clone();
        let grants = access::capture(
            self.lease,
            self.subject.as_ref(),
            &store,
            &self.prefix,
            policy,
            &|| self.check(),
        )
        .await?;
        if !grants.needs_discovery() && !grants.ids().contains(&producer) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let project = self.project(producer, policy).await?;
        self.check()?;
        if grants.allows(project.id, &project.name) {
            Ok(grants.revision)
        } else {
            Err(io::ErrorKind::PermissionDenied.into())
        }
    }

    async fn project(
        &self,
        producer: ProjectId,
        policy: &HubResourcePolicy,
    ) -> io::Result<graphit_hub::project_metadata::HubProjectMetadata> {
        let bytes = self
            .small(
                &graphit_hub::project_metadata::project_metadata_key(producer),
                policy.content().max_input_bytes,
            )
            .await?;
        graphit_hub::project_metadata::HubProjectMetadata::decode(
            &bytes,
            producer,
            policy.content().max_input_bytes,
        )
    }

    pub(crate) async fn resolve(
        &self,
        selection: FileSelection<'_>,
        policy: &HubResourcePolicy,
    ) -> io::Result<ResolvedFile> {
        if selection.producer != self.producer
            || matches!(selection.kind, ArtifactType::Ast | ArtifactType::Knowledge)
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.authorize(policy).await?;
        let original = self.lease.original().map_err(io::Error::other)?;
        let limits =
            crate::native_publication::metadata_limits(policy.content(), policy.projection());
        let bytes = self
            .small(
                &registry_key(selection.producer, selection.kind, selection.id)?,
                limits.bytes,
            )
            .await?;
        let manifest =
            RemoteArtifactManifest::decode(&bytes, limits, original).map_err(io::Error::other)?;
        if manifest.producer() != selection.producer
            || manifest.artifact_type() != selection.kind
            || manifest.id() != selection.id
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let selected = select_version(
            &manifest,
            selection.version,
            VersionSelectionLimits {
                max_versions: limits.versions,
                max_version_bytes: usize::try_from(
                    policy.limit(ResourceSetting::ProjectionIdentityBytes).value,
                )
                .map_err(io::Error::other)?,
            },
            original,
        )
        .map_err(io::Error::other)?;
        let version = selected.version().clone();
        let archive_sha256 = *selected.hash();
        self.check()?;
        Ok(ResolvedFile {
            manifest,
            version,
            archive_sha256,
        })
    }

    pub(crate) async fn download(
        &self,
        resolved: &ResolvedFile,
        stage_parent: &Path,
        policy: &HubResourcePolicy,
    ) -> io::Result<TempDir> {
        self.check()?;
        if resolved.manifest.producer() != self.producer {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let original = self.lease.original().map_err(io::Error::other)?;
        let release = release_prefix(
            self.producer,
            resolved.manifest.artifact_type(),
            resolved.manifest.id(),
            &resolved.version,
            &resolved.archive_sha256,
        )?;
        let object = self.key(&format!("{release}payload.zip"))?;
        let response = wait(original.snapshot(), self.store.get(&object)).await?;
        let copy = policy.copy();
        if response.meta.size > copy.total_file_bytes_limit() {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let mut stream = response.into_stream();
        let mut hash = Sha256::new();
        let spool = NamedTempFile::new_in(stage_parent)?;
        let mut output = tokio::fs::File::from_std(spool.reopen()?);
        let mut bytes = 0_u64;
        while let Some(chunk) = wait(original.snapshot(), stream.try_next()).await? {
            self.check()?;
            bytes = bytes
                .checked_add(chunk.len() as u64)
                .ok_or(io::ErrorKind::FileTooLarge)?;
            if bytes > copy.total_file_bytes_limit() {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            hash.update(&chunk);
            wait(original.snapshot(), output.write_all(&chunk)).await?;
        }
        wait(original.snapshot(), output.flush()).await?;
        drop(output);
        self.check()?;
        if <[u8; 32]>::from(hash.finalize()) != resolved.archive_sha256 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let snapshot = Arc::clone(original.snapshot());
        let parent = stage_parent.to_owned();
        let stage = tokio::task::spawn_blocking(move || {
            let mut input = spool.reopen()?;
            stage_file_payload(&mut input, &parent, copy, &snapshot)
        })
        .await
        .map_err(io::Error::other)??;
        self.check()?;
        Ok(stage)
    }
}

pub(crate) async fn wait<T, E: std::error::Error + Send + Sync + 'static>(
    original: &SnapshotContext,
    future: impl Future<Output = Result<T, E>>,
) -> io::Result<T> {
    original.check()?;
    let result = tokio::select! {
        biased;
        () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
        () = tokio::time::sleep_until(original.required_deadline()?.into()) => return Err(io::ErrorKind::TimedOut.into()),
        result = future => result.map_err(|error| {
            let kind = (&error as &(dyn std::error::Error + 'static))
                .downcast_ref::<io::Error>().map_or(io::ErrorKind::Other, io::Error::kind);
            io::Error::new(kind, error)
        }),
    };
    match result {
        Ok(value) => {
            original.check()?;
            Ok(value)
        }
        Err(cause) => Err(cause),
    }
}

pub(crate) fn storage_origin(value: &str, allow_loopback_http: bool) -> io::Result<()> {
    let url = url::Url::parse(value).map_err(io::Error::other)?;
    let loopback = match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        None => false,
    };
    if url.host().is_none()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || !(url.scheme() == "https" || (url.scheme() == "http" && allow_loopback_http && loopback))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}
fn storage_prefix(value: &str) -> io::Result<&str> {
    let value = value.strip_suffix('/').unwrap_or(value);
    if !value.is_empty()
        && (value.chars().any(char::is_control)
            || value.contains(['\\', ':'])
            || value.split('/').any(|part| matches!(part, "" | "." | "..")))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(value)
}
