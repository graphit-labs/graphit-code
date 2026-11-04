//! Verified streaming artifact reads, independent of the native execution backend.
use crate::context::{ContextError, InferenceContext};
use graphit_core::{
    artifact_store::SnapshotContext, native_document_file::CapturedDocumentDirectory,
};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fmt,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelArtifactErrorKind {
    Context(ContextError),
    Configuration,
    Limit,
    Artifact,
    Integrity,
}
pub struct ModelArtifactError {
    kind: ModelArtifactErrorKind,
    cause: Option<Box<dyn Error + Send + Sync>>,
}
impl ModelArtifactError {
    pub fn kind(&self) -> ModelArtifactErrorKind {
        self.kind
    }
    fn invalid(kind: ModelArtifactErrorKind) -> Self {
        Self { kind, cause: None }
    }
    fn caused(kind: ModelArtifactErrorKind, cause: impl Error + Send + Sync + 'static) -> Self {
        Self {
            kind,
            cause: Some(Box::new(cause)),
        }
    }
}
impl From<ContextError> for ModelArtifactError {
    fn from(cause: ContextError) -> Self {
        Self::caused(ModelArtifactErrorKind::Context(cause), cause)
    }
}
impl fmt::Debug for ModelArtifactError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("ModelArtifactError")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for ModelArtifactError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "model artifact {:?}", self.kind)
    }
}
impl Error for ModelArtifactError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause.as_deref().map(|e| e as _)
    }
}
type Result<T> = std::result::Result<T, ModelArtifactError>;

pub(crate) fn read_artifact(
    root: &Path,
    path: &str,
    minimum: u64,
    digest: &[u8; 32],
    context: InferenceContext<'_>,
    mut consume: impl FnMut(&[u8]) -> std::io::Result<()>,
) -> Result<u64> {
    if !crate::model_manifest::portable_path(path) {
        return Err(ModelArtifactError::invalid(
            ModelArtifactErrorKind::Artifact,
        ));
    }
    context.check()?;
    let path = root
        .join(path)
        .canonicalize()
        .map_err(|e| ModelArtifactError::caused(ModelArtifactErrorKind::Artifact, e))?;
    if !path.starts_with(root) {
        return Err(ModelArtifactError::invalid(
            ModelArtifactErrorKind::Artifact,
        ));
    }
    let mut file = File::open(path)
        .map_err(|e| ModelArtifactError::caused(ModelArtifactErrorKind::Artifact, e))?;
    let meta = file
        .metadata()
        .map_err(|e| ModelArtifactError::caused(ModelArtifactErrorKind::Artifact, e))?;
    if !meta.is_file() || meta.len() < minimum {
        return Err(ModelArtifactError::invalid(
            ModelArtifactErrorKind::Integrity,
        ));
    }
    let mut length = 0u64;
    let mut sha = Sha256::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        context.check()?;
        let n = file
            .read(&mut chunk)
            .map_err(|e| ModelArtifactError::caused(ModelArtifactErrorKind::Artifact, e))?;
        if n == 0 {
            break;
        }
        sha.update(&chunk[..n]);
        length += n as u64;
        consume(&chunk[..n])
            .map_err(|e| ModelArtifactError::caused(ModelArtifactErrorKind::Artifact, e))?;
    }
    if length < minimum || sha.finalize().as_slice() != digest {
        return Err(ModelArtifactError::invalid(
            ModelArtifactErrorKind::Integrity,
        ));
    }
    Ok(length)
}

/// Model content and identity only. Consumers bind and authorize this descriptor before load.
/// Diagnostic/serialization traits are deliberately absent.
pub struct GgufModelSource {
    pub model: String,
    pub revision: String,
    pub root: PathBuf,
    pub path: String,
    pub minimum_bytes: u64,
    pub maximum_bytes: u64,
    pub sha256: [u8; 32],
}
pub(crate) struct CapturedGgufArtifact {
    pub model: String,
    pub revision: String,
    pub bundle: tempfile::TempDir,
}
/// Stages the verified bytes once; the native owner must drop before this bundle.
pub(crate) fn capture_gguf(
    snapshot_parent: &CapturedDocumentDirectory,
    source: GgufModelSource,
    context: InferenceContext<'_>,
) -> Result<CapturedGgufArtifact> {
    context.check()?;
    let result = (|| {
        if source.model.is_empty()
            || source.revision.is_empty()
            || source.minimum_bytes == 0
            || source.maximum_bytes < source.minimum_bytes
        {
            return Err(ModelArtifactError::invalid(
                ModelArtifactErrorKind::Configuration,
            ));
        }
        let io_error = |e| ModelArtifactError::caused(ModelArtifactErrorKind::Artifact, e);
        let root = source.root.canonicalize().map_err(io_error)?;
        let original = SnapshotContext::new(context.deadline(), context.cancellation().clone());
        let parent_handle = snapshot_parent
            .private_handle(&original)
            .map_err(io_error)?;
        let bundle = tempfile::Builder::new()
            .prefix(".gguf-")
            .tempdir_in(snapshot_parent.canonical_path())
            .map_err(io_error)?;
        snapshot_parent
            .verify_private_handle(&parent_handle, &original)
            .map_err(io_error)?;
        let mut file = File::create(bundle.path().join("model.gguf")).map_err(io_error)?;
        let mut bytes = 0u64;
        let mut exceeded = false;
        let staged = read_artifact(
            &root,
            &source.path,
            source.minimum_bytes,
            &source.sha256,
            context,
            |chunk| {
                let next = bytes
                    .checked_add(chunk.len() as u64)
                    .filter(|n| *n <= source.maximum_bytes);
                let Some(next) = next else {
                    exceeded = true;
                    return Err(std::io::Error::other("native artifact byte limit"));
                };
                file.write_all(chunk)?;
                bytes = next;
                Ok(())
            },
        );
        if let Err(mut error) = staged {
            if exceeded && !matches!(error.kind(), ModelArtifactErrorKind::Context(_)) {
                error.kind = ModelArtifactErrorKind::Limit;
            }
            return Err(error);
        }
        file.flush().map_err(io_error)?;
        drop(file);
        snapshot_parent
            .verify_private_handle(&parent_handle, &original)
            .map_err(io_error)?;
        Ok(CapturedGgufArtifact {
            model: source.model,
            revision: source.revision,
            bundle,
        })
    })();
    context.check()?;
    result
}

#[cfg(test)]
pub(crate) fn test_snapshot_parent()
-> (tempfile::TempDir, std::sync::Arc<CapturedDocumentDirectory>) {
    use graphit_core::{file_lock::Cancellation, native_document_file::NativeDocumentFileLimits};
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    let root = tempfile::tempdir().unwrap();
    let snapshot = root.path().join("snapshot");
    graphit_fs::create_private_directory(&snapshot).unwrap();
    let original = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    );
    let parent = CapturedDocumentDirectory::capture(
        &snapshot,
        NativeDocumentFileLimits::new(1, 4096, 64, 65536).unwrap(),
        &original,
    )
    .unwrap();
    // Establish the fixture's actual SDK privacy policy before testing artifact outcomes.
    parent.private_handle(&original).unwrap();
    (root, Arc::new(parent))
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    #[cfg(unix)]
    use graphit_core::native_document_file::NativeDocumentFileLimits;
    use std::time::{Duration, Instant};

    fn context(signal: &Cancellation) -> InferenceContext<'_> {
        InferenceContext::new(signal, Instant::now() + Duration::from_secs(30))
    }
    fn source(root: &Path, bytes: &[u8]) -> GgufModelSource {
        std::fs::write(root.join("input.gguf"), bytes).unwrap();
        GgufModelSource {
            model: "fixture".into(),
            revision: "pinned".into(),
            root: root.into(),
            path: "input.gguf".into(),
            minimum_bytes: 1,
            maximum_bytes: bytes.len() as u64,
            sha256: Sha256::digest(bytes).into(),
        }
    }
    #[test]
    fn gguf_snapshot_uses_explicit_private_parent_and_retains_verified_bytes() {
        let (_root, parent) = test_snapshot_parent();
        let input = tempfile::tempdir().unwrap();
        let bytes = vec![42; 128 * 1024 + 7];
        let cancel = Cancellation::default();
        let captured =
            capture_gguf(&parent, source(input.path(), &bytes), context(&cancel)).unwrap();
        assert_eq!(
            captured.bundle.path().parent(),
            Some(parent.canonical_path())
        );
        assert_eq!(
            std::fs::read(captured.bundle.path().join("model.gguf")).unwrap(),
            bytes
        );
        let staged = captured.bundle.path().to_owned();
        assert_eq!(captured.model, "fixture");
        assert_eq!(captured.revision, "pinned");
        assert!(staged.exists());
        drop(captured);
        assert!(!staged.exists());
        assert!(input.path().join("input.gguf").exists());
    }
    #[test]
    fn gguf_snapshot_digest_error_cleans_only_its_stage() {
        let (_root, parent) = test_snapshot_parent();
        let input = tempfile::tempdir().unwrap();
        let cancel = Cancellation::default();
        let mut source = source(input.path(), b"verified bytes");
        source.sha256 = [0; 32];
        let error = capture_gguf(&parent, source, context(&cancel))
            .err()
            .unwrap();
        assert_eq!(error.kind(), ModelArtifactErrorKind::Integrity);
        assert_eq!(
            std::fs::read_dir(parent.canonical_path()).unwrap().count(),
            0
        );
        assert!(input.path().join("input.gguf").exists());
    }
    #[test]
    fn gguf_snapshot_byte_limit_cleans_partial_stage() {
        let (_root, parent) = test_snapshot_parent();
        let input = tempfile::tempdir().unwrap();
        let cancel = Cancellation::default();
        let mut source = source(input.path(), &vec![42; 64 * 1024 + 1]);
        source.maximum_bytes = 64 * 1024;
        let error = capture_gguf(&parent, source, context(&cancel))
            .err()
            .unwrap();
        assert_eq!(error.kind(), ModelArtifactErrorKind::Limit);
        assert_eq!(
            std::fs::read_dir(parent.canonical_path()).unwrap().count(),
            0
        );
    }
    #[test]
    fn gguf_snapshot_cancellation_and_deadline_preserve_original_error_without_io() {
        let (_root, parent) = test_snapshot_parent();
        let input = tempfile::tempdir().unwrap();
        for expired in [false, true] {
            let cancel = Cancellation::default();
            if !expired {
                cancel.cancel();
            }
            let deadline = if expired {
                Instant::now()
            } else {
                Instant::now() + Duration::from_secs(30)
            };
            let error = capture_gguf(
                &parent,
                source(input.path(), b"bytes"),
                InferenceContext::new(&cancel, deadline),
            )
            .err()
            .unwrap();
            assert_eq!(
                error.kind(),
                ModelArtifactErrorKind::Context(if expired {
                    ContextError::Deadline
                } else {
                    ContextError::Cancelled
                })
            );
            assert_eq!(
                std::fs::read_dir(parent.canonical_path()).unwrap().count(),
                0
            );
        }
    }
    #[cfg(unix)]
    #[test]
    fn gguf_snapshot_rejects_replaced_parent_without_adopting_it() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("snapshots");
        graphit_fs::create_private_directory(&path).unwrap();
        let cancel = Cancellation::default();
        let original = SnapshotContext::new(context(&cancel).deadline(), cancel.clone());
        let parent = CapturedDocumentDirectory::capture(
            &path,
            NativeDocumentFileLimits::new(1, 4096, 64, 65536).unwrap(),
            &original,
        )
        .unwrap();
        std::fs::rename(&path, root.path().join("old")).unwrap();
        graphit_fs::create_private_directory(&path).unwrap();
        let input = tempfile::tempdir().unwrap();
        let error = capture_gguf(&parent, source(input.path(), b"bytes"), context(&cancel))
            .err()
            .unwrap();
        assert_eq!(error.kind(), ModelArtifactErrorKind::Artifact);
        assert_eq!(std::fs::read_dir(path).unwrap().count(), 0);
    }
    #[cfg(unix)]
    #[test]
    fn gguf_snapshot_missing_parent_fails_without_system_temp_fallback() {
        let (_root, parent) = test_snapshot_parent();
        std::fs::remove_dir(parent.canonical_path()).unwrap();
        let input = tempfile::tempdir().unwrap();
        let cancel = Cancellation::default();
        let error = capture_gguf(&parent, source(input.path(), b"bytes"), context(&cancel))
            .err()
            .unwrap();
        assert_eq!(error.kind(), ModelArtifactErrorKind::Artifact);
        assert!(!parent.canonical_path().exists());
    }
    #[cfg(unix)]
    #[test]
    fn gguf_snapshot_rejects_nonprivate_parent() {
        use std::os::unix::fs::PermissionsExt;
        let (_root, parent) = test_snapshot_parent();
        std::fs::set_permissions(
            parent.canonical_path(),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        let input = tempfile::tempdir().unwrap();
        let cancel = Cancellation::default();
        let error = capture_gguf(&parent, source(input.path(), b"bytes"), context(&cancel))
            .err()
            .unwrap();
        assert_eq!(error.kind(), ModelArtifactErrorKind::Artifact);
        assert_eq!(
            std::fs::read_dir(parent.canonical_path()).unwrap().count(),
            0
        );
    }
}
