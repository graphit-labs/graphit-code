//! DATAONLY bindings from a real caller-supplied captured source. A fingerprint,
//! relative path or role does not authorize a later SDK reopen or Installation.
use crate::model_manifest::{
    self, BindingCause, BindingStage, ManifestLimits, ModelBindingError, ModelManifest,
    ModelSelection,
};
use graphit_core::artifact_store::{CapturedSourceReader, SnapshotContext};
use std::{
    io::{self, Read},
    path::Path,
};

pub struct ModelBindings {
    pub snapshot_fingerprint: [u8; 32],
    pub manifest: ModelManifest,
}

/// Read and select metadata from one actual captured manifest. An empty relative
/// path selects a captured root FILE. This declares no artifact integrity or Auth.
/// Callers own the final reader fence and all physical weight verification.
pub fn read_captured_model_manifest(
    reader: &CapturedSourceReader,
    original: &SnapshotContext,
    manifest_relative: &Path,
    expected: &ModelSelection<'_>,
    limits: &ManifestLimits,
) -> Result<ModelManifest, ModelBindingError> {
    reader
        .check_request(original)
        .map_err(|error| ModelBindingError::io(BindingStage::Request, error))?;
    limits.validate()?;
    if expected.id.len() > limits.max_id_bytes
        || expected.revision.len() > limits.max_revision_bytes
        || !model_manifest::exact_name(expected.id)
        || !model_manifest::exact_name(expected.revision)
    {
        return Err(ModelBindingError::new(
            BindingStage::Selection,
            BindingCause::Selection,
            io::ErrorKind::InvalidInput,
        ));
    }
    if !manifest_relative.as_os_str().is_empty()
        && manifest_relative.to_str().is_none_or(|path| {
            path.len() > limits.max_relative_path_bytes || !model_manifest::portable_path(path)
        })
    {
        return Err(ModelBindingError::new(
            BindingStage::Manifest,
            BindingCause::Path,
            io::ErrorKind::InvalidInput,
        ));
    }
    let node = reader
        .nodes()
        .find(|node| node.relative() == manifest_relative)
        .ok_or_else(|| {
            ModelBindingError::new(
                BindingStage::Manifest,
                BindingCause::Artifact,
                io::ErrorKind::NotFound,
            )
        })?;
    // Core defines None only for Directory and Some(digest) only for File.
    if node.digest().is_none() {
        return Err(ModelBindingError::new(
            BindingStage::Manifest,
            BindingCause::Artifact,
            io::ErrorKind::InvalidData,
        ));
    }
    let length = usize::try_from(node.bytes()).map_err(|_| {
        ModelBindingError::new(
            BindingStage::Manifest,
            BindingCause::Capacity,
            io::ErrorKind::FileTooLarge,
        )
    })?;
    if length == 0 || length > limits.max_manifest_bytes || length > isize::MAX as usize {
        return Err(ModelBindingError::new(
            BindingStage::Manifest,
            BindingCause::Capacity,
            io::ErrorKind::FileTooLarge,
        ));
    }
    reader
        .check_request(original)
        .map_err(|error| ModelBindingError::io(BindingStage::Request, error))?;
    let file = reader
        .open_file(manifest_relative)
        .map_err(|error| ModelBindingError::io(BindingStage::Manifest, error))?;
    if file.len() != node.bytes() {
        return Err(ModelBindingError::new(
            BindingStage::Manifest,
            BindingCause::Artifact,
            io::ErrorKind::InvalidData,
        ));
    }
    reader
        .check_request(original)
        .map_err(|error| ModelBindingError::io(BindingStage::Request, error))?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(length).map_err(|_| {
        ModelBindingError::new(
            BindingStage::Manifest,
            BindingCause::Allocation,
            io::ErrorKind::OutOfMemory,
        )
    })?;
    bytes.resize(length, 0);
    let mut stream = file
        .stream(0)
        .map_err(|error| ModelBindingError::io(BindingStage::Manifest, error))?;
    stream
        .read_exact(&mut bytes)
        .map_err(|error| ModelBindingError::io(BindingStage::Manifest, error))?;
    drop(stream);
    drop(file);
    let manifest =
        model_manifest::decode_manifest(&bytes, limits, &mut || reader.check_request(original))?;
    drop(bytes);
    if manifest.id != expected.id
        || manifest.revision != expected.revision
        || manifest.task != expected.task
    {
        return Err(ModelBindingError::new(
            BindingStage::Selection,
            BindingCause::Selection,
            io::ErrorKind::InvalidData,
        ));
    }
    Ok(manifest)
}

pub fn bind_captured_model(
    reader: &CapturedSourceReader,
    original: &SnapshotContext,
    manifest_relative: &Path,
    expected: &ModelSelection<'_>,
    limits: &ManifestLimits,
) -> Result<ModelBindings, ModelBindingError> {
    reader
        .check_request(original)
        .map_err(|error| ModelBindingError::io(BindingStage::Request, error))?;
    limits.validate()?;
    if manifest_relative.as_os_str().is_empty() {
        return Err(ModelBindingError::new(
            BindingStage::Manifest,
            BindingCause::Path,
            io::ErrorKind::InvalidInput,
        ));
    }
    let manifest =
        read_captured_model_manifest(reader, original, manifest_relative, expected, limits)?;
    for artifact in &manifest.artifacts {
        reader
            .check_request(original)
            .map_err(|error| ModelBindingError::io(BindingStage::Request, error))?;
        let path = Path::new(&artifact.path);
        if path == manifest_relative {
            return Err(ModelBindingError::new(
                BindingStage::Artifact,
                BindingCause::Path,
                io::ErrorKind::InvalidData,
            ));
        }
        let node = reader.nodes().find(|node| node.relative() == path);
        match node {
            None if !artifact.required => {}
            None => {
                return Err(ModelBindingError::new(
                    BindingStage::Artifact,
                    BindingCause::Artifact,
                    io::ErrorKind::NotFound,
                ));
            }
            Some(node)
                if node.digest() != Some(artifact.digest)
                    || node.bytes() < artifact.minimum_bytes =>
            {
                return Err(ModelBindingError::new(
                    BindingStage::Artifact,
                    BindingCause::Artifact,
                    io::ErrorKind::InvalidData,
                ));
            }
            Some(_) => {}
        }
    }
    reader
        .check_request(original)
        .map_err(|error| ModelBindingError::io(BindingStage::Request, error))?;
    #[cfg(test)]
    fences::run();
    reader
        .revalidate_content()
        .map_err(|error| ModelBindingError::io(BindingStage::Revalidate, error))?;
    reader
        .check_request(original)
        .map_err(|error| ModelBindingError::io(BindingStage::Request, error))?;
    Ok(ModelBindings {
        snapshot_fingerprint: reader.fingerprint(),
        manifest,
    })
}

// No production callback or observer API. A test peer mutates a real fixture
// precisely at the final fence; the actual Core proof still decides the result.
#[cfg(test)]
pub(crate) mod fences {
    use std::cell::RefCell;
    thread_local! {
        static BEFORE_REVALIDATE: RefCell<Option<Box<dyn FnOnce()>>> = const { RefCell::new(None) };
    }
    pub(crate) fn before_revalidate(action: impl FnOnce() + 'static) {
        BEFORE_REVALIDATE.with(|slot| {
            assert!(slot.borrow().is_none(), "fixture fence already registered");
            *slot.borrow_mut() = Some(Box::new(action));
        });
    }
    pub(super) fn run() {
        let action = BEFORE_REVALIDATE.with(|slot| slot.borrow_mut().take());
        if let Some(action) = action {
            action();
        }
    }
}
