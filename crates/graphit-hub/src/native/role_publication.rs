//! Publication of tool-owned native roles through the existing Core writer.
//! Authentication is supplied by the real caller before and after each effect.
use super::{RenderedContent, Role, RoleCredentialReference, RoleSource};
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
pub enum RolePublicationError {
    Input(io::Error),
    Physical(ArtifactError),
    /// The native file was published; a later guard or cleanup failed.
    AfterPublication {
        change: ArtifactChange,
        cause: io::Error,
    },
}
impl fmt::Display for RolePublicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("managed role publication failed")
    }
}
impl std::error::Error for RolePublicationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Input(e) | Self::AfterPublication { cause: e, .. } => e,
            Self::Physical(e) => e,
        })
    }
}
impl From<io::Error> for RolePublicationError {
    fn from(e: io::Error) -> Self {
        Self::Input(e)
    }
}
impl From<ArtifactError> for RolePublicationError {
    fn from(e: ArtifactError) -> Self {
        Self::Physical(e)
    }
}

/// Physical destination selected by the process owner, not by profile content.
#[derive(Clone, Copy)]
pub enum RoleDestination<'a> {
    Project(&'a Path),
    GlobalAgents(&'a Path),
}

/// Publish one of the three managed names. References are provenance data; the
/// callback must authorize Configure/PublishedFilesystem with the original caller.
/// Existing unowned files are never adopted and unrelated native files are untouched.
pub fn publish_role(
    store: &ArtifactStore,
    destination: RoleDestination<'_>,
    rendered: RenderedContent,
    reference: &RoleCredentialReference,
    directory_limits: NativeDocumentFileLimits,
    request: &RequestContext<'_>,
    mut before: impl FnMut() -> io::Result<()>,
) -> Result<ArtifactChange, RolePublicationError> {
    request.remaining()?;
    before()?;
    let (target_root, global) = match destination {
        RoleDestination::Project(root) => (root, false),
        RoleDestination::GlobalAgents(root) => (root, true),
    };
    if global
        && !matches!(
            store.owner(),
            graphit_core::artifact_store::StoreOwner::Installation { .. }
        )
    {
        return Err(RolePublicationError::Input(
            io::ErrorKind::InvalidInput.into(),
        ));
    }
    let RoleSource::ToolOwned(brand) = reference.source() else {
        return Err(RolePublicationError::Input(
            io::ErrorKind::InvalidInput.into(),
        ));
    };
    if brand != store.scope().brand()
        || !matches!(reference.role(), Role::Scout | Role::Tracker | Role::Scribe)
        || !store
            .scope()
            .allowed_roots()
            .any(|root| root == target_root)
        || rendered.provenance.effective_name
            != format!("{}-{}", brand.key(), reference.role().as_str())
        || rendered.relative_entry
            != rendered
                .provenance
                .binding
                .agent_path(&rendered.provenance.effective_name)
    {
        return Err(RolePublicationError::Input(
            io::ErrorKind::InvalidInput.into(),
        ));
    }
    let entry = Path::new(&rendered.relative_entry);
    let relative = if global {
        Path::new(
            entry
                .file_name()
                .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?,
        )
    } else {
        entry
    };
    let parent = relative
        .parent()
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let mut current = target_root.to_owned();
    for component in parent.components() {
        let std::path::Component::Normal(component) = component else {
            return Err(RolePublicationError::Input(
                io::ErrorKind::InvalidInput.into(),
            ));
        };
        request.remaining()?;
        before()?;
        let held =
            CapturedDocumentDirectory::capture(&current, directory_limits, request.snapshot())?;
        let child = current.join(component);
        match fs::symlink_metadata(&child) {
            Ok(meta) if meta.is_dir() && !meta.is_symlink() => {}
            Ok(_) => {
                return Err(RolePublicationError::Input(
                    io::ErrorKind::InvalidData.into(),
                ));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let _created = graphit_fs::create_private_directory(&child)?;
            }
            Err(e) => return Err(e.into()),
        }
        held.revalidate(request.snapshot())?;
        before()?;
        current = child;
    }
    let held = CapturedDocumentDirectory::capture(&current, directory_limits, request.snapshot())?;
    let namespace = if global {
        let index = store
            .scope()
            .allowed_roots()
            .position(|root| root == target_root)
            .ok_or_else(|| io::Error::from(io::ErrorKind::PermissionDenied))?;
        std::borrow::Cow::Owned(format!("global-native-{index}"))
    } else {
        std::borrow::Cow::Borrowed(rendered.provenance.binding.as_str())
    };
    let key = ArtifactKey::new(
        &namespace,
        "managed-role",
        if global {
            relative
                .to_str()
                .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?
        } else {
            &rendered.provenance.effective_name
        },
    )?;
    let mode = file_mode(&fs::metadata(&current)?.permissions())?;
    let copy = store.limits().copy_limits();
    let plan = ValidatedPayloadPlan::generated(
        [GeneratedNode::file(PathBuf::new(), rendered.bytes, mode)?],
        &PayloadPolicy::new(NodeKind::File, mode, copy),
        request.snapshot(),
    )?;
    let mut writer = store.writer_with_context(request.snapshot())?;
    writer.recover()?;
    before()?;
    held.revalidate(request.snapshot())?;
    let change = writer.publish_generated(
        &key,
        &plan,
        &target_root.join(relative),
        NodeKind::File,
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
    )?;
    let after = (|| -> io::Result<()> {
        before()?;
        held.revalidate(request.snapshot())?;
        Ok(())
    })();
    if let Err(cause) = after {
        return Err(RolePublicationError::AfterPublication { change, cause });
    }
    Ok(change)
}
fn file_mode(permissions: &fs::Permissions) -> io::Result<PayloadMode> {
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
