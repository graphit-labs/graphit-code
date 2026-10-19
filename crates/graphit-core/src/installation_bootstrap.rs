//! Private installation directory preparation. This creates no authentication state.
use crate::{
    auth_document::AuthDocumentError,
    file_io::{RequestIdentity, SnapshotContext},
    native_document_file::{CapturedDocumentDirectory, NativeDocumentFileLimits},
    project_paths::InstallationPaths,
};
use std::{fs, fs::File, io, path::Path};

/// Explicit, positive limits. Directory metadata performs no content reads.
#[derive(Clone, Copy)]
pub struct BootstrapLimits {
    native: NativeDocumentFileLimits,
    path_bytes: usize,
    ancestors: usize,
}
impl BootstrapLimits {
    pub fn new(path_bytes: usize, ancestors: usize) -> Result<Self, AuthDocumentError> {
        Ok(Self {
            native: NativeDocumentFileLimits::new(1, path_bytes, ancestors, 1)
                .map_err(|_| AuthDocumentError::InvalidPolicy)?,
            path_bytes,
            ancestors,
        })
    }

    /// Directory-capture limits derived by this bootstrap's validated constructor.
    /// This is policy data; directory capture and original-request checks still apply.
    pub fn native_limits(&self) -> NativeDocumentFileLimits {
        self.native
    }
}

/// Effects remain visible even when qualification or the original request fails.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BootstrapEffects {
    pub created_directories: usize,
    /// A native creation failed and may have left its fixed directory entry.
    pub uncertain_creation: bool,
}

pub struct BootstrapOutcome {
    installation: Option<PrivateInstallation>,
    effects: BootstrapEffects,
    failure: Option<AuthDocumentError>,
}
impl BootstrapOutcome {
    pub fn effects(&self) -> BootstrapEffects {
        self.effects
    }
    pub fn failure(&self) -> Option<AuthDocumentError> {
        self.failure
    }
    pub fn installation(&self) -> Option<&PrivateInstallation> {
        self.installation.as_ref()
    }
    pub fn into_installation(self) -> Result<PrivateInstallation, AuthDocumentError> {
        self.installation
            .ok_or(self.failure.unwrap_or(AuthDocumentError::Unavailable))
    }
}

/// A current physical observation, never a login or permission capability.
///
/// ```compile_fail
/// use graphit_core::installation_bootstrap::PrivateInstallation;
/// let _: PrivateInstallation = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_core::installation_bootstrap::PrivateInstallation;
/// fn clone<T: Clone>() {} clone::<PrivateInstallation>();
/// ```
/// ```compile_fail
/// use graphit_core::installation_bootstrap::PrivateInstallation;
/// fn debug<T: std::fmt::Debug>() {} debug::<PrivateInstallation>();
/// ```
/// ```compile_fail
/// use graphit_core::installation_bootstrap::PrivateInstallation;
/// fn serializable<T: serde::Serialize>() {} serializable::<PrivateInstallation>();
/// ```
pub struct PrivateInstallation {
    paths: InstallationPaths,
    directory: CapturedDocumentDirectory,
    handle: File,
    request: RequestIdentity,
}
impl PrivateInstallation {
    pub(crate) fn into_runtime_parts(
        self,
        context: &SnapshotContext,
    ) -> Result<(CapturedDocumentDirectory, File, crate::brand::Brand), AuthDocumentError> {
        self.revalidate(context)?;
        // Brand is Arc-backed; retain this captured namespace without copying its strings.
        let brand = self.paths.brand().clone();
        Ok((self.directory, self.handle, brand))
    }

    pub(crate) fn runtime_directory(&self) -> &CapturedDocumentDirectory {
        &self.directory
    }

    /// Resolved path data; it does not restore this physical observation.
    pub fn paths(&self) -> &InstallationPaths {
        &self.paths
    }
    pub fn revalidate(&self, context: &SnapshotContext) -> Result<(), AuthDocumentError> {
        context.check()?;
        if !self.request.matches(context) {
            return Err(AuthDocumentError::Conflict);
        }
        self.directory
            .verify_private_handle(&self.handle, context)?;
        Ok(())
    }
}

/// Qualify an existing root or create its missing directories with private SDK birth.
/// Existing ancestors are never repaired. An unsuccessful preparation never rolls back.
pub fn prepare(
    paths: &InstallationPaths,
    limits: BootstrapLimits,
    context: &SnapshotContext,
) -> BootstrapOutcome {
    prepare_inner(
        paths,
        limits,
        context,
        graphit_fs::create_private_directory,
        |_, _| {},
    )
}

#[derive(Clone, Copy)]
enum Phase {
    BeforeCreate,
    AfterCreate,
}

fn prepare_inner(
    paths: &InstallationPaths,
    limits: BootstrapLimits,
    context: &SnapshotContext,
    mut create: impl FnMut(&Path) -> io::Result<File>,
    mut phase: impl FnMut(Phase, &Path),
) -> BootstrapOutcome {
    let mut effects = BootstrapEffects::default();
    let result = (|| -> Result<PrivateInstallation, AuthDocumentError> {
        context.check()?;
        let root = paths.root();
        if !root.is_absolute() {
            return Err(AuthDocumentError::InvalidPolicy);
        }
        limits.native.check_path(root)?;
        // InstallationPaths retains start independently of an absolute override.
        limits.native.check_path(paths.start_directory())?;
        let mut missing_count = 0usize;
        let mut existing = root;
        loop {
            context.check()?;
            limits.native.check_path(existing)?;
            match fs::symlink_metadata(existing) {
                Ok(metadata) if metadata.is_dir() => break,
                Ok(_) => return Err(AuthDocumentError::Denied),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    if existing.file_name().is_none() {
                        return Err(AuthDocumentError::InvalidPolicy);
                    }
                    missing_count = missing_count
                        .checked_add(1)
                        .ok_or(AuthDocumentError::Limit)?;
                    existing = existing.parent().ok_or(AuthDocumentError::InvalidPolicy)?;
                }
                Err(error) => return Err(error.into()),
            }
        }
        // Capture the real original parent before any namespace effects.
        let mut parent = CapturedDocumentDirectory::capture(existing, limits.native, context)?;
        let mut missing = Vec::new();
        missing
            .try_reserve_exact(missing_count)
            .map_err(|_| AuthDocumentError::Resource)?;
        for path in root.ancestors().take(missing_count) {
            missing.push(path.to_owned());
        }
        // Admit every future capture, including canonical aliases and retained
        // ancestor paths, before the first create operation.
        let mut planned = parent.canonical_path().to_owned();
        for path in missing.iter().rev() {
            context.check()?;
            let name = path.file_name().ok_or(AuthDocumentError::InvalidPolicy)?;
            let mut extra = name.as_encoded_bytes().len();
            if cfg!(windows) {
                extra = extra.checked_mul(2).ok_or(AuthDocumentError::Limit)?;
            }
            let separator = if cfg!(windows) { 2 } else { 1 };
            let cost = limits
                .native
                .path_cost(&planned)?
                .checked_add(extra)
                .and_then(|n| n.checked_add(separator))
                .ok_or(AuthDocumentError::Limit)?;
            if cost > limits.path_bytes {
                return Err(AuthDocumentError::Limit);
            }
            planned
                .try_reserve_exact(extra + separator)
                .map_err(|_| AuthDocumentError::Resource)?;
            planned.push(name);
            if planned.ancestors().count() > limits.ancestors {
                return Err(AuthDocumentError::Limit);
            }
            limits.native.check_path(path)?;
            limits.native.check_path(&planned)?;
        }
        let mut private_handle = None;
        for path in missing.iter().rev() {
            context.check()?;
            parent.revalidate(context)?;
            phase(Phase::BeforeCreate, path);
            context.check()?;
            parent.revalidate(context)?;
            let created = match create(path) {
                Ok(handle) => {
                    effects.created_directories += 1;
                    Some(handle)
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                Err(error) => {
                    effects.uncertain_creation = true;
                    return Err(error.into());
                }
            };
            phase(Phase::AfterCreate, path);
            context.check()?;
            parent.revalidate(context)?;
            let captured = CapturedDocumentDirectory::capture(path, limits.native, context)?;
            let handle = match created {
                Some(handle) => {
                    captured.verify_private_handle(&handle, context)?;
                    handle
                }
                None => captured.private_handle(context)?,
            };
            parent = captured;
            private_handle = Some(handle);
        }
        let handle = match private_handle {
            Some(handle) => handle,
            None => parent.private_handle(context)?,
        };
        parent.verify_private_handle(&handle, context)?;
        context.check()?;
        Ok(PrivateInstallation {
            paths: paths.clone(),
            directory: parent,
            handle,
            request: context.request_identity(),
        })
    })();
    match result {
        Ok(installation) => BootstrapOutcome {
            installation: Some(installation),
            effects,
            failure: None,
        },
        Err(error) => BootstrapOutcome {
            installation: None,
            effects,
            failure: Some(error),
        },
    }
}

#[cfg(test)]
mod tests;
