//! Pinned model provisioning with captured filesystem authority and original context.
use graphit_ai::{
    model_bindings::bind_captured_model,
    model_manifest::{ManifestLimits, ModelBindingError, ModelSelection, ModelTask},
};
use graphit_core::{
    artifact_store::{
        ArtifactError, ArtifactKey, ArtifactLimits, ArtifactMode, ArtifactStore,
        CapturedPublicationScope, CapturedSourceReader, CopyLimits, CopyPolicy, PhysicalSource,
        RetainedBytesLimits, RetainedFileSet, ScopeLimits, SnapshotContext, SnapshotPolicy,
        SourceReadLimits,
    },
    auth_document::AuthDocumentError,
    config::ConfigStore,
    installation_bootstrap::{self, BootstrapLimits, PrivateInstallation},
    installation_identity::InstallationIdentityStore,
    project_paths::InstallationPaths,
};
use graphit_http::client::{
    BoundedClient, ClientPolicy, Limits, NetworkRequest, NetworkRequestContext,
    NetworkResponseSink, NetworkStreamError,
};
use reqwest::{
    Method, StatusCode, Url,
    header::{HeaderMap, LOCATION},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

pub const MODEL_ID: &str = "embeddinggemma2";
pub const MODEL_REVISION: &str = "bfcd298762cc34d0357ece5ebdd31791a3a374d8";
pub const MODEL_FILE: &str = "embeddinggemma-2-BF16.gguf";
pub const MODEL_BYTES: u64 = 557950176;
pub const MODEL_SHA256: [u8; 32] = [
    104, 186, 226, 157, 98, 251, 140, 125, 35, 233, 141, 33, 253, 70, 98, 117, 61, 221, 99, 110,
    107, 98, 184, 167, 13, 254, 5, 154, 152, 68, 242, 22,
];
pub const MODEL_URL: &str = "https://huggingface.co/ggml-org/embeddinggemma-2-GGUF/resolve/bfcd298762cc34d0357ece5ebdd31791a3a374d8/embeddinggemma-2-BF16.gguf";

/// Official upstream weights in the pinned F16 GGUF; distribution revision is in its URL.
pub const RERANK_MODEL_ID: &str = "bge-reranker-v2-m3";
pub const RERANK_MODEL_REVISION: &str = "953dc6f6f85a1b2dbfca4c34a2796e7dde08d41e";
pub const RERANK_MODEL_FILE: &str = "bge-reranker-v2-m3-F16.gguf";
pub const RERANK_MODEL_BYTES: u64 = 1_159_774_912;
pub const RERANK_MODEL_SHA256: [u8; 32] = [
    60, 45, 228, 8, 210, 192, 168, 90, 148, 114, 220, 9, 249, 213, 162, 44, 155, 115, 116, 60, 99,
    67, 149, 44, 21, 5, 50, 153, 199, 119, 194, 152,
];

pub const RERANK_MODEL_URL: &str = "https://huggingface.co/sinjab/bge-reranker-v2-m3-F16-GGUF/resolve/1458e3304258a8a11773000c7d0c192abfe78291/bge-reranker-v2-m3-F16.gguf";

/// The two product models share installation, integrity and publication logic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PinnedModel {
    EmbeddingGemma2,
    BgeRerankerV2M3,
}
impl PinnedModel {
    pub fn id(self) -> &'static str {
        match self {
            Self::EmbeddingGemma2 => MODEL_ID,
            Self::BgeRerankerV2M3 => RERANK_MODEL_ID,
        }
    }
    pub fn revision(self) -> &'static str {
        match self {
            Self::EmbeddingGemma2 => MODEL_REVISION,
            Self::BgeRerankerV2M3 => RERANK_MODEL_REVISION,
        }
    }
    pub fn file(self) -> &'static str {
        match self {
            Self::EmbeddingGemma2 => MODEL_FILE,
            Self::BgeRerankerV2M3 => RERANK_MODEL_FILE,
        }
    }
    pub fn url(self) -> &'static str {
        match self {
            Self::EmbeddingGemma2 => MODEL_URL,
            Self::BgeRerankerV2M3 => RERANK_MODEL_URL,
        }
    }
    pub fn bytes(self) -> u64 {
        match self {
            Self::EmbeddingGemma2 => MODEL_BYTES,
            Self::BgeRerankerV2M3 => RERANK_MODEL_BYTES,
        }
    }
    pub fn digest(self) -> [u8; 32] {
        match self {
            Self::EmbeddingGemma2 => MODEL_SHA256,
            Self::BgeRerankerV2M3 => RERANK_MODEL_SHA256,
        }
    }
    pub fn task(self) -> ModelTask {
        match self {
            Self::EmbeddingGemma2 => ModelTask::Embedding,
            Self::BgeRerankerV2M3 => ModelTask::Rerank,
        }
    }
    fn manifests(self) -> io::Result<Vec<(String, Vec<u8>)>> {
        match self {
            Self::EmbeddingGemma2 => [128, 256, 512, 768]
                .into_iter()
                .map(|dimensions| {
                    Ok((
                        manifest_name(dimensions),
                        crate::models::pinned_manifest(dimensions)?,
                    ))
                })
                .collect(),
            Self::BgeRerankerV2M3 => Ok(vec![(
                "model.json".into(),
                crate::models::pinned_rerank_manifest()?,
            )]),
        }
    }
}

pub enum ProvisionError<E> {
    Io {
        kind: io::ErrorKind,
        native: Option<i32>,
    },
    Bootstrap(AuthDocumentError),
    Artifact(ArtifactError),
    Binding(ModelBindingError),
    Model(graphit_daemon::model_service::ModelServiceError),
    Admission(E),
    Network(NetworkStreamError<E, StageError>),
    Configuration,
    Integrity,
    Conflict,
    Redirect,
}
impl<E> fmt::Display for ProvisionError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { kind, native } => write!(f, "model IO {kind:?} native={native:?}"),
            Self::Bootstrap(e) => write!(f, "model installation {e}"),
            Self::Artifact(e) => write!(f, "model artifact {e}"),
            Self::Binding(e) => write!(f, "model {e}"),
            Self::Model(e) => write!(f, "model {e}"),
            Self::Admission(_) => f.write_str("model admission rejected"),
            Self::Network(e) => write!(f, "model {e}"),
            Self::Configuration => f.write_str("model configuration invalid"),
            Self::Integrity => f.write_str("pinned model integrity mismatch"),
            Self::Conflict => f.write_str("model bundle content conflict"),
            Self::Redirect => f.write_str("model redirect rejected"),
        }
    }
}
impl<E> fmt::Debug for ProvisionError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl<E: std::error::Error + 'static> std::error::Error for ProvisionError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bootstrap(e) => Some(e),
            Self::Artifact(e) => Some(e),
            Self::Binding(e) => Some(e),
            Self::Model(e) => Some(e),
            Self::Admission(e) => Some(e),
            Self::Network(e) => Some(e),
            _ => None,
        }
    }
}
impl<E> From<io::Error> for ProvisionError<E> {
    fn from(e: io::Error) -> Self {
        Self::Io {
            kind: e.kind(),
            native: e.raw_os_error(),
        }
    }
}
impl<E> From<AuthDocumentError> for ProvisionError<E> {
    fn from(e: AuthDocumentError) -> Self {
        Self::Bootstrap(e)
    }
}
impl<E> From<ArtifactError> for ProvisionError<E> {
    fn from(e: ArtifactError) -> Self {
        Self::Artifact(e)
    }
}
impl<E> From<ModelBindingError> for ProvisionError<E> {
    fn from(e: ModelBindingError) -> Self {
        Self::Binding(e)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum StageError {
    Io {
        kind: io::ErrorKind,
        native: Option<i32>,
    },
    Status(u16),
    Integrity,
    Redirect,
    RedirectDenied,
}
impl From<io::Error> for StageError {
    fn from(e: io::Error) -> Self {
        Self::Io {
            kind: e.kind(),
            native: e.raw_os_error(),
        }
    }
}
impl fmt::Display for StageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "model stage {self:?}")
    }
}
impl std::error::Error for StageError {}

/// Logical product ceilings, not RSS or preemption guarantees.
pub struct ProvisionLimits {
    pub bootstrap: BootstrapLimits,
    pub scope: ScopeLimits,
    pub artifact: ArtifactLimits,
    pub snapshot: SnapshotPolicy,
    pub read: SourceReadLimits,
    pub manifest: ManifestLimits,
    pub http: Limits,
}
impl ProvisionLimits {
    pub fn product() -> io::Result<Self> {
        let copy = CopyLimits::new(4 * 1024 * 1024 * 1024, 256, 64 * 1024, 16)?;
        let retained = RetainedBytesLimits::new(64 * 1024, 16, 64 * 1024)?;
        Ok(Self {
            bootstrap: BootstrapLimits::new(16 * 1024, 128)
                .map_err(|_| io::ErrorKind::InvalidInput)?,
            scope: ScopeLimits::new(4, 16 * 1024, 4096)?,
            artifact: ArtifactLimits::new(1024 * 1024, 1024, copy)?,
            snapshot: SnapshotPolicy::new(
                CopyPolicy::RejectFileLinks,
                copy,
                RetainedFileSet::new([], retained)?,
                retained,
            )?,
            read: SourceReadLimits::new(64 * 1024, 64 * 1024, copy.total_file_bytes_limit())?,
            manifest: ManifestLimits {
                max_manifest_bytes: 32 * 1024,
                max_artifacts: 64,
                max_role_bytes: 128,
                max_relative_path_bytes: 16 * 1024,
                max_id_bytes: 128,
                max_revision_bytes: 128,
                max_dimension: 65536,
                max_tokens: 65536,
            },
            http: Limits {
                request_body_bytes: 1024,
                response_body_bytes: usize::try_from(MODEL_BYTES.max(RERANK_MODEL_BYTES))
                    .map_err(|_| io::ErrorKind::InvalidInput)?,
                url_bytes: 16 * 1024,
                header_bytes: 64 * 1024,
                header_count: 128,
                max_in_flight: 1,
                max_idle_per_host: 1,
                idle_timeout: Duration::from_secs(30),
                max_certificates: 8,
                max_certificate_bytes: 64 * 1024,
            },
        })
    }
}

/// These captures convey native filesystem authority, never an Auth login/grant.
pub struct ModelProvisioner {
    paths: InstallationPaths,
    installation: PrivateInstallation,
    original: Arc<SnapshotContext>,
    limits: ProvisionLimits,
    artifacts: ArtifactStore,
    config: ConfigStore,
}
pub struct InstalledModel {
    pub bundle: PathBuf,
    pub manifest_relative: PathBuf,
    pub selected: bool,
}
pub enum InstallSource<'a> {
    Remote,
    Captured {
        reader: &'a CapturedSourceReader,
        relative: &'a Path,
    },
}

impl ModelProvisioner {
    pub fn capture(
        paths: &InstallationPaths,
        original: Arc<SnapshotContext>,
        limits: ProvisionLimits,
    ) -> Result<Self, ProvisionError<io::Error>> {
        original.required_deadline()?;
        let installation = installation_bootstrap::prepare(paths, limits.bootstrap, &original)
            .into_installation()?;
        original.check()?;
        let identity = InstallationIdentityStore::new(paths)?
            .ensure(remaining(&original)?, original.cancellation())?;
        installation.revalidate(&original)?;
        let models = paths.models_directory();
        private_directory(&models, &original)?;
        let scope = CapturedPublicationScope::installation_models(
            paths,
            identity.unit_id().clone(),
            [models],
            limits.scope,
            &original,
        )?;
        let artifacts = ArtifactStore::new(scope, limits.artifact);
        let config = ConfigStore::global(paths)?;
        original.check()?;
        Ok(Self {
            paths: paths.clone(),
            installation,
            original,
            limits,
            artifacts,
            config,
        })
    }
    pub fn original(&self) -> &Arc<SnapshotContext> {
        &self.original
    }
    pub fn config(&self) -> &ConfigStore {
        &self.config
    }
    pub fn limits(&self) -> &ProvisionLimits {
        &self.limits
    }
    pub fn paths(&self) -> &InstallationPaths {
        &self.paths
    }
    pub fn check<E>(
        &self,
        gate: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<(), ProvisionError<E>> {
        self.check_admission(gate)?;
        self.installation.revalidate(&self.original)?;
        self.original.check()?;
        Ok(())
    }
    fn check_admission<E>(
        &self,
        gate: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<(), ProvisionError<E>> {
        self.original.check()?;
        let checked = gate();
        self.original.check()?;
        checked.map_err(ProvisionError::Admission)?;
        Ok(())
    }
    pub fn reader<E>(
        &self,
        bundle: &Path,
        gate: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<CapturedSourceReader, ProvisionError<E>> {
        self.check(gate)?;
        let reader = CapturedSourceReader::capture(
            bundle,
            graphit_fs::NodeKind::Directory,
            &self.limits.snapshot,
            self.limits.read,
            self.original.clone(),
        )?;
        self.check(gate)?;
        Ok(reader)
    }
    pub async fn install<E>(
        &self,
        model: PinnedModel,
        source: InstallSource<'_>,
        gate: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<InstalledModel, ProvisionError<E>> {
        self.check(gate)?;
        let bundle =
            self.paths
                .models_directory()
                .join(format!("{}-{}", model.id(), model.revision()));
        match fs::symlink_metadata(&bundle) {
            Ok(metadata) => {
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return Err(ProvisionError::Conflict);
                }
                let existing = self.reader(&bundle, gate)?;
                let manifests = model.manifests()?;
                if existing.nodes().any(|node| {
                    let path = node.relative();
                    !path.as_os_str().is_empty()
                        && path != Path::new(model.file())
                        && !manifests.iter().any(|(name, _)| path == Path::new(name))
                }) {
                    return Err(ProvisionError::Conflict);
                }
                for (name, expected) in manifests {
                    self.check(gate)?;
                    let file = existing.open_file(Path::new(&name))?;
                    if file.len() != expected.len() as u64 {
                        return Err(ProvisionError::Conflict);
                    }
                    let mut actual = vec![0; expected.len()];
                    file.stream(0)?.read_exact(&mut actual)?;
                    if actual != expected {
                        return Err(ProvisionError::Conflict);
                    }
                }
                self.bind_pin(model, &existing, Path::new("model.json"), gate)?;
                let manifest_relative = PathBuf::from("model.json");
                let selected = crate::models::select_bundle(
                    self,
                    &bundle,
                    &manifest_relative,
                    false,
                    None,
                    gate,
                )?;
                return Ok(InstalledModel {
                    bundle,
                    manifest_relative,
                    selected,
                });
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let stage = tempfile::Builder::new()
            .prefix(".model-stage-")
            .tempdir_in(self.paths.models_directory())?;
        self.check(gate)?;
        let mut output = graphit_fs::create_private_file(&stage.path().join(model.file()))?;
        match source {
            InstallSource::Remote => self.fetch(model, &mut output, gate).await?,
            InstallSource::Captured { reader, relative } => {
                reader.check_request(&self.original)?;
                let file = reader.open_file(relative)?;
                if file.len() != model.bytes() {
                    return Err(ProvisionError::Integrity);
                }
                let mut stream = file.stream(0)?;
                let mut digest = Sha256::new();
                let mut bytes = 0u64;
                let mut block = [0u8; 64 * 1024];
                loop {
                    // The output is our retained private descriptor. Recheck the
                    // installation at phase boundaries, not for every IO block;
                    // SourceRangeStream validates its captured source binding.
                    self.check_admission(gate)?;
                    let read = stream.read(&mut block)?;
                    self.check_admission(gate)?;
                    if read == 0 {
                        break;
                    }
                    bytes = bytes
                        .checked_add(read as u64)
                        .ok_or(ProvisionError::Integrity)?;
                    if bytes > model.bytes() {
                        return Err(ProvisionError::Integrity);
                    }
                    output.write_all(&block[..read])?;
                    digest.update(&block[..read]);
                    self.check_admission(gate)?;
                }
                if bytes != model.bytes() || <[u8; 32]>::from(digest.finalize()) != model.digest() {
                    return Err(ProvisionError::Integrity);
                }
                // The exact bytes just copied were hashed above. Reject changed
                // paths/observations without hashing the same source again.
                reader.revalidate_bindings()?;
            }
        }
        self.check(gate)?;
        output.sync_all()?;
        drop(output);
        for (name, body) in model.manifests()? {
            self.check(gate)?;
            if body.len() > self.limits.manifest.max_manifest_bytes {
                return Err(ProvisionError::Configuration);
            }
            let mut manifest = graphit_fs::create_private_file(&stage.path().join(name))?;
            manifest.write_all(&body)?;
            manifest.sync_all()?;
            self.check(gate)?;
        }
        let staged = self.reader(stage.path(), gate)?;
        self.bind_pin(model, &staged, Path::new("model.json"), gate)?;
        let mut writer = self.artifacts.writer_with_context(&self.original)?;
        self.check(gate)?;
        match fs::symlink_metadata(&bundle) {
            Ok(metadata) => {
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return Err(ProvisionError::Conflict);
                }
                let existing = self.reader(&bundle, gate)?;
                let signature = |reader: &CapturedSourceReader| {
                    reader
                        .nodes()
                        .map(|node| (node.relative().to_path_buf(), (node.digest(), node.bytes())))
                        .collect::<BTreeMap<_, _>>()
                };
                if signature(&staged) != signature(&existing) {
                    return Err(ProvisionError::Conflict);
                }
                existing.revalidate_bindings()?;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        self.check(gate)?;
        // Capture and bind_pin already verified this private stage's digest;
        // the publisher also hashes and verifies its own physical copy.
        staged.revalidate_bindings()?;
        writer.publish(
            &ArtifactKey::new("models", model.id(), model.revision())?,
            PhysicalSource::ExternalSource(stage.path()),
            &bundle,
            graphit_fs::NodeKind::Directory,
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        )?;
        drop(writer);
        self.check(gate)?;
        let manifest_relative = PathBuf::from("model.json");
        let selected =
            crate::models::select_bundle(self, &bundle, &manifest_relative, false, None, gate)?;
        self.check(gate)?;
        Ok(InstalledModel {
            bundle,
            manifest_relative,
            selected,
        })
    }
    fn bind_pin<E>(
        &self,
        model: PinnedModel,
        reader: &CapturedSourceReader,
        manifest: &Path,
        gate: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<(), ProvisionError<E>> {
        self.check(gate)?;
        bind_captured_model(
            reader,
            &self.original,
            manifest,
            &ModelSelection {
                id: model.id(),
                revision: model.revision(),
                task: model.task(),
            },
            &self.limits.manifest,
        )?;
        self.check(gate)
    }
    async fn fetch<E>(
        &self,
        model: PinnedModel,
        file: &mut File,
        gate: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<(), ProvisionError<E>> {
        let mut url = Url::parse(model.url()).map_err(|_| ProvisionError::Redirect)?;
        let mut clients = BTreeMap::new();
        for hop in 0..=2 {
            self.check(gate)?;
            let origin = remote_origin(&url).ok_or(ProvisionError::Redirect)?;
            if let std::collections::btree_map::Entry::Vacant(entry) = clients.entry(origin) {
                let client = BoundedClient::new(
                    ClientPolicy {
                        origin,
                        allow_loopback_http: false,
                        proxy: None,
                        additional_roots: &[],
                    },
                    self.limits.http,
                )
                .map_err(|source| {
                    ProvisionError::Network(NetworkStreamError::Network {
                        source,
                        effect: graphit_http::client::NetworkEffect::NotSent,
                        accepted_bytes: 0,
                    })
                })?;
                entry.insert(client);
            }
            let mut admitting = || self.check_admission(gate);
            let result = receive_pinned(
                model,
                &clients[origin],
                url.as_str(),
                file,
                &self.original,
                &mut admitting,
            )
            .await
            .map_err(flatten)?;
            self.check(gate)?;
            match result {
                DownloadHop::Complete => return Ok(()),
                DownloadHop::Redirect(next) if hop < 2 => url = next,
                DownloadHop::Redirect(_) => return Err(ProvisionError::Redirect),
            }
        }
        Err(ProvisionError::Redirect)
    }
    /// Select an installed, captured manifest; infer runtime only in this App producer.
    pub fn select<E>(
        &self,
        manifest_relative: &Path,
        runtime: Option<graphit_daemon::model_service::RuntimeSettings>,
        gate: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<bool, ProvisionError<E>> {
        self.check(gate)?;
        if manifest_relative.as_os_str().is_empty()
            || manifest_relative.as_os_str().len() > self.limits.manifest.max_relative_path_bytes
            || manifest_relative
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(ProvisionError::Configuration);
        }
        let manifest = self.paths.models_directory().join(manifest_relative);
        let parent = manifest.parent().ok_or(ProvisionError::Configuration)?;
        let bundle = fs::canonicalize(parent)?;
        let models = fs::canonicalize(self.paths.models_directory())?;
        if !bundle.starts_with(&models) {
            return Err(ProvisionError::Configuration);
        }
        let relative = PathBuf::from(manifest.file_name().ok_or(ProvisionError::Configuration)?);
        crate::models::select_bundle(self, &bundle, &relative, true, runtime, gate)
    }
}

pub fn manifest_name(dimensions: u64) -> String {
    if dimensions == 768 {
        "model.json".into()
    } else {
        format!("model-{dimensions}.json")
    }
}
pub fn remaining(original: &SnapshotContext) -> io::Result<Duration> {
    Ok(original
        .required_deadline()?
        .saturating_duration_since(Instant::now()))
}
fn private_directory(path: &Path, original: &SnapshotContext) -> io::Result<()> {
    original.check()?;
    match graphit_fs::create_private_directory(path) {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            let metadata = fs::symlink_metadata(path)?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        }
        Err(e) => return Err(e),
    }
    original.check()
}
fn remote_origin(url: &Url) -> Option<&'static str> {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.port().is_some_and(|port| port != 443)
    {
        return None;
    }
    match url.host_str()? {
        "huggingface.co" => Some("https://huggingface.co"),
        "us.aws.cdn.hf.co" => Some("https://us.aws.cdn.hf.co"),
        _ => None,
    }
}
/// No diagnostic representation: a redirect can contain transient signed query data.
pub enum DownloadHop {
    Complete,
    Redirect(Url),
}
struct FileSink<'a> {
    model: PinnedModel,
    file: &'a mut File,
    original: &'a SnapshotContext,
    url: Url,
    redirect: Option<Url>,
    bytes: u64,
    digest: Sha256,
}
impl NetworkResponseSink for FileSink<'_> {
    type Error = StageError;
    fn head(&mut self, status: StatusCode, headers: &HeaderMap) -> Result<(), StageError> {
        self.original.check()?;
        if matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308) {
            let mut values = headers.get_all(LOCATION).iter();
            let value = values
                .next()
                .ok_or(StageError::RedirectDenied)?
                .to_str()
                .map_err(|_| StageError::RedirectDenied)?;
            if values.next().is_some() {
                return Err(StageError::RedirectDenied);
            }
            let next = self
                .url
                .join(value)
                .map_err(|_| StageError::RedirectDenied)?;
            if remote_origin(&next).is_none() {
                return Err(StageError::RedirectDenied);
            }
            self.redirect = Some(next);
            return Err(StageError::Redirect);
        }
        if status != StatusCode::OK {
            return Err(StageError::Status(status.as_u16()));
        }
        if let Some(length) = headers.get(reqwest::header::CONTENT_LENGTH)
            && length.to_str().ok().and_then(|v| v.parse::<u64>().ok()) != Some(self.model.bytes())
        {
            return Err(StageError::Integrity);
        }
        self.original.check().map_err(StageError::from)
    }
    fn chunk(&mut self, bytes: &[u8]) -> Result<(), StageError> {
        self.original.check()?;
        let next = self
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or(StageError::Integrity)?;
        if next > self.model.bytes() {
            return Err(StageError::Integrity);
        }
        self.file.write_all(bytes)?;
        self.digest.update(bytes);
        self.bytes = next;
        self.original.check().map_err(StageError::from)
    }
}
/// Receive into caller-owned private staging; integrity is always the actual fixed pin.
/// The supplied client owns origin admission. Redirects remain the fixed HTTPS allowlist.
pub async fn receive_pinned<E>(
    model: PinnedModel,
    client: &BoundedClient,
    url: &str,
    file: &mut File,
    original: &SnapshotContext,
    gate: &mut impl FnMut() -> Result<(), E>,
) -> Result<DownloadHop, ProvisionError<E>> {
    original.check()?;
    let mut sink = FileSink {
        model,
        file,
        original,
        url: Url::parse(url).map_err(|_| ProvisionError::Redirect)?,
        redirect: None,
        bytes: 0,
        digest: Sha256::new(),
    };
    let headers = HeaderMap::new();
    let mut denied = None;
    let result = {
        let mut admitting = || match gate() {
            Ok(()) => Ok(()),
            Err(error) => {
                if denied.is_none() {
                    denied = Some(error);
                }
                Err(())
            }
        };
        client
            .execute_stream_guarded(
                NetworkRequest {
                    method: Method::GET,
                    url,
                    headers: &headers,
                    body: &[],
                },
                NetworkRequestContext::new(original.required_deadline()?, original.cancellation()),
                &mut admitting,
                &mut sink,
            )
            .await
    };
    if let Err(error) = original.check() {
        let source = match error.kind() {
            io::ErrorKind::Interrupted => graphit_http::client::NetworkError::Cancelled,
            io::ErrorKind::TimedOut => graphit_http::client::NetworkError::Deadline,
            _ => return Err(error.into()),
        };
        let (effect, accepted_bytes) = match &result {
            Ok(response) => (
                graphit_http::client::NetworkEffect::MayHaveApplied,
                response.accepted_bytes(),
            ),
            Err(error) => (error.effect(), error.accepted_bytes()),
        };
        return Err(ProvisionError::Network(NetworkStreamError::Network {
            source,
            effect,
            accepted_bytes,
        }));
    }
    match result {
        Ok(_) => {
            if sink.bytes != model.bytes()
                || <[u8; 32]>::from(sink.digest.finalize()) != model.digest()
            {
                return Err(ProvisionError::Integrity);
            }
            Ok(DownloadHop::Complete)
        }
        Err(NetworkStreamError::Sink {
            source: StageError::Redirect,
            effect,
            accepted_bytes,
        }) => {
            if let Some(source) = denied {
                return Err(ProvisionError::Network(NetworkStreamError::Admission {
                    source,
                    effect,
                    accepted_bytes,
                }));
            }
            Ok(DownloadHop::Redirect(
                sink.redirect.ok_or(ProvisionError::Redirect)?,
            ))
        }
        Err(NetworkStreamError::Admission {
            effect,
            accepted_bytes,
            ..
        }) => {
            let source = denied.ok_or(ProvisionError::Configuration)?;
            Err(ProvisionError::Network(NetworkStreamError::Admission {
                source,
                effect,
                accepted_bytes,
            }))
        }
        Err(NetworkStreamError::Network {
            source,
            effect,
            accepted_bytes,
        }) => Err(ProvisionError::Network(NetworkStreamError::Network {
            source,
            effect,
            accepted_bytes,
        })),
        Err(NetworkStreamError::Sink {
            source,
            effect,
            accepted_bytes,
        }) => Err(ProvisionError::Network(NetworkStreamError::Sink {
            source,
            effect,
            accepted_bytes,
        })),
    }
}

fn flatten<E>(error: ProvisionError<ProvisionError<E>>) -> ProvisionError<E> {
    match error {
        ProvisionError::Io { kind, native } => ProvisionError::Io { kind, native },
        ProvisionError::Bootstrap(e) => ProvisionError::Bootstrap(e),
        ProvisionError::Artifact(e) => ProvisionError::Artifact(e),
        ProvisionError::Binding(e) => ProvisionError::Binding(e),
        ProvisionError::Model(e) => ProvisionError::Model(e),
        ProvisionError::Admission(e) => e,
        ProvisionError::Configuration => ProvisionError::Configuration,
        ProvisionError::Integrity => ProvisionError::Integrity,
        ProvisionError::Conflict => ProvisionError::Conflict,
        ProvisionError::Redirect => ProvisionError::Redirect,
        ProvisionError::Network(NetworkStreamError::Network {
            source,
            effect,
            accepted_bytes,
        }) => ProvisionError::Network(NetworkStreamError::Network {
            source,
            effect,
            accepted_bytes,
        }),
        ProvisionError::Network(NetworkStreamError::Sink {
            source,
            effect,
            accepted_bytes,
        }) => ProvisionError::Network(NetworkStreamError::Sink {
            source,
            effect,
            accepted_bytes,
        }),
        ProvisionError::Network(NetworkStreamError::Admission {
            source: ProvisionError::Admission(source),
            effect,
            accepted_bytes,
        }) => ProvisionError::Network(NetworkStreamError::Admission {
            source,
            effect,
            accepted_bytes,
        }),
        ProvisionError::Network(NetworkStreamError::Admission { source, .. }) => source,
    }
}

#[cfg(all(test, target_os = "linux"))]
mod namespace_tests;
