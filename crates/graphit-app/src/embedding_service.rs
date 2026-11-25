//! Embedding execution for an already admitted service selection.
//!
//! Model observations are data, never authority. The caller resolves a coherent
//! vector-space identity and retains its corpus admission independently. Remote
//! selection does not require a local slot, manifest, process or loaded model.
use axum::http::{HeaderMap, HeaderValue, header};
use graphit_ai::{
    direct_embedding::{self, DirectEmbeddingError, DirectEmbeddingInput},
    embedding::{EmbeddingError, EmbeddingLimits, GuardedEmbeddingError},
    graphit_embedding::{GraphitEmbeddingInput, GraphitEmbeddings},
};
use graphit_auth::{
    ServiceBindingView, ServiceMode, ServicePurpose, ServiceSelection, ServiceUseError,
    broker::{BrokerEmbeddingService, BrokerError, BrokerRequestContext},
};
use graphit_core::runtime_key::RuntimeKeyOwner;
use graphit_daemon::{
    broker_verifiers::{BrokerVerifierCache, BrokerVerifierCacheError},
    model_service::{
        EmbedInput, EmbedOperation, ExpectedModel, ModelErrorKind, ModelServiceClient,
        ModelServiceError,
    },
};
use graphit_http::client::BoundedClient;
use std::sync::Arc;
use std::{error::Error, fmt, io, net::SocketAddr, time::Instant};
use tokio::net::TcpStream;
use zeroize::Zeroizing;

/// Portable model observations resolved by the composition layer. In particular,
/// `revision` must describe the resolved service rather than a fabricated local
/// artifact. Operational device/threads and credentials do not identify vectors.
#[derive(Clone, Eq, PartialEq)]
pub struct EmbeddingMetadata {
    pub vector_id: [u8; 32],
    pub dimensions: u64,
    pub model: String,
    pub revision: String,
    pub local_generation: Option<u64>,
}
impl EmbeddingMetadata {
    pub fn local(expected: &ExpectedModel) -> Result<Self, EmbeddingServiceError> {
        let metadata = Self {
            vector_id: expected.vector_id(),
            dimensions: expected
                .dimensions()
                .ok_or(EmbeddingServiceError::Metadata)?,
            model: expected.model().to_owned(),
            revision: expected.revision().to_owned(),
            local_generation: None,
        };
        metadata.validate()?;
        Ok(metadata)
    }
    pub fn remote(
        vector_id: [u8; 32],
        dimensions: u64,
        model: String,
        revision: String,
    ) -> Result<Self, EmbeddingServiceError> {
        let metadata = Self {
            vector_id,
            dimensions,
            model,
            revision,
            local_generation: None,
        };
        metadata.validate()?;
        Ok(metadata)
    }
    fn validate(&self) -> Result<usize, EmbeddingServiceError> {
        if self.model.trim().is_empty() || self.revision.trim().is_empty() {
            return Err(EmbeddingServiceError::Metadata);
        }
        usize::try_from(self.dimensions)
            .ok()
            .filter(|width| *width > 0)
            .ok_or(EmbeddingServiceError::Metadata)
    }
    fn matches_local(&self, expected: &ExpectedModel) -> bool {
        self.vector_id == expected.vector_id()
            && Some(self.dimensions) == expected.dimensions()
            && self.model == expected.model()
            && self.revision == expected.revision()
    }
}

pub struct EmbeddingResult {
    pub metadata: EmbeddingMetadata,
    pub vectors: Vec<Vec<f32>>,
}

#[derive(Clone, Copy)]
pub struct LocalEmbeddingResources<'a> {
    pub client: &'a ModelServiceClient,
    pub runtime: &'a RuntimeKeyOwner,
    pub expected: &'a ExpectedModel,
    pub address: SocketAddr,
}

/// Borrow existing owners. `local` is required only for an actual Local binding.
pub struct EmbeddingResources<'a> {
    pub local: Option<LocalEmbeddingResources<'a>>,
    pub http: Option<&'a BoundedClient>,
    pub brokers: &'a BrokerVerifierCache,
    pub limits: EmbeddingLimits,
}

#[derive(Debug)]
pub enum EmbeddingServiceError {
    Service(ServiceUseError),
    Local(ModelServiceError),
    Direct(DirectEmbeddingError),
    Broker(BrokerError),
    Verifier(BrokerVerifierCacheError),
    Graphit(GuardedEmbeddingError<ServiceUseError>),
    MissingLocal,
    MissingHttp,
    Unavailable,
    Operation(io::Error),
    Metadata,
    Output,
}
impl fmt::Display for EmbeddingServiceError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "embedding service: {self:?}")
    }
}
impl Error for EmbeddingServiceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Service(error) => Some(error),
            Self::Local(error) => Some(error),
            Self::Direct(error) => Some(error),
            Self::Broker(error) => Some(error),
            Self::Verifier(error) => Some(error),
            Self::Graphit(error) => Some(error),
            Self::Operation(error) => Some(error),
            Self::Metadata
            | Self::Output
            | Self::MissingLocal
            | Self::MissingHttp
            | Self::Unavailable => None,
        }
    }
}
impl From<ServiceUseError> for EmbeddingServiceError {
    fn from(error: ServiceUseError) -> Self {
        Self::Service(error)
    }
}
impl From<ModelServiceError> for EmbeddingServiceError {
    fn from(error: ModelServiceError) -> Self {
        Self::Local(error)
    }
}
impl From<DirectEmbeddingError> for EmbeddingServiceError {
    fn from(error: DirectEmbeddingError) -> Self {
        Self::Direct(error)
    }
}
impl From<io::Error> for EmbeddingServiceError {
    fn from(error: io::Error) -> Self {
        Self::Operation(error)
    }
}

fn output(
    metadata: &EmbeddingMetadata,
    expected_count: usize,
    vectors: Vec<Vec<f32>>,
    local_generation: Option<u64>,
) -> Result<EmbeddingResult, EmbeddingServiceError> {
    let width = metadata.validate()?;
    if vectors.len() != expected_count
        || vectors
            .iter()
            .any(|vector| vector.len() != width || vector.iter().any(|n| !n.is_finite()))
    {
        return Err(EmbeddingServiceError::Output);
    }
    let mut metadata = metadata.clone();
    metadata.local_generation = local_generation;
    Ok(EmbeddingResult { metadata, vectors })
}

fn direct_input(input: EmbedInput) -> DirectEmbeddingInput {
    match input {
        EmbedInput::Query | EmbedInput::CodeQuery => DirectEmbeddingInput::Query,
        EmbedInput::Document | EmbedInput::CodeDocument => DirectEmbeddingInput::Document,
    }
}

/// Preserve the strict private RPC and its actual model generation.
pub async fn embed_local(
    selection: &mut ServiceSelection<'_>,
    resources: LocalEmbeddingResources<'_>,
    metadata: &EmbeddingMetadata,
    operation: EmbedOperation,
) -> Result<EmbeddingResult, EmbeddingServiceError> {
    if !matches!(
        selection.binding()?,
        ServiceBindingView::Local {
            purpose: ServicePurpose::Embedding,
            ..
        }
    ) {
        return Err(ServiceUseError::WrongMode.into());
    }
    if !metadata.matches_local(resources.expected) {
        return Err(EmbeddingServiceError::Metadata);
    }
    metadata.validate()?;
    let count = operation.texts.len();
    let lease = selection.local()?;
    let result = resources
        .client
        .embed(
            &lease,
            resources.runtime,
            resources.expected,
            operation,
            TcpStream::connect(resources.address),
        )
        .await?;
    lease.check()?;
    output(metadata, count, result.vectors, Some(result.generation))
}

/// The caller captures this selection from a finite operation request. This keeps
/// Direct's existing lease and deadline enforcement intact, including when corpus
/// indexing itself uses a continuous Original with independent admissions.
pub async fn embed_direct(
    selection: &mut ServiceSelection<'_>,
    resources: &EmbeddingResources<'_>,
    metadata: &EmbeddingMetadata,
    operation: EmbedOperation,
) -> Result<EmbeddingResult, EmbeddingServiceError> {
    metadata.validate()?;
    let selected = match selection.binding()? {
        ServiceBindingView::Direct {
            purpose: ServicePurpose::Embedding,
            protocol,
            model,
            model_revision,
            recipe,
            dimensions,
            ..
        } => {
            crate::embedding_metadata::direct(protocol, model, model_revision, recipe, dimensions)?
        }
        _ => return Err(ServiceUseError::WrongMode.into()),
    };
    if !same_space(metadata, &selected) {
        return Err(EmbeddingServiceError::Metadata);
    }
    let original = selection.original()?;
    original.snapshot().check()?;
    let deadline = original.snapshot().required_deadline()?;
    // Never widen the requested operation by handing Direct a longer-lived lease.
    if deadline > operation.deadline {
        return Err(EmbeddingServiceError::Metadata);
    }
    if Instant::now() >= operation.deadline {
        return Err(io::ErrorKind::TimedOut.into());
    }
    let lease = selection.credential()?;
    let texts: Vec<_> = operation.texts.iter().map(String::as_str).collect();
    let vectors = direct_embedding::embed(
        &lease,
        &texts,
        direct_input(operation.input),
        resources.http.ok_or(EmbeddingServiceError::MissingHttp)?,
        resources.limits,
    )
    .await?;
    lease.check()?;
    output(metadata, texts.len(), vectors, None)
}
impl From<io::ErrorKind> for EmbeddingServiceError {
    fn from(kind: io::ErrorKind) -> Self {
        Self::Operation(kind.into())
    }
}

impl From<BrokerError> for EmbeddingServiceError {
    fn from(error: BrokerError) -> Self {
        Self::Broker(error)
    }
}
impl From<BrokerVerifierCacheError> for EmbeddingServiceError {
    fn from(error: BrokerVerifierCacheError) -> Self {
        Self::Verifier(error)
    }
}
impl From<GuardedEmbeddingError<ServiceUseError>> for EmbeddingServiceError {
    fn from(error: GuardedEmbeddingError<ServiceUseError>) -> Self {
        Self::Graphit(error)
    }
}
fn same_space(left: &EmbeddingMetadata, right: &EmbeddingMetadata) -> bool {
    left.vector_id == right.vector_id
        && left.dimensions == right.dimensions
        && left.model == right.model
        && left.revision == right.revision
}
fn finite_operation(
    selection: &mut ServiceSelection<'_>,
    deadline: Instant,
) -> Result<(), EmbeddingServiceError> {
    let original = selection.original()?;
    original.snapshot().check()?;
    if original.snapshot().required_deadline()? > deadline {
        return Err(EmbeddingServiceError::Metadata);
    }
    if Instant::now() >= deadline {
        return Err(io::ErrorKind::TimedOut.into());
    }
    Ok(())
}
async fn broker_service(
    selection: &mut ServiceSelection<'_>,
    resources: &EmbeddingResources<'_>,
) -> Result<
    (
        Arc<graphit_auth::broker::BrokerVerifier>,
        BrokerEmbeddingService,
    ),
    EmbeddingServiceError,
> {
    if !matches!(
        selection.binding()?,
        ServiceBindingView::Broker {
            purpose: ServicePurpose::Embedding,
            ..
        }
    ) {
        return Err(ServiceUseError::WrongMode.into());
    }
    let verifier = resources.brokers.get_for_service(selection)?;
    let original = Arc::clone(selection.original()?.snapshot());
    let metadata = verifier
        .metadata(BrokerRequestContext::new(&original))
        .await?;
    selection.mode()?;
    original.check()?;
    let service = metadata
        .embedding_service()?
        .ok_or(EmbeddingServiceError::Unavailable)?;
    Ok((verifier, service))
}

/// Resolve only the explicitly selected embedding service. This never resolves
/// reranking or starts a model process. The returned metadata grants no access.
pub async fn metadata(
    selection: &mut ServiceSelection<'_>,
    resources: &EmbeddingResources<'_>,
) -> Result<EmbeddingMetadata, EmbeddingServiceError> {
    match selection.binding()? {
        ServiceBindingView::Local {
            purpose: ServicePurpose::Embedding,
            ..
        } => EmbeddingMetadata::local(
            resources
                .local
                .ok_or(EmbeddingServiceError::MissingLocal)?
                .expected,
        ),
        ServiceBindingView::Direct {
            purpose: ServicePurpose::Embedding,
            protocol,
            model,
            model_revision,
            recipe,
            dimensions,
            ..
        } => crate::embedding_metadata::direct(protocol, model, model_revision, recipe, dimensions),
        ServiceBindingView::Broker {
            purpose: ServicePurpose::Embedding,
            ..
        } => {
            let (_, service) = broker_service(selection, resources).await?;
            crate::embedding_metadata::broker(&service)
        }
        _ => Err(ServiceUseError::WrongMode.into()),
    }
}

/// Execute the selected mode through its concrete existing client. Only the Local
/// branch requires a model child; no mode silently falls back to another provider.
pub async fn embed(
    selection: &mut ServiceSelection<'_>,
    resources: &EmbeddingResources<'_>,
    expected: &EmbeddingMetadata,
    operation: EmbedOperation,
) -> Result<EmbeddingResult, EmbeddingServiceError> {
    match selection.binding()? {
        ServiceBindingView::Local {
            purpose: ServicePurpose::Embedding,
            ..
        } => {
            embed_local(
                selection,
                resources.local.ok_or(EmbeddingServiceError::MissingLocal)?,
                expected,
                operation,
            )
            .await
        }
        ServiceBindingView::Direct {
            purpose: ServicePurpose::Embedding,
            ..
        } => embed_direct(selection, resources, expected, operation).await,
        ServiceBindingView::Broker {
            purpose: ServicePurpose::Embedding,
            ..
        } => {
            finite_operation(selection, operation.deadline)?;
            let (verifier, service) = broker_service(selection, resources).await?;
            if !same_space(expected, &crate::embedding_metadata::broker(&service)?) {
                return Err(EmbeddingServiceError::Metadata);
            }
            let client = GraphitEmbeddings::new(
                resources.http.ok_or(EmbeddingServiceError::MissingHttp)?,
                &service,
                resources.limits,
            )
            .map_err(|source| {
                EmbeddingServiceError::Graphit(GuardedEmbeddingError::Embedding {
                    source,
                    effect: graphit_http::client::NetworkEffect::NotSent,
                    completed: Vec::new(),
                    usage: Vec::new(),
                })
            })?;
            let lease = if selection.mode()? == ServiceMode::BrokerNative {
                selection.native_credential(&verifier).await?
            } else {
                selection.credential()?
            };
            let mut headers = HeaderMap::new();
            if let Some(credential) = lease.credential()? {
                let mut bearer = Zeroizing::new(String::from("Bearer "));
                bearer
                    .try_reserve(credential.len())
                    .map_err(io::Error::other)?;
                bearer.push_str(credential);
                let mut value = HeaderValue::from_bytes(bearer.as_bytes())
                    .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
                value.set_sensitive(true);
                headers.insert(header::AUTHORIZATION, value);
            }
            let texts: Vec<_> = operation.texts.iter().map(String::as_str).collect();
            let input = match operation.input {
                EmbedInput::Query | EmbedInput::CodeQuery => GraphitEmbeddingInput::Query,
                EmbedInput::Document | EmbedInput::CodeDocument => GraphitEmbeddingInput::Document,
            };
            let original = lease.original()?;
            let context = graphit_ai::context::InferenceContext::new(
                original.cancellation(),
                operation.deadline,
            );
            let result = client
                .embed_guarded(&texts, input, &headers, context, &mut || lease.check())
                .await?;
            lease.check()?;
            output(expected, texts.len(), result.into_vectors(), None)
        }
        _ => Err(ServiceUseError::WrongMode.into()),
    }
}

impl EmbeddingServiceError {
    pub fn into_io(self) -> io::Error {
        let kind = match self.kind() {
            ModelErrorKind::Cancelled => io::ErrorKind::Interrupted,
            ModelErrorKind::Deadline => io::ErrorKind::TimedOut,
            ModelErrorKind::Authentication => io::ErrorKind::PermissionDenied,
            ModelErrorKind::Configuration | ModelErrorKind::Limit => io::ErrorKind::InvalidInput,
            ModelErrorKind::ConfigChanged | ModelErrorKind::Output => io::ErrorKind::InvalidData,
            ModelErrorKind::Selection => io::ErrorKind::NotConnected,
            _ => io::ErrorKind::Other,
        };
        io::Error::new(kind, self)
    }

    /// Finite classification for consumers' existing model error/status paths;
    /// the concrete provider cause remains available through Error::source.
    pub fn kind(&self) -> ModelErrorKind {
        match self {
            Self::Local(error) => error.kind(),
            Self::Operation(error) => match error.kind() {
                io::ErrorKind::Interrupted => ModelErrorKind::Cancelled,
                io::ErrorKind::TimedOut => ModelErrorKind::Deadline,
                _ => ModelErrorKind::Transport,
            },
            Self::Service(error) => service_kind(error),
            Self::Verifier(BrokerVerifierCacheError::Service(error)) => service_kind(error),
            Self::Verifier(_) => ModelErrorKind::Authentication,
            Self::Broker(BrokerError::Cancelled) => ModelErrorKind::Cancelled,
            Self::Broker(BrokerError::Deadline) => ModelErrorKind::Deadline,
            Self::Broker(BrokerError::Connection | BrokerError::Transport) => {
                ModelErrorKind::Transport
            }
            Self::Broker(_) => ModelErrorKind::Authentication,
            Self::Direct(DirectEmbeddingError::UnsupportedBinding) => ModelErrorKind::Selection,
            Self::Direct(DirectEmbeddingError::Context(error)) => io_kind(error.kind()),
            Self::Direct(DirectEmbeddingError::Service { source, .. })
            | Self::Graphit(GuardedEmbeddingError::Admission { source, .. }) => {
                service_kind(source)
            }
            Self::Direct(DirectEmbeddingError::Embedding { source, .. })
            | Self::Graphit(GuardedEmbeddingError::Embedding { source, .. }) => {
                embedding_kind(*source)
            }
            Self::Output => ModelErrorKind::Output,
            Self::Metadata => ModelErrorKind::ConfigChanged,
            Self::MissingHttp | Self::MissingLocal | Self::Unavailable => ModelErrorKind::Selection,
        }
    }
}

pub(crate) fn service_kind(error: &ServiceUseError) -> ModelErrorKind {
    use graphit_auth::{AuthError, profile::ProfileError, store::AuthStoreError};
    use graphit_core::auth_document::AuthDocumentError;
    match error {
        ServiceUseError::Profile(ProfileError::Cancelled)
        | ServiceUseError::Store(AuthStoreError::Profile(ProfileError::Cancelled))
        | ServiceUseError::Store(AuthStoreError::Core(AuthDocumentError::Cancelled))
        | ServiceUseError::Broker(BrokerError::Cancelled) => ModelErrorKind::Cancelled,
        ServiceUseError::Profile(ProfileError::Deadline)
        | ServiceUseError::Store(AuthStoreError::Profile(ProfileError::Deadline))
        | ServiceUseError::Store(AuthStoreError::Core(AuthDocumentError::Deadline))
        | ServiceUseError::Broker(BrokerError::Deadline) => ModelErrorKind::Deadline,
        ServiceUseError::Auth(AuthError::Io | AuthError::State | AuthError::Entropy)
        | ServiceUseError::Store(AuthStoreError::Core(
            AuthDocumentError::Io | AuthDocumentError::Resource | AuthDocumentError::Unavailable,
        ))
        | ServiceUseError::Broker(BrokerError::Connection | BrokerError::Transport) => {
            ModelErrorKind::Transport
        }
        ServiceUseError::Broker(BrokerError::Busy) => ModelErrorKind::Busy,
        ServiceUseError::Broker(
            BrokerError::InvalidMetadata | BrokerError::InvalidDocument | BrokerError::InvalidKeys,
        ) => ModelErrorKind::Output,
        ServiceUseError::Auth(AuthError::Limit)
        | ServiceUseError::Profile(ProfileError::Limit)
        | ServiceUseError::Store(AuthStoreError::Profile(ProfileError::Limit))
        | ServiceUseError::Store(AuthStoreError::Core(AuthDocumentError::Limit))
        | ServiceUseError::Broker(BrokerError::Limit) => ModelErrorKind::Limit,
        ServiceUseError::Profile(ProfileError::Conflict | ProfileError::Stale)
        | ServiceUseError::Store(AuthStoreError::Profile(
            ProfileError::Conflict | ProfileError::Stale,
        ))
        | ServiceUseError::Store(AuthStoreError::Core(AuthDocumentError::Conflict)) => {
            ModelErrorKind::ConfigChanged
        }
        ServiceUseError::Auth(AuthError::InvalidSetting(_))
        | ServiceUseError::Profile(ProfileError::InvalidPolicy | ProfileError::InvalidDocument)
        | ServiceUseError::Store(AuthStoreError::Profile(
            ProfileError::InvalidPolicy | ProfileError::InvalidDocument,
        ))
        | ServiceUseError::Store(AuthStoreError::Core(AuthDocumentError::InvalidPolicy))
        | ServiceUseError::Broker(BrokerError::InvalidPolicy) => ModelErrorKind::Configuration,
        _ => ModelErrorKind::Authentication,
    }
}

fn io_kind(kind: io::ErrorKind) -> ModelErrorKind {
    match kind {
        io::ErrorKind::Interrupted => ModelErrorKind::Cancelled,
        io::ErrorKind::TimedOut => ModelErrorKind::Deadline,
        io::ErrorKind::PermissionDenied => ModelErrorKind::Authentication,
        _ => ModelErrorKind::Transport,
    }
}

fn embedding_kind(error: EmbeddingError) -> ModelErrorKind {
    use graphit_ai::context::ContextError;
    use graphit_http::client::NetworkError;
    match error {
        EmbeddingError::Context(ContextError::Cancelled)
        | EmbeddingError::Network(NetworkError::Cancelled) => ModelErrorKind::Cancelled,
        EmbeddingError::Context(ContextError::Deadline)
        | EmbeddingError::Network(NetworkError::Deadline) => ModelErrorKind::Deadline,
        EmbeddingError::Network(_) => ModelErrorKind::Transport,
        EmbeddingError::Credential => ModelErrorKind::Authentication,
        EmbeddingError::Status(status) if matches!(status.as_u16(), 401 | 403) => {
            ModelErrorKind::Authentication
        }
        EmbeddingError::Status(_) => ModelErrorKind::Transport,
        EmbeddingError::Limit | EmbeddingError::Allocation => ModelErrorKind::Limit,
        EmbeddingError::InvalidPolicy | EmbeddingError::InvalidRequest => {
            ModelErrorKind::Configuration
        }
        EmbeddingError::Codec | EmbeddingError::Shape | EmbeddingError::NonFinite => {
            ModelErrorKind::Output
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_store_interruption_preserves_consumer_io_and_cause() {
        use graphit_auth::{profile::ProfileError, store::AuthStoreError};
        use graphit_core::auth_document::AuthDocumentError;
        for (store, kind, io_kind) in [
            (
                AuthStoreError::Core(AuthDocumentError::Cancelled),
                ModelErrorKind::Cancelled,
                io::ErrorKind::Interrupted,
            ),
            (
                AuthStoreError::Core(AuthDocumentError::Deadline),
                ModelErrorKind::Deadline,
                io::ErrorKind::TimedOut,
            ),
            (
                AuthStoreError::Profile(ProfileError::Cancelled),
                ModelErrorKind::Cancelled,
                io::ErrorKind::Interrupted,
            ),
            (
                AuthStoreError::Profile(ProfileError::Deadline),
                ModelErrorKind::Deadline,
                io::ErrorKind::TimedOut,
            ),
            (
                AuthStoreError::Core(AuthDocumentError::Denied),
                ModelErrorKind::Authentication,
                io::ErrorKind::PermissionDenied,
            ),
            (
                AuthStoreError::Core(AuthDocumentError::Conflict),
                ModelErrorKind::ConfigChanged,
                io::ErrorKind::InvalidData,
            ),
            (
                AuthStoreError::Profile(ProfileError::Stale),
                ModelErrorKind::ConfigChanged,
                io::ErrorKind::InvalidData,
            ),
        ] {
            let error = EmbeddingServiceError::Service(ServiceUseError::Store(store));
            assert_eq!(error.kind(), kind);
            let error = error.into_io();
            assert_eq!(error.kind(), io_kind);
            assert!(matches!(
                error.get_ref().and_then(|source| source.downcast_ref::<EmbeddingServiceError>()),
                Some(EmbeddingServiceError::Service(ServiceUseError::Store(actual))) if *actual == store
            ));
        }
        for cause in [
            ServiceUseError::WrongMode,
            ServiceUseError::Auth(graphit_auth::AuthError::Denied),
        ] {
            assert_eq!(
                EmbeddingServiceError::Service(cause).into_io().kind(),
                io::ErrorKind::PermissionDenied
            );
        }
    }

    #[test]
    fn service_transport_and_configuration_failures_are_not_authentication_denials() {
        use graphit_auth::store::AuthStoreError;
        use graphit_core::auth_document::AuthDocumentError;
        for (cause, expected) in [
            (
                ServiceUseError::Broker(BrokerError::Connection),
                ModelErrorKind::Transport,
            ),
            (
                ServiceUseError::Broker(BrokerError::Busy),
                ModelErrorKind::Busy,
            ),
            (
                ServiceUseError::Broker(BrokerError::Limit),
                ModelErrorKind::Limit,
            ),
            (
                ServiceUseError::Broker(BrokerError::InvalidMetadata),
                ModelErrorKind::Output,
            ),
            (
                ServiceUseError::Broker(BrokerError::InvalidDocument),
                ModelErrorKind::Output,
            ),
            (
                ServiceUseError::Broker(BrokerError::InvalidKeys),
                ModelErrorKind::Output,
            ),
            (
                ServiceUseError::Broker(BrokerError::InvalidToken),
                ModelErrorKind::Authentication,
            ),
            (
                ServiceUseError::Auth(graphit_auth::AuthError::Io),
                ModelErrorKind::Transport,
            ),
            (
                ServiceUseError::Store(AuthStoreError::Core(AuthDocumentError::Io)),
                ModelErrorKind::Transport,
            ),
            (
                ServiceUseError::Profile(graphit_auth::profile::ProfileError::InvalidDocument),
                ModelErrorKind::Configuration,
            ),
            (
                ServiceUseError::Auth(graphit_auth::AuthError::Denied),
                ModelErrorKind::Authentication,
            ),
        ] {
            assert_eq!(service_kind(&cause), expected);
        }
    }

    fn metadata() -> EmbeddingMetadata {
        EmbeddingMetadata::remote(
            [73; 32],
            2,
            "observed-model".into(),
            "observed-revision".into(),
        )
        .unwrap()
    }

    #[test]
    fn output_preserves_vectors_and_only_observed_local_generation() {
        let metadata = metadata();
        let vectors = vec![vec![0.25, -0.75], vec![1., 0.]];
        let local = output(&metadata, 2, vectors.clone(), Some(7)).unwrap();
        assert_eq!(local.vectors, vectors);
        assert_eq!(local.metadata.local_generation, Some(7));
        let remote = output(&local.metadata, 2, vectors.clone(), None).unwrap();
        assert_eq!(remote.vectors, vectors);
        assert_eq!(remote.metadata.local_generation, None);
        assert_eq!(remote.metadata.vector_id, metadata.vector_id);
        assert_eq!(remote.metadata.model, metadata.model);
        assert_eq!(remote.metadata.revision, metadata.revision);
    }

    #[test]
    fn incompatible_width_count_and_nonfinite_values_are_errors() {
        let metadata = metadata();
        for vectors in [
            vec![],
            vec![vec![1.]],
            vec![vec![f32::NAN, 0.]],
            vec![vec![0., f32::INFINITY]],
        ] {
            assert!(matches!(
                output(&metadata, 1, vectors, None),
                Err(EmbeddingServiceError::Output)
            ));
        }
        assert!(EmbeddingMetadata::remote([1; 32], 0, "model".into(), "revision".into()).is_err());
        assert!(EmbeddingMetadata::remote([1; 32], 2, "model".into(), "".into()).is_err());
    }

    #[test]
    fn code_roles_select_provider_query_or_document_without_prefixing_text() {
        assert_eq!(direct_input(EmbedInput::Query), DirectEmbeddingInput::Query);
        assert_eq!(
            direct_input(EmbedInput::CodeQuery),
            DirectEmbeddingInput::Query
        );
        assert_eq!(
            direct_input(EmbedInput::Document),
            DirectEmbeddingInput::Document
        );
        assert_eq!(
            direct_input(EmbedInput::CodeDocument),
            DirectEmbeddingInput::Document
        );
    }
}
