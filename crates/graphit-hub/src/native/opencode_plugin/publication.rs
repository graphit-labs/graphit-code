//! Exclusive plugin files use the ordinary Core generated-file publisher.
use super::OpenCodePluginPlan;
use crate::policy::RequestContext;
use graphit_core::{
    artifact_store::{
        ArtifactChange, ArtifactError, ArtifactKey, ArtifactMode, ArtifactStore, CopyPolicy,
        GeneratedNode, PayloadMode, PayloadPolicy, ValidatedPayloadPlan,
    },
    native_document_file::{CapturedDocumentDirectory, NativeDocumentFileLimits},
};
use graphit_fs::NodeKind;
use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub enum PluginPublicationError {
    Input(io::Error),
    Physical(ArtifactError),
    AfterPublication {
        change: ArtifactChange,
        cause: io::Error,
    },
}
impl fmt::Display for PluginPublicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("native plugin publication failed")
    }
}
impl std::error::Error for PluginPublicationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Input(e) | Self::AfterPublication { cause: e, .. } => e,
            Self::Physical(e) => e,
        })
    }
}
impl From<io::Error> for PluginPublicationError {
    fn from(e: io::Error) -> Self {
        Self::Input(e)
    }
}
impl From<ArtifactError> for PluginPublicationError {
    fn from(e: ArtifactError) -> Self {
        Self::Physical(e)
    }
}
fn key(identifier: &str) -> io::Result<ArtifactKey> {
    if identifier.is_empty()
        || !identifier
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    ArtifactKey::new("opencode", "native-plugin", identifier)
}
fn mode(permissions: &fs::Permissions) -> io::Result<PayloadMode> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = permissions;
        PayloadMode::from_permissions(&fs::Permissions::from_mode(0o600))
    }
    #[cfg(not(unix))]
    {
        PayloadMode::from_permissions(permissions)
    }
}

/// Authentication and the selected PROJECT scope are supplied by the real caller.
/// Core alone owns the file's receipts, pending publication, recovery and removal.
pub fn publish_opencode_plugin(
    store: &ArtifactStore,
    target_root: &Path,
    plan: &OpenCodePluginPlan,
    directory_limits: NativeDocumentFileLimits,
    request: &RequestContext<'_>,
    mut before: impl FnMut() -> io::Result<()>,
) -> Result<ArtifactChange, PluginPublicationError> {
    request.remaining()?;
    before()?;
    let key = key(plan.identifier())?;
    if !matches!(
        store.owner(),
        graphit_core::artifact_store::StoreOwner::Project { .. }
    ) || !store
        .scope()
        .allowed_roots()
        .any(|root| root == target_root)
    {
        return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
    }
    let mut parent_path = target_root.to_owned();
    for part in [".opencode", "plugins"] {
        request.remaining()?;
        before()?;
        let held =
            CapturedDocumentDirectory::capture(&parent_path, directory_limits, request.snapshot())?;
        let next = parent_path.join(part);
        match fs::symlink_metadata(&next) {
            Ok(m) if m.is_dir() && !m.is_symlink() => {}
            Ok(_) => return Err(io::Error::from(io::ErrorKind::InvalidData).into()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                graphit_fs::create_private_directory(&next)?;
            }
            Err(e) => return Err(e.into()),
        }
        held.revalidate(request.snapshot())?;
        before()?;
        parent_path = next;
    }
    let held =
        CapturedDocumentDirectory::capture(&parent_path, directory_limits, request.snapshot())?;
    let mode = mode(&fs::metadata(&parent_path)?.permissions())?;
    let payload = ValidatedPayloadPlan::generated(
        [GeneratedNode::file(
            PathBuf::new(),
            plan.bytes().to_vec(),
            mode,
        )?],
        &PayloadPolicy::new(NodeKind::File, mode, store.limits().copy_limits()),
        request.snapshot(),
    )?;
    let mut writer = store.writer_with_context(request.snapshot())?;
    writer.recover()?;
    before()?;
    held.revalidate(request.snapshot())?;
    let change = writer.publish_generated(
        &key,
        &payload,
        &target_root.join(plan.relative_entry()),
        NodeKind::File,
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
    )?;
    let after = (|| -> io::Result<()> {
        request.remaining()?;
        before()?;
        held.revalidate(request.snapshot())?;
        Ok(())
    })();
    if let Err(cause) = after {
        return Err(PluginPublicationError::AfterPublication { change, cause });
    }
    Ok(change)
}

pub fn remove_opencode_plugin(
    store: &ArtifactStore,
    identifier: &str,
    request: &RequestContext<'_>,
    mut before: impl FnMut() -> io::Result<()>,
) -> Result<ArtifactChange, PluginPublicationError> {
    request.remaining()?;
    before()?;
    let key = key(identifier)?;
    let mut writer = store.writer_with_context(request.snapshot())?;
    writer.recover()?;
    before()?;
    let change = writer.remove(&key)?;
    if let Err(cause) = request.remaining().and_then(|_| before()) {
        return Err(PluginPublicationError::AfterPublication { change, cause });
    }
    Ok(change)
}
