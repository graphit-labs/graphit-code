//! One authentic full PROJECT FILE allocation for native extraction.
//! Capture hashes the FILE and verifies the retained range against that digest.
//! Later accesses validate scope, controls and native identity/size/timestamps;
//! Unix observations include ctime, avoiding repeated payload hashes. Other
//! platforms retain content checks until equivalent mutation metadata is captured.

use graphit_auth::ProjectCapture;
use graphit_core::artifact_store::{
    CapturedSourceFile, CapturedSourceReader, CopyLimits, CopyPolicy, RetainedBytesLimits,
    RetainedFileSet, RetainedSourceRange, ScopeContext, SnapshotContext, SnapshotPolicy,
    SourceReadLimits, StoreOwner,
};
use graphit_fs::NodeKind;
use sha2::{Digest, Sha256};
use std::{
    collections::TryReserveError,
    error::Error,
    ffi::OsStr,
    fmt, io,
    path::{Component, Path, PathBuf},
    str::Utf8Error,
    sync::Arc,
};

pub struct NativeNnPhysicalSource<'project> {
    project: ProjectCapture<'project>,
    original: Arc<SnapshotContext>,
    relative_path: String,
    relative_components: Vec<String>,
    reader: CapturedSourceReader,
    file: CapturedSourceFile,
    range: RetainedSourceRange,
    content_sha256: [u8; 32],
}

#[derive(Debug)]
pub enum NativeNnPhysicalSourceError {
    Operation(io::Error),
    Utf8(Utf8Error),
    Allocation(TryReserveError),
    InvalidInput,
    RequestMismatch,
    Representation,
}
impl fmt::Display for NativeNnPhysicalSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(error) => write!(f, "physical PROJECT Source: {error}"),
            Self::Utf8(error) => write!(f, "physical PROJECT Source UTF-8: {error}"),
            Self::Allocation(error) => write!(f, "physical PROJECT Source allocation: {error}"),
            Self::InvalidInput => f.write_str("invalid physical PROJECT Source association"),
            Self::RequestMismatch => f.write_str("physical PROJECT Source Original mismatch"),
            Self::Representation => f.write_str("physical PROJECT Source representation overflow"),
        }
    }
}
impl Error for NativeNnPhysicalSourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(error) => Some(error),
            Self::Utf8(error) => Some(error),
            Self::Allocation(error) => Some(error),
            _ => None,
        }
    }
}
impl From<io::Error> for NativeNnPhysicalSourceError {
    fn from(error: io::Error) -> Self {
        Self::Operation(error)
    }
}

fn validate_project(
    project: ProjectCapture<'_>,
    original: &SnapshotContext,
) -> Result<(), NativeNnPhysicalSourceError> {
    original.check()?;
    if !matches!(project.scope.live.owner(), StoreOwner::Project { .. })
        || project.version.scope() != project.scope.live.owner_root()
    {
        return Err(NativeNnPhysicalSourceError::InvalidInput);
    }
    let context = ScopeContext::from_snapshot(original);
    if !project
        .scope
        .record
        .matches_scope_bounded(project.scope.live, &context)?
    {
        return Err(NativeNnPhysicalSourceError::InvalidInput);
    }
    project.scope.live.revalidate_bounded(&context)?;
    original.check()?;
    Ok(())
}

fn selection(
    components: &[&str],
    original: &SnapshotContext,
) -> Result<(PathBuf, String, Vec<String>), NativeNnPhysicalSourceError> {
    if components.is_empty() {
        return Err(NativeNnPhysicalSourceError::InvalidInput);
    }
    let mut bytes = components.len() - 1;
    for component in components {
        original.check()?;
        let mut parts = Path::new(component).components();
        if component.is_empty()
            || component.contains('\0')
            || !matches!(parts.next(), Some(Component::Normal(value)) if value == OsStr::new(component))
            || parts.next().is_some()
            || (cfg!(windows) && component.contains(':'))
        {
            return Err(NativeNnPhysicalSourceError::InvalidInput);
        }
        bytes = bytes
            .checked_add(component.len())
            .ok_or(NativeNnPhysicalSourceError::Representation)?;
    }
    let mut native = PathBuf::new();
    native
        .try_reserve(bytes)
        .map_err(NativeNnPhysicalSourceError::Allocation)?;
    let mut logical = String::new();
    logical
        .try_reserve_exact(bytes)
        .map_err(NativeNnPhysicalSourceError::Allocation)?;
    let mut owned = Vec::new();
    owned
        .try_reserve_exact(components.len())
        .map_err(NativeNnPhysicalSourceError::Allocation)?;
    for component in components {
        original.check()?;
        let mut name = String::new();
        name.try_reserve_exact(component.len())
            .map_err(NativeNnPhysicalSourceError::Allocation)?;
        name.push_str(component);
        if !logical.is_empty() {
            logical.push('/');
        }
        logical.push_str(component);
        native.push(component);
        owned.push(name);
    }
    original.check()?;
    Ok((native, logical, owned))
}

/// Capture a current physical file; scope/version observations grant no access.
pub fn capture_native_nn_physical_source<'project>(
    project: ProjectCapture<'project>,
    relative_components: &[&str],
    copy: CopyLimits,
    retention: RetainedBytesLimits,
    limits: SourceReadLimits,
    original: &Arc<SnapshotContext>,
) -> Result<NativeNnPhysicalSource<'project>, NativeNnPhysicalSourceError> {
    original.check()?;
    let result = capture_checked(
        project,
        relative_components,
        copy,
        retention,
        limits,
        original,
    );
    original.check()?;
    result
}

fn capture_checked<'project>(
    project: ProjectCapture<'project>,
    components: &[&str],
    copy: CopyLimits,
    retention: RetainedBytesLimits,
    limits: SourceReadLimits,
    original: &Arc<SnapshotContext>,
) -> Result<NativeNnPhysicalSource<'project>, NativeNnPhysicalSourceError> {
    validate_project(project, original)?;
    let (selection, relative_path, relative_components) = selection(components, original)?;
    // No body is retained by capture; exactly the subsequent full range owns it.
    let policy = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        copy,
        RetainedFileSet::new([], retention)?,
        retention,
    )?;
    let reader = CapturedSourceReader::capture_project(
        project.scope.live,
        &selection,
        NodeKind::File,
        &policy,
        limits,
        original,
    )?;
    reader.check_request(original)?;
    reader.revalidate_bindings()?;
    if reader.retained_bytes() != 0 {
        return Err(NativeNnPhysicalSourceError::InvalidInput);
    }
    let file = reader.open_file(Path::new(""))?;
    let length =
        usize::try_from(file.len()).map_err(|_| NativeNnPhysicalSourceError::Representation)?;
    let range = file.read_range(0, length)?;
    std::str::from_utf8(range.as_ref()).map_err(NativeNnPhysicalSourceError::Utf8)?;
    let node = reader
        .nodes()
        .find(|node| node.relative().as_os_str().is_empty())
        .ok_or(NativeNnPhysicalSourceError::InvalidInput)?;
    if node.kind() != NodeKind::File || node.bytes() != file.len() || range.len() != length {
        return Err(NativeNnPhysicalSourceError::InvalidInput);
    }
    let expected = node
        .digest()
        .ok_or(NativeNnPhysicalSourceError::InvalidInput)?;
    let mut hash = Sha256::new();
    for chunk in range.as_ref().chunks(64 * 1024) {
        original.check()?;
        hash.update(chunk);
    }
    let content_sha256: [u8; 32] = hash.finalize().into();
    if content_sha256 != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "physical Source content digest mismatch",
        )
        .into());
    }
    let captured = NativeNnPhysicalSource {
        project,
        original: Arc::clone(original),
        relative_path,
        relative_components,
        reader,
        file,
        range,
        content_sha256,
    };
    captured.revalidate(original)?;
    Ok(captured)
}

impl NativeNnPhysicalSource<'_> {
    fn check_original(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> Result<(), NativeNnPhysicalSourceError> {
        original.check()?;
        if !Arc::ptr_eq(original, &self.original) {
            return Err(NativeNnPhysicalSourceError::RequestMismatch);
        }
        self.reader.check_request(original)?;
        Ok(())
    }
    pub fn revalidate(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> Result<(), NativeNnPhysicalSourceError> {
        self.check_original(original)?;
        let result = self.revalidate_checked(original);
        self.check_original(original)?;
        result
    }
    fn revalidate_checked(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> Result<(), NativeNnPhysicalSourceError> {
        validate_project(self.project, original)?;
        #[cfg(unix)]
        self.reader.revalidate_bindings()?;
        #[cfg(not(unix))]
        self.reader.revalidate_content()?;
        if usize::try_from(self.file.len())
            .map_err(|_| NativeNnPhysicalSourceError::Representation)?
            != self.range.len()
        {
            return Err(NativeNnPhysicalSourceError::InvalidInput);
        }
        validate_project(self.project, original)
    }
    pub fn source(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> Result<&str, NativeNnPhysicalSourceError> {
        self.revalidate(original)?;
        let result =
            std::str::from_utf8(self.range.as_ref()).map_err(NativeNnPhysicalSourceError::Utf8);
        self.revalidate(original)?;
        result
    }
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }
    pub fn relative_components(&self) -> &[String] {
        &self.relative_components
    }
    pub fn logical_version(&self) -> &str {
        self.project.version.version()
    }
    pub fn lance_branch(&self) -> String {
        self.project.version.lance_branch()
    }
    pub fn content_sha256(&self) -> [u8; 32] {
        self.content_sha256
    }
}
