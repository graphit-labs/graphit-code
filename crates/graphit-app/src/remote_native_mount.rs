//! Request-scoped binding of installed remote metadata to the maintained native SDKs.
use futures_util::TryStreamExt;
use graphit_auth::{
    AuthError, ImportedAstReadAdmission, ImportedKnowledgeReadAdmission, ScopeCapture,
    ServiceBindingView, ServiceCredentialLease, ServicePurpose, ServiceUseError,
};
use graphit_core::{artifact_store::StoreOwner, file_io::SnapshotContext};
use graphit_data::lance_module_store::{LanceModuleStore, ModuleSnapshot, StoreError};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceOrigin,
    native_root::{NativeRole, NativeRoot, NativeSnapshot},
    policy::RequestContext,
    registry::{HubRegistry, HubScope, Installation, InstalledBackend, InstalledPayload},
    remote_metadata::{MetadataError, MetadataLimits},
    remote_native::RemoteNativeRecord,
    remote_storage::release_prefix,
};
use lance::session::{CacheSpec, Session};
use lance_io::object_store::{
    DEFAULT_CLOUD_IO_PARALLELISM, ObjectStore as LanceObjectStore, ObjectStoreParams,
    StorageOptions,
    providers::{ObjectStoreProvider, ObjectStoreRegistry},
};
use object_store::{ObjectStore, ObjectStoreExt, path::Path};
use sha2::{Digest, Sha256};
use std::{error::Error, fmt, future::Future, io, sync::Arc};
use url::Url;

/// Borrows the existing module admission; it neither creates nor restores authority.
#[derive(Clone, Copy)]
pub(crate) enum ReadAdmission<'a> {
    Ast(&'a ImportedAstReadAdmission<'a>),
    Knowledge(&'a ImportedKnowledgeReadAdmission<'a>),
}
impl<'a> ReadAdmission<'a> {
    pub(crate) fn request(self) -> &'a RequestContext<'a> {
        match self {
            Self::Ast(read) => read.request(),
            Self::Knowledge(read) => read.request(),
        }
    }
    fn installation(self) -> ScopeCapture<'a> {
        match self {
            Self::Ast(read) => read.installation(),
            Self::Knowledge(read) => read.installation(),
        }
    }
    fn kind(self) -> ArtifactType {
        match self {
            Self::Ast(_) => ArtifactType::Ast,
            Self::Knowledge(_) => ArtifactType::Knowledge,
        }
    }
    fn recheck(self) -> Result<(), ErrorKind> {
        self.request().snapshot().check()?;
        match self {
            Self::Ast(read) => read.recheck(),
            Self::Knowledge(read) => read.recheck(),
        }
        .map_err(ErrorKind::Auth)
    }
}

#[derive(Debug)]
pub(crate) enum ErrorKind {
    Auth(AuthError),
    Service(ServiceUseError),
    Operation(io::Error),
    Metadata(MetadataError),
    Storage(StoreError),
    Object(object_store::Error),
    InvalidInstallation,
    InvalidBinding,
    SelectionChanged,
    RootDigest,
    ManifestDigest,
    UnsupportedComponent,
    Credentials(crate::native_storage_credentials::StorageCredentialsError),
    Indexed(graphit_ast::indexed_source::IndexedSourceError),
    Inspection(graphit_ast::source_inspection::SourceInspectionError),
    Knowledge(graphit_data::knowledge_index::KnowledgeIndexError),
    Search(graphit_data::knowledge_search::KnowledgeSearchError),
    Graph(graphit_data::remote_graph::RemoteGraphError),
    Entity(graphit_ast::imported_graph::EntitySourceError),
    Embedding(Box<graphit_data::knowledge_embeddings::KnowledgeEmbeddingError>),
    Semantic(Box<crate::knowledge_semantic::KnowledgeSemanticError>),
}
impl ErrorKind {
    pub(crate) fn status(&self) -> axum::http::StatusCode {
        use crate::native_storage_credentials::StorageCredentialsError as C;
        use axum::http::StatusCode as S;
        match self {
            Self::Auth(AuthError::Request) => S::REQUEST_TIMEOUT,
            Self::Auth(AuthError::State | AuthError::Io) => S::INTERNAL_SERVER_ERROR,
            Self::Auth(_) | Self::Service(_) => S::FORBIDDEN,
            Self::Credentials(C::Authorization(_) | C::Expired) => S::FORBIDDEN,
            Self::Credentials(C::Http(status)) if matches!(status.as_u16(), 401 | 403) => {
                S::FORBIDDEN
            }
            Self::Operation(error) | Self::Credentials(C::Request(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::TimedOut
                ) =>
            {
                S::REQUEST_TIMEOUT
            }
            Self::Credentials(C::Transport(error)) if error.is_timeout() => S::REQUEST_TIMEOUT,
            _ => S::CONFLICT,
        }
    }
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Auth(AuthError::Request) => "request_cancelled_or_expired",
            Self::Auth(AuthError::State | AuthError::Io) => "authentication_store_failure",
            Self::Auth(_) | Self::Service(_) => "authentication_or_permission_denied",
            Self::Operation(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::TimedOut
                ) =>
            {
                "request_cancelled_or_expired"
            }
            Self::Credentials(
                crate::native_storage_credentials::StorageCredentialsError::Expired,
            ) => "storage_credentials_expired",
            Self::InvalidBinding | Self::SelectionChanged => "remote_native_binding_changed",
            Self::RootDigest | Self::ManifestDigest => "remote_native_digest_mismatch",
            Self::UnsupportedComponent => "remote_native_component_unavailable",
            _ => "remote_native_read_failed",
        }
    }
}
impl fmt::Display for ErrorKind {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Public handlers can retain the causal type without disclosing storage URLs.
        out.write_str(match self {
            Self::Auth(_) => "remote native admission denied",
            Self::Service(_) => "remote native service unavailable",
            Self::Operation(_) => "remote native operation failed",
            Self::Metadata(_) => "remote native metadata invalid",
            Self::Storage(_) => "remote native dataset unavailable or invalid",
            Self::Object(_) => "remote native object unavailable",
            Self::InvalidInstallation => "remote native installation invalid",
            Self::InvalidBinding => "remote native provider binding changed",
            Self::SelectionChanged => "remote native selection changed",
            Self::RootDigest => "remote native root digest mismatch",
            Self::ManifestDigest => "remote native manifest digest mismatch",
            Self::UnsupportedComponent => "remote native component unsupported",
            Self::Credentials(_) => "remote native credentials unavailable",
            Self::Indexed(_) => "remote native source invalid",
            Self::Inspection(_) => "remote native source selection failed",
            Self::Knowledge(_) => "remote native Knowledge invalid",
            Self::Search(_) => "remote native Knowledge query failed",
            Self::Graph(_) => "remote native graph invalid",
            Self::Entity(_) => "remote native entity unavailable or invalid",
            Self::Embedding(_) => "remote native semantic snapshot invalid",
            Self::Semantic(_) => "remote native semantic query failed",
        })
    }
}
impl Error for ErrorKind {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Auth(cause) => Some(cause),
            Self::Service(cause) => Some(cause),
            Self::Operation(cause) => Some(cause),
            Self::Metadata(cause) => Some(cause),
            Self::Storage(cause) => Some(cause),
            Self::Object(cause) => Some(cause),
            Self::Credentials(cause) => Some(cause),
            Self::Indexed(cause) => Some(cause),
            Self::Inspection(cause) => Some(cause),
            Self::Knowledge(cause) => Some(cause),
            Self::Search(cause) => Some(cause),
            Self::Graph(cause) => Some(cause),
            Self::Entity(cause) => Some(cause),
            Self::Embedding(cause) => Some(cause.as_ref()),
            Self::Semantic(cause) => Some(cause.as_ref()),
            _ => None,
        }
    }
}
impl From<io::Error> for ErrorKind {
    fn from(cause: io::Error) -> Self {
        Self::Operation(cause)
    }
}
impl From<ServiceUseError> for ErrorKind {
    fn from(cause: ServiceUseError) -> Self {
        Self::Service(cause)
    }
}

/// The retained registry tuple is data. Each operation still needs its admission/lease.
pub(crate) struct Selection<'a> {
    read: ReadAdmission<'a>,
    registry: &'a HubRegistry,
    installed: Installation,
}
impl<'a> Selection<'a> {
    /// `None` leaves the separately supported local Native backend to its own reader.
    pub(crate) fn capture(
        read: ReadAdmission<'a>,
        registry: &'a HubRegistry,
        id: &ArtifactId,
        version: &ConcreteVersion,
    ) -> Result<Option<Self>, ErrorKind> {
        read.recheck()?;
        validate_registry(read, registry)?;
        let document = registry.load()?;
        read.recheck()?;
        let Some(artifact) = document.get_version(read.kind(), id.as_str(), version) else {
            return Ok(None);
        };
        let mut entries = artifact.installations.iter().filter(|entry| {
            entry.target == InstallationTarget::GlobalNative
                && matches!(entry.backend, InstalledBackend::RemoteNative(_))
        });
        let Some(installed) = entries.next() else {
            return Ok(None);
        };
        if entries.next().is_some()
            || installed.alias.is_some()
            || installed.entry_name.is_some()
            || !matches!(
                installed.payload,
                InstalledPayload::Original {
                    selection_binding: None
                }
            )
            || installed.original_source.origin() != SourceOrigin::HubCache
            || installed.original_source.root().as_path() != read.installation().live.owner_root()
            || installed.original_source.physical_id() != id
        {
            return Err(ErrorKind::InvalidInstallation);
        }
        let selection = Self {
            read,
            registry,
            installed: installed.clone(),
        };
        selection.recheck()?;
        Ok(Some(selection))
    }
    pub(crate) fn record(&self) -> &RemoteNativeRecord {
        match &self.installed.backend {
            InstalledBackend::RemoteNative(record) => record,
            _ => unreachable!("captured remote native backend"),
        }
    }
    pub(crate) fn source(&self) -> &graphit_hub::SourceRef {
        &self.installed.original_source
    }
    pub(crate) fn request(&self) -> &'a RequestContext<'a> {
        self.read.request()
    }
    pub(crate) fn recheck(&self) -> Result<(), ErrorKind> {
        self.read.recheck()?;
        validate_registry(self.read, self.registry)?;
        let document = self.registry.load()?;
        self.read.recheck()?;
        let source = self.source();
        let current = document
            .get_version(
                source.artifact_type(),
                source.id().as_str(),
                source.version(),
            )
            .is_some_and(|artifact| {
                artifact
                    .installations
                    .iter()
                    .any(|v| v.same_source(&self.installed))
            });
        if !current {
            return Err(ErrorKind::SelectionChanged);
        }
        Ok(())
    }
    pub(crate) fn check(&self, lease: &ServiceCredentialLease<'_>) -> Result<(), ErrorKind> {
        self.recheck()?;
        lease.check()?;
        if !Arc::ptr_eq(self.request().snapshot(), lease.original()?.snapshot()) {
            return Err(ErrorKind::InvalidBinding);
        }
        match lease.binding()? {
            ServiceBindingView::Broker {
                provider,
                purpose: ServicePurpose::HubRead,
                ..
            } if provider == self.record().provider_name() => Ok(()),
            _ => Err(ErrorKind::InvalidBinding),
        }
    }
}

fn validate_registry(read: ReadAdmission<'_>, registry: &HubRegistry) -> Result<(), ErrorKind> {
    let scope = read.installation();
    let StoreOwner::Installation { installation_id } = scope.live.owner() else {
        return Err(ErrorKind::InvalidInstallation);
    };
    if !matches!(registry.scope(), HubScope::Global { installation_id: id } if id == installation_id.as_str())
        || registry.path().parent() != Some(scope.live.owner_root())
    {
        return Err(ErrorKind::InvalidInstallation);
    }
    Ok(())
}

/// Bind Lance to the already admitted store and bucket; no ambient provider chain.
pub(crate) fn admitted_session(bucket: &str, store: &Arc<dyn ObjectStore>) -> Arc<Session> {
    let providers = Arc::new(ObjectStoreRegistry::empty());
    providers.insert(
        "s3",
        Arc::new(AdmittedStore {
            bucket: bucket.to_owned(),
            store: Arc::clone(store),
        }),
    );
    Arc::new(Session::with_cache_backends(
        CacheSpec::Default,
        CacheSpec::Default,
        providers,
    ))
}

/// Immutable data handles only. The caller retains and checks the current lease.
pub(crate) struct Mount {
    pub(crate) root: NativeRoot,
    store: Arc<dyn ObjectStore>,
    prefix: Path,
    bucket: String,
    session: Arc<Session>,
}
impl Mount {
    pub(crate) async fn open(
        selected: &Selection<'_>,
        lease: &ServiceCredentialLease<'_>,
        store: Arc<dyn ObjectStore>,
        limits: MetadataLimits,
    ) -> Result<Self, ErrorKind> {
        selected.check(lease)?;
        let source = selected.source();
        let record = selected.record();
        let release = release_prefix(
            source.producer(),
            source.artifact_type(),
            source.id(),
            source.version(),
            record.root_digest(),
        )?;
        let prefix = if record.prefix().is_empty() {
            release
        } else {
            format!("{}/{release}", record.prefix())
        };
        let prefix = Path::parse(prefix).map_err(|_| ErrorKind::InvalidBinding)?;
        let original = selected.request().snapshot();
        let bytes = read_small(
            &store,
            &prefix.clone().join("native.json"),
            limits.bytes,
            original,
        )
        .await?;
        selected.check(lease)?;
        if <[u8; 32]>::from(Sha256::digest(&bytes)) != *record.root_digest() {
            return Err(ErrorKind::RootDigest);
        }
        let root =
            NativeRoot::decode(&bytes, limits, selected.request()).map_err(ErrorKind::Metadata)?;
        if root.producer() != source.producer()
            || root.artifact_type() != source.artifact_type()
            || root.id() != source.id()
            || root.version() != source.version()
        {
            return Err(ErrorKind::InvalidInstallation);
        }
        selected.check(lease)?;
        let session = admitted_session(record.bucket(), &store);
        Ok(Self {
            root,
            store,
            prefix,
            bucket: record.bucket().to_owned(),
            session,
        })
    }
    pub(crate) async fn source(
        &self,
        selected: &Selection<'_>,
        lease: &ServiceCredentialLease<'_>,
    ) -> Result<Option<ModuleSnapshot>, ErrorKind> {
        self.open_lance(selected, lease, NativeRole::Source).await
    }
    pub(crate) async fn wiki(
        &self,
        selected: &Selection<'_>,
        lease: &ServiceCredentialLease<'_>,
    ) -> Result<ModuleSnapshot, ErrorKind> {
        self.open_lance(selected, lease, NativeRole::Wiki)
            .await?
            .ok_or(ErrorKind::UnsupportedComponent)
    }
    pub(crate) async fn entity_search(
        &self,
        selected: &Selection<'_>,
        lease: &ServiceCredentialLease<'_>,
    ) -> Result<Option<graphit_data::lance_branch_store::BranchSnapshot>, ErrorKind> {
        selected.check(lease)?;
        let Some(component) = self
            .root
            .components()
            .iter()
            .find(|component| component.role == NativeRole::EntitySearch)
        else {
            return Ok(None);
        };
        let NativeSnapshot::Lance {
            dataset_version,
            manifest_path,
            manifest_sha256,
        } = &component.snapshot
        else {
            return Err(ErrorKind::UnsupportedComponent);
        };
        let path = self.prefix.clone().join(component.path.as_str());
        let mut uri =
            Url::parse(&format!("s3://{}/", self.bucket)).map_err(|_| ErrorKind::InvalidBinding)?;
        uri.path_segments_mut()
            .map_err(|_| ErrorKind::InvalidBinding)?
            .clear()
            .extend(path.as_ref().split('/'));
        let (snapshot, location) = wait(selected.request().snapshot(), async {
            graphit_data::lance_branch_store::BranchSnapshot::capture_native(
                uri.as_str(),
                Arc::clone(&self.session),
                *dataset_version,
            )
            .await
            .map_err(|error| ErrorKind::Embedding(Box::new(error.into())))
        })
        .await?;
        selected.check(lease)?;
        if location.version != *dataset_version
            || location.path
                != Path::parse(format!("{path}/{}", manifest_path.as_str()))
                    .map_err(|_| ErrorKind::InvalidBinding)?
        {
            return Err(ErrorKind::ManifestDigest);
        }
        let digest =
            hash_object(&self.store, &location.path, selected.request().snapshot()).await?;
        selected.check(lease)?;
        if digest != *manifest_sha256 {
            return Err(ErrorKind::ManifestDigest);
        }
        graphit_ast::corpus::validate_schema(snapshot.schema()).map_err(ErrorKind::Operation)?;
        Ok(Some(snapshot))
    }
    pub(crate) async fn semantic(
        &self,
        selected: &Selection<'_>,
        lease: &ServiceCredentialLease<'_>,
    ) -> Result<Option<graphit_data::lance_branch_store::BranchSnapshot>, ErrorKind> {
        selected.check(lease)?;
        let Some(component) = self
            .root
            .components()
            .iter()
            .find(|component| component.role == NativeRole::Wiki)
        else {
            return Ok(None);
        };
        let NativeSnapshot::Lance {
            dataset_version,
            manifest_path,
            manifest_sha256,
        } = &component.snapshot
        else {
            return Err(ErrorKind::UnsupportedComponent);
        };
        let path = self.prefix.clone().join(component.path.as_str());
        let mut uri =
            Url::parse(&format!("s3://{}/", self.bucket)).map_err(|_| ErrorKind::InvalidBinding)?;
        uri.path_segments_mut()
            .map_err(|_| ErrorKind::InvalidBinding)?
            .clear()
            .extend(path.as_ref().split('/'));
        let (snapshot, location) = wait(selected.request().snapshot(), async {
            graphit_data::lance_branch_store::BranchSnapshot::capture_native(
                uri.as_str(),
                Arc::clone(&self.session),
                *dataset_version,
            )
            .await
            .map_err(|error| ErrorKind::Embedding(Box::new(error.into())))
        })
        .await?;
        selected.check(lease)?;
        if location.version != *dataset_version
            || location.path
                != Path::parse(format!("{path}/{}", manifest_path.as_str()))
                    .map_err(|_| ErrorKind::InvalidBinding)?
        {
            return Err(ErrorKind::ManifestDigest);
        }
        let digest =
            hash_object(&self.store, &location.path, selected.request().snapshot()).await?;
        selected.check(lease)?;
        if digest != *manifest_sha256 {
            return Err(ErrorKind::ManifestDigest);
        }
        graphit_data::knowledge_embeddings::native_knowledge_embedding_dimensions(snapshot.schema())
            .map_err(|error| ErrorKind::Embedding(Box::new(error)))
            .map(|dimensions| dimensions.map(|_| snapshot))
    }
    pub(crate) async fn graph(
        &self,
        selected: &Selection<'_>,
        lease: &ServiceCredentialLease<'_>,
        policy: graphit_data::LocalGraphPolicy,
    ) -> Result<Arc<graphit_data::remote_graph::RemoteIcebugGraph>, ErrorKind> {
        selected.check(lease)?;
        let component = self
            .root
            .components()
            .iter()
            .find(|component| component.role == NativeRole::Graph)
            .ok_or(ErrorKind::UnsupportedComponent)?;
        let NativeSnapshot::Icebug {
            manifest_path,
            generation,
            manifest_sha256,
        } = &component.snapshot
        else {
            return Err(ErrorKind::UnsupportedComponent);
        };
        if manifest_path != "icebug.json" {
            return Err(ErrorKind::ManifestDigest);
        }
        let graph = graphit_data::remote_graph::RemoteIcebugGraph::open(
            Arc::clone(&self.store),
            self.prefix.clone().join(component.path.as_str()),
            *manifest_sha256,
            policy,
            Arc::clone(selected.request().snapshot()),
        )
        .await
        .map_err(ErrorKind::Graph)?;
        selected.check(lease)?;
        if graph.schema().manifest_generation() != generation {
            return Err(ErrorKind::ManifestDigest);
        }
        Ok(Arc::new(graph))
    }
    async fn open_lance(
        &self,
        selected: &Selection<'_>,
        lease: &ServiceCredentialLease<'_>,
        role: NativeRole,
    ) -> Result<Option<ModuleSnapshot>, ErrorKind> {
        selected.check(lease)?;
        let Some(component) = self.root.components().iter().find(|v| v.role == role) else {
            return Ok(None);
        };
        let NativeSnapshot::Lance {
            dataset_version,
            manifest_path,
            manifest_sha256,
        } = &component.snapshot
        else {
            return Err(ErrorKind::UnsupportedComponent);
        };
        let schema = match role {
            NativeRole::Source => graphit_ast::indexed_source::source_schema(),
            NativeRole::Wiki => graphit_data::knowledge_index::physical_schema(),
            _ => return Err(ErrorKind::UnsupportedComponent),
        };
        let path = self.prefix.clone().join(component.path.as_str());
        let mut uri =
            Url::parse(&format!("s3://{}/", self.bucket)).map_err(|_| ErrorKind::InvalidBinding)?;
        uri.path_segments_mut()
            .map_err(|_| ErrorKind::InvalidBinding)?
            .clear()
            .extend(path.as_ref().split('/'));
        let (snapshot, location) = if role == NativeRole::Wiki {
            let (pin, location) = wait(selected.request().snapshot(), async {
                graphit_data::lance_branch_store::BranchSnapshot::capture_native(
                    uri.as_str(),
                    Arc::clone(&self.session),
                    *dataset_version,
                )
                .await
                .map_err(|error| ErrorKind::Embedding(Box::new(error.into())))
            })
            .await?;
            graphit_data::knowledge_embeddings::native_knowledge_embedding_dimensions(pin.schema())
                .map_err(|error| ErrorKind::Embedding(Box::new(error)))?;
            (pin.module_snapshot(), location)
        } else {
            let (store, location) = wait(selected.request().snapshot(), async {
                LanceModuleStore::open_native_snapshot(
                    uri.as_str(),
                    schema,
                    Arc::clone(&self.session),
                    *dataset_version,
                )
                .await
                .map_err(ErrorKind::Storage)
            })
            .await?;
            (store.snapshot(), location)
        };
        selected.check(lease)?;
        if location.version != *dataset_version
            || location.path
                != Path::parse(format!("{path}/{}", manifest_path.as_str()))
                    .map_err(|_| ErrorKind::InvalidBinding)?
        {
            return Err(ErrorKind::ManifestDigest);
        }
        let digest =
            hash_object(&self.store, &location.path, selected.request().snapshot()).await?;
        selected.check(lease)?;
        if digest != *manifest_sha256 {
            return Err(ErrorKind::ManifestDigest);
        }
        Ok(Some(snapshot))
    }
}

async fn wait<T>(
    original: &SnapshotContext,
    future: impl Future<Output = Result<T, ErrorKind>>,
) -> Result<T, ErrorKind> {
    original.check()?;
    let deadline = original.required_deadline()?;
    let result = tokio::select! {
        biased;
        () = original.cancellation().cancelled() => Err(io::Error::from(io::ErrorKind::Interrupted).into()),
        () = tokio::time::sleep_until(deadline.into()) => Err(io::Error::from(io::ErrorKind::TimedOut).into()),
        result = future => result,
    };
    original.check()?;
    result
}
pub(crate) async fn read_small(
    store: &Arc<dyn ObjectStore>,
    path: &Path,
    limit: usize,
    original: &SnapshotContext,
) -> Result<Vec<u8>, ErrorKind> {
    let response = wait(original, async {
        store.get(path).await.map_err(ErrorKind::Object)
    })
    .await?;
    if response.meta.size > limit as u64 {
        return Err(io::Error::from(io::ErrorKind::FileTooLarge).into());
    }
    let mut stream = response.into_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = wait(original, async {
        stream.try_next().await.map_err(ErrorKind::Object)
    })
    .await?
    {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(io::Error::from(io::ErrorKind::FileTooLarge).into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
pub(crate) async fn hash_object(
    store: &Arc<dyn ObjectStore>,
    path: &Path,
    original: &SnapshotContext,
) -> Result<[u8; 32], ErrorKind> {
    let response = wait(original, async {
        store.get(path).await.map_err(ErrorKind::Object)
    })
    .await?;
    let mut stream = response.into_stream();
    let mut hash = Sha256::new();
    while let Some(chunk) = wait(original, async {
        stream.try_next().await.map_err(ErrorKind::Object)
    })
    .await?
    {
        hash.update(&chunk);
    }
    Ok(hash.finalize().into())
}

struct AdmittedStore {
    bucket: String,
    store: Arc<dyn ObjectStore>,
}
impl fmt::Debug for AdmittedStore {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str("admitted remote native store")
    }
}
#[async_trait::async_trait]
impl ObjectStoreProvider for AdmittedStore {
    async fn new_store(
        &self,
        url: Url,
        params: &ObjectStoreParams,
    ) -> Result<LanceObjectStore, lance::Error> {
        if url.scheme() != "s3"
            || url.host_str() != Some(self.bucket.as_str())
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(lance::Error::invalid_input(
                "remote native store binding mismatch",
            ));
        }
        Ok(LanceObjectStore::new(
            Arc::clone(&self.store),
            url,
            params.block_size,
            None,
            false,
            true,
            DEFAULT_CLOUD_IO_PARALLELISM,
            StorageOptions(Default::default()).download_retry_count(),
            None,
        ))
    }
}
