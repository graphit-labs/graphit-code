//! Reuse the Core-owned copy receipt; catalog digest is part of its identity.
use super::{FileTransport, ResolvedFile};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_core::{
    artifact_store::{
        ArtifactChange, ArtifactKey, ArtifactMode, ArtifactStore, CopyPolicy, PhysicalSource,
    },
    file_io::SnapshotContext,
};
use graphit_fs::NodeKind;
use graphit_hub::{SourceRef, policy::HubResourcePolicy};
use sha2::{Digest as _, Sha256};
use std::{io, path::Path, sync::Arc, time::Instant};

impl FileTransport<'_, '_> {
    pub(crate) fn verify_cache(
        &self,
        source: &SourceRef,
        store: &ArtifactStore,
        destination: &Path,
    ) -> io::Result<bool> {
        self.check()?;
        match std::fs::symlink_metadata(destination) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err(io::ErrorKind::InvalidData.into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        }
        let key = cache_key(source)?;
        store.limits().validate_key(&key)?;
        let original = Arc::clone(self.lease.original().map_err(io::Error::other)?.snapshot());
        let verified = writer(store, &original)?
            .verify_owned_copy(&key, destination, NodeKind::Directory)
            .map_err(|error| io::Error::new(error.kind(), error))?;
        self.check()?;
        Ok(verified)
    }

    /// The caller admits the filesystem effect and creates normal parent directories.
    /// No lock or write permission is held while downloading the remote object.
    pub(crate) async fn cache(
        &self,
        resolved: &ResolvedFile,
        store: &ArtifactStore,
        source: &SourceRef,
        stage_parent: &Path,
        policy: &HubResourcePolicy,
    ) -> io::Result<ArtifactChange> {
        self.check()?;
        if source.producer() != resolved.manifest.producer()
            || source.artifact_type() != resolved.manifest.artifact_type()
            || source.id() != resolved.manifest.id()
            || source.version() != &resolved.version
            || source.archive_sha256() != Some(&resolved.archive_sha256)
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let destination = source.directory().as_path();
        let key = cache_key(source)?;
        store.limits().validate_key(&key)?;
        let original = Arc::clone(self.lease.original().map_err(io::Error::other)?.snapshot());
        let cached = writer(store, &original)?
            .verify_owned_copy(&key, destination, NodeKind::Directory)
            .map_err(|error| io::Error::new(error.kind(), error))?;
        self.check()?;
        if cached {
            return Ok(ArtifactChange::Unchanged);
        }
        let stage = self.download(resolved, stage_parent, policy).await?;
        self.commit_cache_stage(resolved, store, source, &stage, &|| self.check())
    }

    /// Commit only the stage downloaded for this same resolved FILE. The caller
    /// admits the local phase before creating parents or entering this writer.
    pub(crate) fn commit_cache_stage(
        &self,
        resolved: &ResolvedFile,
        store: &ArtifactStore,
        source: &SourceRef,
        stage: &tempfile::TempDir,
        check: &dyn Fn() -> io::Result<()>,
    ) -> io::Result<ArtifactChange> {
        self.check()?;
        if resolved.manifest.producer() != self.producer
            || source.producer() != resolved.manifest.producer()
            || source.artifact_type() != resolved.manifest.artifact_type()
            || source.id() != resolved.manifest.id()
            || source.version() != &resolved.version
            || source.archive_sha256() != Some(&resolved.archive_sha256)
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let destination = source.directory().as_path();
        let key = cache_key(source)?;
        store.limits().validate_key(&key)?;
        let original = Arc::clone(self.lease.original().map_err(io::Error::other)?.snapshot());
        let mut writer = writer(store, &original)?;
        self.check()?;
        check()?;
        if writer
            .verify_owned_copy(&key, destination, NodeKind::Directory)
            .map_err(|error| io::Error::new(error.kind(), error))?
        {
            return Ok(ArtifactChange::Unchanged);
        }
        self.check()?;
        check()?;
        let change = writer
            .publish(
                &key,
                PhysicalSource::ExternalSource(stage.path()),
                destination,
                NodeKind::Directory,
                ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
            )
            .map_err(|error| io::Error::new(error.kind(), error))?;
        // Return the known physical effect before the handler checks authority again.
        // A later expiry must not erase an already committed cache publication.
        Ok(change)
    }
}

fn cache_key(source: &SourceRef) -> io::Result<ArtifactKey> {
    let origin = source
        .remote_file_origin()
        .ok_or(io::ErrorKind::InvalidData)?;
    let digest = source.archive_sha256().ok_or(io::ErrorKind::InvalidData)?;
    let identity = serde_json::to_vec(&(
        origin,
        source.producer(),
        source.id(),
        source.version(),
        digest,
    ))
    .map_err(io::Error::other)?;
    ArtifactKey::new(
        "hub-cache",
        source.artifact_type().as_str(),
        &URL_SAFE_NO_PAD.encode(Sha256::digest(identity)),
    )
}

fn writer(
    store: &ArtifactStore,
    original: &Arc<SnapshotContext>,
) -> io::Result<graphit_core::artifact_store::ArtifactWriter> {
    original.check()?;
    store
        .writer(
            original
                .required_deadline()?
                .saturating_duration_since(Instant::now()),
            original.cancellation(),
        )
        .and_then(|writer| writer.with_io_context(Arc::clone(original)))
        .map_err(|error| io::Error::new(error.kind(), error))
}
