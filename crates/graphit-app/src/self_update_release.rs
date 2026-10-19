//! Prepare the maintained public release package; no installation authority or effects.
mod metadata;
pub use metadata::{MetadataError, Target};

use flate2::read::MultiGzDecoder;
use graphit_core::{artifact_store::CopyLimits, file_io::SnapshotContext};
use graphit_http::client::{
    BoundedClient, ClientPolicy, Limits as HttpLimits, NetworkError, NetworkRequest,
    NetworkRequestContext, NetworkResponseSink, NetworkStreamError,
};
use reqwest::{
    Method, StatusCode,
    header::{CONTENT_LENGTH, HeaderMap, HeaderValue, LOCATION},
};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};
use tempfile::TempDir;
use url::Url;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub http: HttpLimits,
    pub tree: CopyLimits,
    pub metadata_bytes: usize,
    /// Includes TAR headers, padding and extension records as well as file bytes.
    pub decoded_archive_bytes: u64,
    pub redirects: usize,
}
#[derive(Debug)]
pub enum PrepareError {
    Metadata(MetadataError),
    Network(NetworkError),
    Io(io::ErrorKind),
    HttpStatus(u16),
    Redirect,
    Size,
    Checksum,
    Archive,
    Layout,
    UnsupportedMacOS { required: String, current: String },
    Limit,
    Worker,
}
impl std::fmt::Display for PrepareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "release preparation: {self:?}")
    }
}
impl std::error::Error for PrepareError {}
impl From<io::Error> for PrepareError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}
impl From<MetadataError> for PrepareError {
    fn from(error: MetadataError) -> Self {
        Self::Metadata(error)
    }
}

/// Owns only this operation's private staging tree. This is not an install grant.
pub struct PreparedRelease {
    tag: String,
    target: Target,
    digest: [u8; 32],
    package: PathBuf,
    root_assets: Vec<String>,
    _private_directory: File,
    _container: TempDir,
}
impl std::fmt::Debug for PreparedRelease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedRelease")
            .field("target", &self.target)
            .finish_non_exhaustive()
    }
}
impl PreparedRelease {
    pub fn tag(&self) -> &str {
        &self.tag
    }
    pub fn target(&self) -> Target {
        self.target
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
    /// Explicit inspection; callers still need their actual installation admission.
    pub fn directory(&self) -> &Path {
        &self.package
    }
    pub fn root_assets(&self) -> &[String] {
        &self.root_assets
    }
}
#[derive(Debug)]
pub enum Preparation {
    Development,
    Current,
    Ready(PreparedRelease),
}

struct Source {
    metadata: Url,
    asset_origin: Url,
    allow_loopback_http: bool,
}
impl Source {
    fn production() -> Result<Self, PrepareError> {
        Ok(Self {
            metadata: Url::parse(
                "https://api.github.com/repos/graphit-labs/graphit-code/releases/latest",
            )
            .map_err(|_| PrepareError::Redirect)?,
            asset_origin: Url::parse("https://github.com").map_err(|_| PrepareError::Redirect)?,
            allow_loopback_http: false,
        })
    }
}

/// Version, target, limits and staging parent are captured by the caller.
/// Development builds do not fetch a release. No daemon, destination or credential path is accepted.
pub async fn prepare(
    original: Arc<SnapshotContext>,
    current_version: &str,
    target: Target,
    staging_parent: &Path,
    limits: Limits,
) -> Result<Preparation, PrepareError> {
    prepare_from(
        original,
        current_version,
        target,
        staging_parent,
        limits,
        Source::production()?,
    )
    .await
}
async fn prepare_from(
    original: Arc<SnapshotContext>,
    current_version: &str,
    target: Target,
    staging_parent: &Path,
    limits: Limits,
    source: Source,
) -> Result<Preparation, PrepareError> {
    original.required_deadline()?;
    original.check()?;
    if limits.decoded_archive_bytes == 0
        || limits.metadata_bytes == 0
        || limits.metadata_bytes > limits.http.response_body_bytes
    {
        return Err(PrepareError::Limit);
    }
    let mut metadata_limits = limits;
    metadata_limits.http.response_body_bytes = limits.metadata_bytes;
    let Some(current) = metadata::current_version(current_version)? else {
        return Ok(Preparation::Development);
    };
    let mut document = Vec::new();
    receive(
        &source,
        source.metadata.clone(),
        &original,
        metadata_limits,
        None,
        &mut document,
    )
    .await?;
    let release = match metadata::select(&document, &current, target, &source.asset_origin)? {
        metadata::Decision::Current => return Ok(Preparation::Current),
        metadata::Decision::Available(release) => *release,
    };
    let mut sidecar = Vec::new();
    receive(
        &source,
        release.checksum.url.clone(),
        &original,
        metadata_limits,
        Some(release.checksum.bytes),
        &mut sidecar,
    )
    .await?;
    let expected = metadata::checksum(&sidecar, &target.archive_name())?;
    original.check()?;
    let container = tempfile::Builder::new()
        .prefix(".graphit-release-")
        .tempdir_in(staging_parent)?;
    let owned = container.path().join("owned");
    let private_directory = graphit_fs::create_private_directory(&owned)?;
    let archive_path = owned.join("release.tar.gz");
    let mut archive = graphit_fs::create_private_file(&archive_path)?;
    let actual = receive(
        &source,
        release.archive.url,
        &original,
        limits,
        Some(release.archive.bytes),
        &mut archive,
    )
    .await?;
    if actual != expected {
        return Err(PrepareError::Checksum);
    }
    original.check()?;
    archive.flush()?;
    archive.seek(SeekFrom::Start(0))?;
    // If the waiting future is dropped, the actual blocking worker retains all staging
    // ownership until its bounded, Original-checked work ends. Drop is not a join receipt.
    tokio::task::spawn_blocking(move || {
        let tree = owned.join("tree");
        let _tree_handle = graphit_fs::create_private_directory(&tree)?;
        let root_assets = extract(archive, &tree, release.target, &original, limits)?;
        original.check()?;
        Ok(Preparation::Ready(PreparedRelease {
            tag: release.tag,
            target: release.target,
            digest: actual,
            package: tree.join(release.target.root_name()),
            root_assets,
            _private_directory: private_directory,
            _container: container,
        }))
    })
    .await
    .map_err(|_| PrepareError::Worker)?
}

enum SinkError {
    Redirect(Url),
    Failure(PrepareError),
}
struct Download<'a, W> {
    writer: &'a mut W,
    url: &'a Url,
    expected: Option<u64>,
    bytes: u64,
    digest: Sha256,
}
impl<W: Write> NetworkResponseSink for Download<'_, W> {
    type Error = SinkError;
    fn head(&mut self, status: StatusCode, headers: &HeaderMap) -> Result<(), SinkError> {
        if matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308) {
            let next = headers
                .get(LOCATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| self.url.join(v).ok())
                .ok_or(SinkError::Failure(PrepareError::Redirect))?;
            // Stop before the SDK delivers any redirect body.
            return Err(SinkError::Redirect(next));
        }
        if status != StatusCode::OK {
            return Err(SinkError::Failure(PrepareError::HttpStatus(
                status.as_u16(),
            )));
        }
        if let Some(length) = headers.get(CONTENT_LENGTH) {
            let length = length
                .to_str()
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .ok_or(SinkError::Failure(PrepareError::Size))?;
            if self.expected.is_some_and(|expected| length != expected) {
                return Err(SinkError::Failure(PrepareError::Size));
            }
        }
        Ok(())
    }
    fn chunk(&mut self, bytes: &[u8]) -> Result<(), SinkError> {
        let count = self
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or(SinkError::Failure(PrepareError::Limit))?;
        if self.expected.is_some_and(|expected| count > expected) {
            return Err(SinkError::Failure(PrepareError::Size));
        }
        self.writer
            .write_all(bytes)
            .map_err(|e| SinkError::Failure(e.into()))?;
        self.digest.update(bytes);
        self.bytes = count;
        Ok(())
    }
}
async fn receive(
    source: &Source,
    mut url: Url,
    original: &SnapshotContext,
    limits: Limits,
    expected: Option<u64>,
    writer: &mut impl Write,
) -> Result<[u8; 32], PrepareError> {
    if expected.is_some_and(|n| n > limits.http.response_body_bytes as u64) {
        return Err(PrepareError::Limit);
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::USER_AGENT,
        HeaderValue::from_static("graphit-release"),
    );
    for hop in 0..=limits.redirects {
        original.check()?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || (url.scheme() != "https" && !(source.allow_loopback_http && url.scheme() == "http"))
        {
            return Err(PrepareError::Redirect);
        }
        let origin = url.origin().ascii_serialization();
        let client = BoundedClient::new(
            ClientPolicy {
                origin: &origin,
                allow_loopback_http: source.allow_loopback_http,
                proxy: None,
                additional_roots: &[],
            },
            limits.http,
        )
        .map_err(PrepareError::Network)?;
        let mut sink = Download {
            writer: &mut *writer,
            url: &url,
            expected,
            bytes: 0,
            digest: Sha256::new(),
        };
        let mut check_original = || original.check();
        let result = client
            .execute_stream_guarded(
                NetworkRequest {
                    method: Method::GET,
                    url: url.as_str(),
                    headers: &headers,
                    body: &[],
                },
                NetworkRequestContext::new(original.required_deadline()?, original.cancellation()),
                &mut check_original,
                &mut sink,
            )
            .await;
        match result {
            Ok(_) => {
                if expected.is_some_and(|expected| sink.bytes != expected) {
                    return Err(PrepareError::Size);
                }
                original.check()?;
                return Ok(sink.digest.finalize().into());
            }
            Err(NetworkStreamError::Sink {
                source: SinkError::Redirect(next),
                ..
            }) if hop < limits.redirects => url = next,
            Err(NetworkStreamError::Sink {
                source: SinkError::Redirect(_),
                ..
            }) => return Err(PrepareError::Redirect),
            Err(NetworkStreamError::Sink {
                source: SinkError::Failure(error),
                ..
            }) => return Err(error),
            Err(NetworkStreamError::Network { source, .. }) => {
                return Err(PrepareError::Network(source));
            }
            Err(NetworkStreamError::Admission { source, .. }) => return Err(source.into()),
        }
    }
    Err(PrepareError::Redirect)
}

struct CheckedRead<'a, R> {
    reader: R,
    original: &'a SnapshotContext,
    left: u64,
    fault: &'a Cell<Option<io::ErrorKind>>,
}
impl<R: Read> Read for CheckedRead<'_, R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let result: io::Result<usize> = (|| {
            self.original.check()?;
            let maximum = usize::try_from(self.left.saturating_add(1))
                .unwrap_or(usize::MAX)
                .min(bytes.len());
            let count = self.reader.read(&mut bytes[..maximum])?;
            self.left = self
                .left
                .checked_sub(count as u64)
                .ok_or(io::ErrorKind::FileTooLarge)?;
            Ok(count)
        })();
        if let Err(error) = &result
            && self.fault.get().is_none()
        {
            self.fault.set(Some(error.kind()));
        }
        result
    }
}
fn normal(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.as_os_str().as_encoded_bytes().contains(&b'\\')
        && path.components().all(|p| matches!(p, Component::Normal(_)))
}
fn extract(
    archive: File,
    tree: &Path,
    target: Target,
    original: &SnapshotContext,
    limits: Limits,
) -> Result<Vec<String>, PrepareError> {
    let fault = Cell::new(None);
    let reader = CheckedRead {
        reader: MultiGzDecoder::new(archive),
        original,
        left: limits.decoded_archive_bytes,
        fault: &fault,
    };
    let mut archive = tar::Archive::new(reader);
    archive.set_ignore_zeros(true);
    let result = extract_entries(
        &mut archive,
        tree,
        target,
        original,
        limits.tree,
        limits.metadata_bytes,
    );
    drop(archive);
    if let Some(kind) = fault.get() {
        return Err(PrepareError::Io(kind));
    }
    result
}
fn extract_entries<R: Read>(
    archive: &mut tar::Archive<R>,
    tree: &Path,
    target: Target,
    original: &SnapshotContext,
    limits: CopyLimits,
    metadata_bytes: usize,
) -> Result<Vec<String>, PrepareError> {
    let root = PathBuf::from(target.root_name());
    let mut seen = BTreeMap::new();
    let mut nodes = std::collections::BTreeSet::new();
    let mut total_bytes = 0u64;
    let mut path_bytes = 0usize;
    for entry in archive.entries().map_err(|_| PrepareError::Archive)? {
        original.check()?;
        let mut entry = entry.map_err(|_| PrepareError::Archive)?;
        let relative = entry
            .path()
            .map_err(|_| PrepareError::Archive)?
            .into_owned();
        if !normal(&relative) || !relative.starts_with(&root) {
            return Err(PrepareError::Archive);
        }
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir() || kind.is_symlink() || kind.is_hard_link()) {
            return Err(PrepareError::Archive);
        }
        if relative.components().count() > limits.depth_limit()
            || seen.insert(relative.clone(), kind).is_some()
        {
            return Err(PrepareError::Archive);
        }
        for node in relative.ancestors().filter(|p| !p.as_os_str().is_empty()) {
            if nodes.insert(node.to_path_buf()) {
                path_bytes = path_bytes
                    .checked_add(node.as_os_str().as_encoded_bytes().len())
                    .ok_or(PrepareError::Limit)?;
                if path_bytes > limits.total_relative_path_bytes_limit()
                    || nodes.len() > limits.node_limit()
                {
                    return Err(PrepareError::Limit);
                }
            }
        }
        if kind.is_file() {
            total_bytes = total_bytes
                .checked_add(entry.size())
                .ok_or(PrepareError::Limit)?;
            if total_bytes > limits.total_file_bytes_limit() {
                return Err(PrepareError::Limit);
            }
        }
        // No extraction may write through a link created by an earlier entry.
        for parent in relative
            .ancestors()
            .skip(1)
            .filter(|p| !p.as_os_str().is_empty())
        {
            if let Ok(m) = fs::symlink_metadata(tree.join(parent))
                && (!m.is_dir() || m.file_type().is_symlink())
            {
                return Err(PrepareError::Archive);
            }
        }
        if kind.is_symlink() || kind.is_hard_link() {
            let link = entry
                .link_name()
                .map_err(|_| PrepareError::Archive)?
                .ok_or(PrepareError::Archive)?;
            if link.is_absolute()
                || link.as_os_str().as_encoded_bytes().contains(&b'\\')
                || link
                    .components()
                    .any(|c| matches!(c, Component::Prefix(_) | Component::RootDir))
            {
                return Err(PrepareError::Archive);
            }
            if kind.is_hard_link()
                && (!normal(&link)
                    || !link.starts_with(&root)
                    || !fs::symlink_metadata(tree.join(&link))
                        .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink()))
            {
                return Err(PrepareError::Archive);
            }
        }
        if !entry.unpack_in(tree).map_err(|_| PrepareError::Archive)? {
            return Err(PrepareError::Archive);
        }
    }
    let package = fs::canonicalize(tree.join(&root)).map_err(|_| PrepareError::Layout)?;
    let mut final_bytes = 0u64;
    for (relative, kind) in &seen {
        original.check()?;
        let path = tree.join(relative);
        let resolved = fs::canonicalize(&path).map_err(|_| PrepareError::Archive)?;
        if !resolved.starts_with(&package) {
            return Err(PrepareError::Archive);
        }
        let m = fs::metadata(&path)?;
        if m.is_file() {
            final_bytes = final_bytes
                .checked_add(m.len())
                .ok_or(PrepareError::Limit)?;
            if final_bytes > limits.total_file_bytes_limit() {
                return Err(PrepareError::Limit);
            }
        }
        if (kind.is_file() || kind.is_symlink() || kind.is_hard_link()) && !m.is_file() {
            return Err(PrepareError::Archive);
        }
    }
    let root_assets = validate_layout(&package, target, &seen, metadata_bytes, original)?;
    original.check()?;
    Ok(root_assets)
}
fn required_file(package: &Path, relative: &str) -> Result<(), PrepareError> {
    if fs::metadata(package.join(relative)).is_ok_and(|m| m.is_file() && m.len() > 0) {
        Ok(())
    } else {
        Err(PrepareError::Layout)
    }
}
fn digest_text(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
#[derive(serde::Deserialize)]
struct Build {
    platform: String,
    cargo_lock_sha256: String,
    native_sources: CargoNativeSources,
}
#[derive(serde::Deserialize)]
struct CargoNativeSources {
    lance: CargoNativeSource,
    lbug: CargoNativeSource,
    #[serde(rename = "llama-cpp-sys-2")]
    llama: CargoNativeSource,
}
#[derive(serde::Deserialize)]
struct CargoNativeSource {
    version: String,
    source: serde_json::Value,
}
fn validate_layout(
    package: &Path,
    target: Target,
    seen: &BTreeMap<PathBuf, tar::EntryType>,
    metadata_bytes: usize,
    original: &SnapshotContext,
) -> Result<Vec<String>, PrepareError> {
    for name in target.binaries() {
        required_file(package, name)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if target != Target::WindowsAmd64
                && fs::metadata(package.join(name))?.permissions().mode() & 0o111 == 0
            {
                return Err(PrepareError::Layout);
            }
        }
    }
    for name in [
        "graphit-runtime/LICENSE",
        "graphit-runtime/NATIVE-RUNTIME.md",
        "graphit-runtime/build.json",
        "graphit-runtime/agents/coordinator/AGENT.md",
        "graphit-runtime/agents/coordinator/prompt.md",
        "graphit-runtime/licenses/cargo/llama-cpp-sys-2/llama.cpp/LICENSE",
        "graphit-runtime/licenses/cargo/llama-cpp-sys-2/llama.cpp/licenses/LICENSE-jsonhpp",
        "graphit-runtime/licenses/cargo/llama-cpp-sys-2/LICENSE-MIT",
        "graphit-runtime/licenses/cargo/llama-cpp-sys-2/LICENSE-APACHE",
        "graphit-runtime/licenses/nvidia/LICENSE",
        "graphit-runtime/licenses/cargo/lance/LICENSE",
        "graphit-runtime/licenses/cargo/lbug/LICENSE",
        "graphit-runtime/licenses/crates/graphit-ast/src/antlr_engine/cpp/vendor/antlr4/LICENSE.txt",
        "graphit-runtime/licenses/crates/graphit-ast/src/antlr_engine/cpp/generated/GRAMMAR_NOTICES.md",
    ] {
        required_file(package, name)?;
    }
    for prefix in [
        "graphit-runtime/licenses/cargo/lbug/lbug-src/third_party",
        "graphit-runtime/licenses/crates/graphit-ast/grammars/tree_sitter",
    ] {
        let prefix = PathBuf::from(target.root_name()).join(prefix);
        if !seen
            .iter()
            .any(|(p, k)| p.starts_with(&prefix) && k.is_file())
        {
            return Err(PrepareError::Layout);
        }
    }
    let build: Build = runtime_metadata(package, "graphit-runtime/build.json", metadata_bytes)?;
    for (source, version) in [
        (&build.native_sources.lance, "14.0.2"),
        (&build.native_sources.lbug, "0.22.1"),
    ] {
        if source.version != version
            || source.source.as_str()
                != Some("registry+https://github.com/rust-lang/crates.io-index")
        {
            return Err(PrepareError::Layout);
        }
    }
    if build.native_sources.llama.version.trim().is_empty()
        || !build
            .native_sources
            .llama
            .source
            .as_str()
            .is_some_and(|source| !source.trim().is_empty())
    {
        return Err(PrepareError::Layout);
    }
    if build.platform != target.name() || !digest_text(&build.cargo_lock_sha256) {
        return Err(PrepareError::Layout);
    }
    validate_runtime_metadata(package, target, metadata_bytes, original)
}
fn runtime_metadata<T: serde::de::DeserializeOwned>(
    package: &Path,
    relative: &str,
    metadata_bytes: usize,
) -> Result<T, PrepareError> {
    required_file(package, relative)?;
    if fs::metadata(package.join(relative))?.len() > metadata_bytes as u64 {
        return Err(PrepareError::Limit);
    }
    serde_json::from_slice(&fs::read(package.join(relative))?).map_err(|_| PrepareError::Layout)
}
// Native build/link qualification belongs to the release producer. The updater
// verifies the complete TAR digest and only checks the installed package layout.
fn validate_runtime_metadata(
    package: &Path,
    target: Target,
    metadata_bytes: usize,
    original: &SnapshotContext,
) -> Result<Vec<String>, PrepareError> {
    let mut root_assets = Vec::new();
    if target == Target::LinuxAmd64 {
        let libraries: RuntimeLibraries = runtime_metadata(
            package,
            "graphit-runtime/linux-libraries.json",
            metadata_bytes,
        )?;
        if libraries.platform != target.name() {
            return Err(PrepareError::Layout);
        }
        for library in libraries.libraries {
            original.check()?;
            if !normal_runtime_name(&library.soname) || library.soname.contains('/') {
                return Err(PrepareError::Layout);
            }
            required_file(package, &format!("graphit-runtime/lib/{}", library.soname))?;
        }
    }
    if matches!(target, Target::DarwinArm64 | Target::WindowsAmd64) {
        root_assets.extend(validate_platform_runtime(
            package,
            target,
            metadata_bytes,
            original,
        )?);
    }
    original.check()?;
    Ok(root_assets)
}
// These manifests describe the loader layout already qualified by the native producer.
// The authenticated archive digest protects their bytes; do not rehash the whole tree.
fn validate_platform_runtime(
    package: &Path,
    target: Target,
    metadata_bytes: usize,
    original: &SnapshotContext,
) -> Result<Vec<String>, PrepareError> {
    let macos = target == Target::DarwinArm64;
    let relative = if macos {
        "graphit-runtime/macos-libraries.json"
    } else {
        "graphit-runtime/windows-libraries.json"
    };
    let manifest: PlatformRuntimeLibraries = runtime_metadata(package, relative, metadata_bytes)?;
    if manifest.platform != target.name() {
        return Err(PrepareError::Layout);
    }
    let mut versions = Vec::new();
    let mut executables = std::collections::BTreeSet::new();
    for executable in manifest.executables {
        original.check()?;
        if !target.binaries().contains(&executable.file.as_str())
            || !executables.insert(executable.file.clone())
        {
            return Err(PrepareError::Layout);
        }
        required_regular_runtime_file(package, &executable.file)?;
        if macos {
            versions.push(macos_version(
                executable
                    .minimum_macos
                    .as_deref()
                    .ok_or(PrepareError::Layout)?,
            )?);
        }
    }
    if executables.len() != target.binaries().len() {
        return Err(PrepareError::Layout);
    }
    let mut names = std::collections::BTreeSet::new();
    let mut root_assets = Vec::new();
    for library in manifest.libraries {
        original.check()?;
        if !normal_runtime_name(&library.name)
            || !library
                .name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
            || !names.insert(library.name.to_ascii_lowercase())
        {
            return Err(PrepareError::Layout);
        }
        if macos {
            if !library.name.ends_with(".dylib")
                || library.file != format!("graphit-runtime/lib/{}", library.name)
            {
                return Err(PrepareError::Layout);
            }
            versions.push(macos_version(
                library
                    .minimum_macos
                    .as_deref()
                    .ok_or(PrepareError::Layout)?,
            )?);
        } else {
            if !library.name.ends_with(".dll") || library.file != library.name {
                return Err(PrepareError::Layout);
            }
            root_assets.push(library.file.clone());
        }
        required_regular_runtime_file(package, &library.file)?;
    }
    if macos {
        let minimum = manifest
            .minimum_macos
            .as_deref()
            .ok_or(PrepareError::Layout)?;
        if versions.into_iter().max() != Some(macos_version(minimum)?) {
            return Err(PrepareError::Layout);
        }
        #[cfg(target_os = "macos")]
        {
            original.check()?;
            let output = std::process::Command::new("/usr/bin/sw_vers")
                .arg("-productVersion")
                .output()?;
            original.check()?;
            if !output.status.success() {
                return Err(PrepareError::Layout);
            }
            let current = std::str::from_utf8(&output.stdout)
                .map_err(|_| PrepareError::Layout)?
                .trim();
            validate_macos_host(minimum, current)?;
        }
    }
    Ok(root_assets)
}
fn required_regular_runtime_file(package: &Path, relative: &str) -> Result<(), PrepareError> {
    required_file(package, relative)?;
    if !fs::symlink_metadata(package.join(relative))?.is_file() {
        return Err(PrepareError::Layout);
    }
    Ok(())
}
fn macos_version(value: &str) -> Result<[u32; 3], PrepareError> {
    let parts: Vec<_> = value.split('.').collect();
    if !(2..=3).contains(&parts.len()) {
        return Err(PrepareError::Layout);
    }
    let mut version = [0; 3];
    for (index, part) in parts.into_iter().enumerate() {
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(PrepareError::Layout);
        }
        version[index] = part.parse().map_err(|_| PrepareError::Layout)?;
    }
    if version[0] == 0 {
        return Err(PrepareError::Layout);
    }
    Ok(version)
}
#[cfg(any(target_os = "macos", test))]
fn validate_macos_host(required: &str, current: &str) -> Result<(), PrepareError> {
    if macos_version(required)? > macos_version(current)? {
        return Err(PrepareError::UnsupportedMacOS {
            required: required.into(),
            current: current.into(),
        });
    }
    Ok(())
}
#[derive(serde::Deserialize)]
struct PlatformRuntimeLibraries {
    platform: String,
    minimum_macos: Option<String>,
    libraries: Vec<PlatformRuntimeLibrary>,
    executables: Vec<PlatformRuntimeExecutable>,
}
#[derive(serde::Deserialize)]
struct PlatformRuntimeLibrary {
    name: String,
    file: String,
    minimum_macos: Option<String>,
}
#[derive(serde::Deserialize)]
struct PlatformRuntimeExecutable {
    file: String,
    minimum_macos: Option<String>,
}
fn normal_runtime_name(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\\')
        && Path::new(value)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}
#[derive(serde::Deserialize)]
struct RuntimeLibraries {
    platform: String,
    libraries: Vec<RuntimeLibrary>,
}
#[derive(serde::Deserialize)]
struct RuntimeLibrary {
    soname: String,
}
#[cfg(test)]
pub(crate) mod tests;
