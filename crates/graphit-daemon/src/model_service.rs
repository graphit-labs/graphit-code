//! Shared lazy native model owners and a private, bounded RPC producer.
//! Authentication belongs to the caller; the compute child requires runtime possession.
mod client;
mod config;
mod local_execution;
mod memory_embeddings;
mod rpc;
mod service;

#[cfg(test)]
use crate::model_service as model_api;
#[cfg(test)]
#[path = "../tests/support/model_fixture.rs"]
mod test_fixture;

pub use client::{EmbedOperation, ExpectedModel, ModelServiceClient};
pub use config::{
    CapturedModelInput, ConfigId, ModelSettings, ResolvedConfig, RuntimeSettings,
    SelectionSettings, SlotSettings, TaskKind,
};
pub use local_execution::LocalExecutionSelection;
pub use memory_embeddings::{
    MemoryEmbeddingCause, MemoryEmbeddingFailure, MemoryEmbeddingIndexRead, MemoryEmbeddingModel,
    UserMemoryEmbeddingFailure, UserMemoryEmbeddingIndexRead,
};
pub use rpc::{
    EmbedInput, EmbedOutput, ModelEmbedError, ModelRpcClient, ModelRpcRequest, ModelRpcResponse,
    RefreshOutput, RerankOutput, RpcLimits, RuntimeCredential, RuntimeSecret, client_transport,
    serve_connection, server_transport,
};
pub use service::{ModelService, ServiceLimits};

use graphit_ai::{context::ContextError, model_artifact::ModelArtifactErrorKind};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, io};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ModelErrorKind {
    Configuration,
    Selection,
    ConfigChanged,
    Limit,
    Cancelled,
    Deadline,
    Shutdown,
    Artifact,
    Integrity,
    Native,
    Output,
    Worker,
    Busy,
    Closed,
    Authentication,
    UnsupportedExecution,
    Transport,
}

/// Embedding wire errors may also carry a finite limit reason; actual causes stay local.
pub struct ModelServiceError {
    kind: ModelErrorKind,
    cause: Option<Box<dyn Error + Send + Sync>>,
}
// Provenance observed by this client, not a claim about a remote execution phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeadlineCause {
    DeadlineLocalOperation,
    RemoteDeadlineCategory,
}
impl fmt::Display for DeadlineCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl Error for DeadlineCause {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelLimitReason {
    InputLimit,
    OutputLimit,
    NativeTokenLimit,
    ArtifactLimit,
    NativeInputLimit,
    AllocationLimit,
}

enum LimitCause {
    LocalInputLimit,
    LocalOutputLimit,
    Remote(ModelLimitReason),
}
impl fmt::Debug for LimitCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocalInputLimit => f.write_str("LocalInputLimit"),
            Self::LocalOutputLimit => f.write_str("LocalOutputLimit"),
            Self::Remote(reason) => f.debug_tuple("RemoteLimit").field(reason).finish(),
        }
    }
}
impl fmt::Display for LimitCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl Error for LimitCause {}

fn native_limit_reason(
    kind: graphit_ai::native_embedding::NativeEmbeddingErrorKind,
) -> Option<ModelLimitReason> {
    use graphit_ai::{embedding::EmbeddingError, native_embedding::NativeEmbeddingErrorKind as E};
    match kind {
        E::TokenLimit { .. } => Some(ModelLimitReason::NativeTokenLimit),
        E::ArtifactLimit => Some(ModelLimitReason::ArtifactLimit),
        E::Embedding(EmbeddingError::Limit) => Some(ModelLimitReason::NativeInputLimit),
        E::Embedding(EmbeddingError::Allocation) => Some(ModelLimitReason::AllocationLimit),
        _ => None,
    }
}
impl ModelServiceError {
    pub fn kind(&self) -> ModelErrorKind {
        self.kind
    }
    pub(crate) fn new(kind: ModelErrorKind) -> Self {
        Self { kind, cause: None }
    }
    fn operation_deadline() -> Self {
        Self::caused(
            ModelErrorKind::Deadline,
            DeadlineCause::DeadlineLocalOperation,
        )
    }
    fn remote_category(kind: ModelErrorKind) -> Self {
        if kind == ModelErrorKind::Deadline {
            Self::caused(kind, DeadlineCause::RemoteDeadlineCategory)
        } else {
            Self::new(kind)
        }
    }
    fn local_input_limit(self) -> Self {
        self.local_limit(LimitCause::LocalInputLimit)
    }
    fn local_output_limit(self) -> Self {
        self.local_limit(LimitCause::LocalOutputLimit)
    }
    fn local_limit(self, cause: LimitCause) -> Self {
        if self.kind == ModelErrorKind::Limit && self.cause.is_none() {
            Self::caused(self.kind, cause)
        } else {
            self
        }
    }
    fn limit_reason(&self) -> Option<ModelLimitReason> {
        if self.kind != ModelErrorKind::Limit {
            return None;
        }
        let cause = self.cause.as_deref()?;
        if let Some(cause) = cause.downcast_ref::<LimitCause>() {
            return Some(match cause {
                LimitCause::LocalInputLimit => ModelLimitReason::InputLimit,
                LimitCause::LocalOutputLimit => ModelLimitReason::OutputLimit,
                LimitCause::Remote(reason) => *reason,
            });
        }
        if let Some(cause) =
            cause.downcast_ref::<graphit_ai::native_embedding::NativeEmbeddingError>()
        {
            return native_limit_reason(cause.kind());
        }
        if let Some(cause) =
            cause.downcast_ref::<graphit_ai::native_embedding::worker::NativeWorkerError>()
            && let graphit_ai::native_embedding::worker::NativeWorkerErrorKind::Native(kind) =
                cause.kind()
        {
            return native_limit_reason(kind);
        }
        None
    }
    fn remote_embedding(error: ModelEmbedError) -> Self {
        if error.kind != ModelErrorKind::Limit && error.limit.is_some() {
            return Self::new(ModelErrorKind::Transport);
        }
        match error.limit {
            Some(reason) => Self::caused(error.kind, LimitCause::Remote(reason)),
            None => Self::remote_category(error.kind),
        }
    }
    pub(crate) fn caused<E: Error + Send + Sync + 'static>(kind: ModelErrorKind, e: E) -> Self {
        Self {
            kind,
            cause: Some(Box::new(e)),
        }
    }
    pub(crate) fn configuration<E: Error + Send + Sync + 'static>(e: E) -> Self {
        Self::caused(ModelErrorKind::Configuration, e)
    }
    pub(crate) fn io(e: io::Error) -> Self {
        let kind = match e.kind() {
            io::ErrorKind::Interrupted => ModelErrorKind::Cancelled,
            io::ErrorKind::TimedOut => ModelErrorKind::Deadline,
            io::ErrorKind::FileTooLarge | io::ErrorKind::OutOfMemory => ModelErrorKind::Limit,
            _ => ModelErrorKind::Configuration,
        };
        Self::caused(kind, e)
    }
}
impl From<graphit_ai::model_manifest::ModelBindingError> for ModelServiceError {
    fn from(e: graphit_ai::model_manifest::ModelBindingError) -> Self {
        use graphit_ai::model_manifest::BindingCause;
        let kind = match e.kind {
            io::ErrorKind::Interrupted => ModelErrorKind::Cancelled,
            io::ErrorKind::TimedOut => ModelErrorKind::Deadline,
            io::ErrorKind::FileTooLarge | io::ErrorKind::OutOfMemory => ModelErrorKind::Limit,
            _ => match e.cause {
                BindingCause::Capacity | BindingCause::Allocation => ModelErrorKind::Limit,
                BindingCause::Selection => ModelErrorKind::Selection,
                BindingCause::Artifact => ModelErrorKind::Artifact,
                _ => ModelErrorKind::Configuration,
            },
        };
        Self::caused(kind, e)
    }
}
impl From<graphit_core::runtime_key::RuntimeKeyError> for ModelServiceError {
    fn from(error: graphit_core::runtime_key::RuntimeKeyError) -> Self {
        use graphit_core::runtime_key::RuntimeKeyError;
        let kind = match error {
            RuntimeKeyError::Cancelled => ModelErrorKind::Cancelled,
            RuntimeKeyError::Deadline => ModelErrorKind::Deadline,
            _ => ModelErrorKind::Authentication,
        };
        Self::caused(kind, error)
    }
}
impl fmt::Debug for ModelServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut output = f.debug_struct("ModelServiceError");
        output.field("kind", &self.kind);
        if let Some(cause) = self.cause.as_deref() {
            if let Some(deadline) = cause.downcast_ref::<DeadlineCause>() {
                output.field("deadline", deadline);
            } else if let Some(limit) = cause.downcast_ref::<LimitCause>() {
                output.field("limit", limit);
            } else if let Some(error) =
                cause.downcast_ref::<graphit_core::runtime_key::RuntimeKeyError>()
            {
                output.field("runtime_key", error);
            } else if let Some(error) = cause.downcast_ref::<graphit_auth::ServiceUseError>() {
                use graphit_auth::ServiceUseError;
                match error {
                    ServiceUseError::Auth(error) => {
                        output.field("service_auth", error);
                    }
                    ServiceUseError::Store(error) => {
                        output.field("service_store", error);
                    }
                    ServiceUseError::Profile(error) => {
                        output.field("service_profile", error);
                    }
                    ServiceUseError::Broker(_) => {
                        output.field("service", &"Broker");
                    }
                    ServiceUseError::Disabled => {
                        output.field("service", &"Disabled");
                    }
                    ServiceUseError::MissingCredential => {
                        output.field("service", &"MissingCredential");
                    }
                    ServiceUseError::NeedsRefresh => {
                        output.field("service", &"NeedsRefresh");
                    }
                    ServiceUseError::LoginRequired => {
                        output.field("service", &"LoginRequired");
                    }
                    ServiceUseError::WrongMode => {
                        output.field("service", &"WrongMode");
                    }
                }
            }
        }
        output.finish_non_exhaustive()
    }
}
impl fmt::Display for ModelServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "model service: {:?}", self.kind)
    }
}
impl Error for ModelServiceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause.as_ref().map(|e| &**e as _)
    }
}
impl From<ContextError> for ModelServiceError {
    fn from(e: ContextError) -> Self {
        Self::caused(
            match e {
                ContextError::Cancelled => ModelErrorKind::Cancelled,
                ContextError::Deadline => ModelErrorKind::Deadline,
            },
            e,
        )
    }
}
pub type ModelResult<T> = Result<T, ModelServiceError>;

fn native_error_kind(
    kind: graphit_ai::native_embedding::NativeEmbeddingErrorKind,
) -> ModelErrorKind {
    use graphit_ai::native_embedding::NativeEmbeddingErrorKind as E;
    match kind {
        E::Context(ContextError::Cancelled) => ModelErrorKind::Cancelled,
        E::Context(ContextError::Deadline) => ModelErrorKind::Deadline,
        E::Configuration => ModelErrorKind::Configuration,
        E::DeviceUnavailable => ModelErrorKind::UnsupportedExecution,
        E::Artifact(ModelArtifactErrorKind::Integrity) => ModelErrorKind::Integrity,
        E::Artifact(ModelArtifactErrorKind::Context(ContextError::Cancelled)) => {
            ModelErrorKind::Cancelled
        }
        E::Artifact(ModelArtifactErrorKind::Context(ContextError::Deadline)) => {
            ModelErrorKind::Deadline
        }
        E::Artifact(_) => ModelErrorKind::Artifact,
        E::ArtifactLimit | E::TokenLimit { .. } => ModelErrorKind::Limit,
        E::Embedding(
            graphit_ai::embedding::EmbeddingError::Limit
            | graphit_ai::embedding::EmbeddingError::Allocation,
        ) => ModelErrorKind::Limit,
        E::Embedding(graphit_ai::embedding::EmbeddingError::InvalidPolicy) => {
            ModelErrorKind::Configuration
        }
        E::Embedding(graphit_ai::embedding::EmbeddingError::InvalidRequest) => {
            ModelErrorKind::Selection
        }
        E::Embedding(graphit_ai::embedding::EmbeddingError::Context(ContextError::Cancelled)) => {
            ModelErrorKind::Cancelled
        }
        E::Embedding(graphit_ai::embedding::EmbeddingError::Context(ContextError::Deadline)) => {
            ModelErrorKind::Deadline
        }
        E::Embedding(_) | E::Output | E::Normalization => ModelErrorKind::Output,
        E::Native | E::Tokenizer => ModelErrorKind::Native,
    }
}
impl From<graphit_ai::native_embedding::NativeEmbeddingError> for ModelServiceError {
    fn from(error: graphit_ai::native_embedding::NativeEmbeddingError) -> Self {
        Self::caused(native_error_kind(error.kind()), error)
    }
}
fn gguf_rerank_error_kind(kind: graphit_ai::gguf_rerank::GgufRerankErrorKind) -> ModelErrorKind {
    use graphit_ai::{
        gguf_rerank::GgufRerankErrorKind as E, native_rerank::NativeRerankError as R,
    };
    match kind {
        E::Context(ContextError::Cancelled) | E::Input(R::Context(ContextError::Cancelled)) => {
            ModelErrorKind::Cancelled
        }
        E::Context(ContextError::Deadline) | E::Input(R::Context(ContextError::Deadline)) => {
            ModelErrorKind::Deadline
        }
        E::Configuration | E::Input(R::Policy) => ModelErrorKind::Configuration,
        E::DeviceUnavailable => ModelErrorKind::UnsupportedExecution,
        E::Input(R::Limit | R::Allocation) | E::ArtifactLimit => ModelErrorKind::Limit,
        E::Artifact(ModelArtifactErrorKind::Integrity) => ModelErrorKind::Integrity,
        E::Artifact(ModelArtifactErrorKind::Context(ContextError::Cancelled)) => {
            ModelErrorKind::Cancelled
        }
        E::Artifact(ModelArtifactErrorKind::Context(ContextError::Deadline)) => {
            ModelErrorKind::Deadline
        }
        E::Artifact(_) => ModelErrorKind::Artifact,
        E::Output => ModelErrorKind::Output,
        E::Input(_) | E::Native | E::Tokenizer => ModelErrorKind::Native,
    }
}
impl From<graphit_ai::gguf_rerank::GgufRerankError> for ModelServiceError {
    fn from(error: graphit_ai::gguf_rerank::GgufRerankError) -> Self {
        Self::caused(gguf_rerank_error_kind(error.kind()), error)
    }
}
impl From<graphit_ai::native_embedding::worker::NativeWorkerError> for ModelServiceError {
    fn from(error: graphit_ai::native_embedding::worker::NativeWorkerError) -> Self {
        use graphit_ai::native_embedding::worker::NativeWorkerErrorKind as E;
        let kind = match error.kind() {
            E::Context(ContextError::Cancelled) => ModelErrorKind::Cancelled,
            E::Context(ContextError::Deadline) => ModelErrorKind::Deadline,
            E::Busy => ModelErrorKind::Busy,
            E::Closed => ModelErrorKind::Closed,
            E::Spawn | E::Join => ModelErrorKind::Worker,
            E::Native(kind) => native_error_kind(kind),
            E::Rerank(kind) => gguf_rerank_error_kind(kind),
        };
        Self::caused(kind, error)
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;
    use graphit_ai::model_artifact::GgufModelSource;
    use graphit_auth::{AuthError, ServiceUseError, profile::ProfileError, store::AuthStoreError};
    use graphit_core::{auth_document::AuthDocumentError, runtime_key::RuntimeKeyError};

    #[test]
    fn embedding_limit_wire_preserves_only_finite_reasons_and_origin() {
        use graphit_ai::{
            embedding::EmbeddingError, native_embedding::NativeEmbeddingErrorKind as N,
        };
        for (kind, expected) in [
            (
                N::TokenLimit {
                    actual: 123456789,
                    maximum: 8192,
                },
                Some(ModelLimitReason::NativeTokenLimit),
            ),
            (N::ArtifactLimit, Some(ModelLimitReason::ArtifactLimit)),
            (
                N::Embedding(EmbeddingError::Limit),
                Some(ModelLimitReason::NativeInputLimit),
            ),
            (
                N::Embedding(EmbeddingError::Allocation),
                Some(ModelLimitReason::AllocationLimit),
            ),
            (N::Native, None),
        ] {
            assert_eq!(native_limit_reason(kind), expected);
            let wire = ModelEmbedError {
                kind: ModelErrorKind::Limit,
                limit: expected,
            };
            let encoded = serde_json::to_string(&wire).unwrap();
            assert!(!encoded.contains("123456789"));
            assert!(!encoded.contains("maximum"));
            let decoded: ModelEmbedError = serde_json::from_str(&encoded).unwrap();
            let remote = ModelServiceError::remote_embedding(decoded);
            assert_eq!(remote.kind(), ModelErrorKind::Limit);
            assert_eq!(remote.limit_reason(), expected);
            if expected.is_some() {
                assert!(format!("{remote:?}").contains("RemoteLimit"));
            }
        }
        for error in [
            ModelServiceError::new(ModelErrorKind::Limit).local_input_limit(),
            ModelServiceError::new(ModelErrorKind::Limit).local_output_limit(),
        ] {
            assert!(format!("{error:?}").contains("Local"));
            let reason = error.limit_reason();
            let wire = ModelEmbedError::from(error);
            let bytes = bincode::serialize(&wire).unwrap();
            assert_eq!(
                bincode::deserialize::<ModelEmbedError>(&bytes).unwrap(),
                wire
            );
            assert_eq!(wire.limit, reason);
        }
        let cause = io::Error::other("private-model-input-secret-canary");
        let error = ModelServiceError::caused(ModelErrorKind::Limit, cause);
        assert!(error.source().unwrap().is::<io::Error>());
        assert!(!format!("{error:?}").contains("canary"));
        let wire = ModelEmbedError::from(error);
        assert_eq!(wire.limit, None);
        assert!(!serde_json::to_string(&wire).unwrap().contains("canary"));
        let unknown = ModelServiceError::remote_embedding(wire);
        assert!(unknown.source().is_none());
        let deadline = ModelServiceError::remote_embedding(ModelEmbedError {
            kind: ModelErrorKind::Deadline,
            limit: None,
        });
        assert_eq!(
            deadline.source().unwrap().downcast_ref::<DeadlineCause>(),
            Some(&DeadlineCause::RemoteDeadlineCategory)
        );
        let invalid = ModelServiceError::remote_embedding(ModelEmbedError {
            kind: ModelErrorKind::Authentication,
            limit: Some(ModelLimitReason::NativeTokenLimit),
        });
        assert_eq!(invalid.kind(), ModelErrorKind::Transport);
    }

    #[tokio::test]
    async fn native_limit_causes_survive_direct_and_worker_preflight_without_loading() {
        let (_snapshot_root, snapshot_parent) = test_fixture::snapshot_parent();
        use graphit_ai::{
            embedding::EmbeddingLimits,
            native_embedding::{
                NativeEmbeddingClient, NativeEmbeddingInput, NativeEmbeddingOptions,
                NativeWeightPrecision,
                worker::{NativeEmbeddingWorker, NativeWorkerError},
            },
        };
        let cancellation = graphit_core::file_lock::Cancellation::default();
        let context = test_fixture::context(&cancellation);
        let options = NativeEmbeddingOptions {
            provider: graphit_ai::local_execution::LocalProvider::CPU,
            device_id: 0,
            threads: std::num::NonZeroU32::new(1).unwrap(),
            max_tokens: std::num::NonZeroU32::new(8192).unwrap(),
            dimensions: 128,
            precision: NativeWeightPrecision::Bf16,
        };
        let limits = EmbeddingLimits::new(32, 4096, 4, 4096, 8, 8192, 8192).unwrap();
        let direct = NativeEmbeddingClient::validate_request(
            options,
            limits,
            &["a"],
            NativeEmbeddingInput::Query,
            context,
        )
        .unwrap_err();
        let direct = ModelServiceError::from(direct);
        assert!(
            direct
                .source()
                .unwrap()
                .is::<graphit_ai::native_embedding::NativeEmbeddingError>()
        );
        assert_eq!(
            ModelEmbedError::from(direct).limit,
            Some(ModelLimitReason::NativeInputLimit)
        );
        let source = GgufModelSource {
            model: "embeddinggemma2".into(),
            revision: "preflight-only".into(),
            root: "private-model-input-secret-canary".into(),
            path: "absent.gguf".into(),
            minimum_bytes: 1,
            maximum_bytes: 1,
            sha256: [0; 32],
        };
        let worker = NativeEmbeddingWorker::start(
            snapshot_parent,
            source,
            options,
            limits,
            std::num::NonZeroUsize::new(1).unwrap(),
            context,
        )
        .await
        .err()
        .unwrap();
        let error = ModelServiceError::from(worker);
        assert_eq!(error.kind(), ModelErrorKind::Limit);
        assert!(error.source().unwrap().is::<NativeWorkerError>());
        let wire = ModelEmbedError::from(error);
        assert_eq!(wire.limit, Some(ModelLimitReason::NativeInputLimit));
        assert!(!serde_json::to_string(&wire).unwrap().contains("canary"));
    }

    #[test]
    fn runtime_key_interruptions_keep_category_and_original_cause() {
        for (cause, expected) in [
            (RuntimeKeyError::Cancelled, ModelErrorKind::Cancelled),
            (RuntimeKeyError::Deadline, ModelErrorKind::Deadline),
            (RuntimeKeyError::Invalid, ModelErrorKind::Authentication),
            (RuntimeKeyError::Limit, ModelErrorKind::Authentication),
            (RuntimeKeyError::Conflict, ModelErrorKind::Authentication),
            (RuntimeKeyError::Busy, ModelErrorKind::Authentication),
            (RuntimeKeyError::Io, ModelErrorKind::Authentication),
        ] {
            let error = ModelServiceError::from(cause);
            assert_eq!(error.kind(), expected);
            assert_eq!(
                error.source().unwrap().downcast_ref::<RuntimeKeyError>(),
                Some(&cause)
            );
            assert!(format!("{error:?}").contains(&format!("runtime_key: {cause:?}")));
        }
    }

    #[test]
    fn authentication_debug_exposes_only_known_typed_causes() {
        for (cause, diagnostic) in [
            (
                ServiceUseError::Auth(AuthError::Denied),
                "service_auth: Denied",
            ),
            (
                ServiceUseError::Profile(ProfileError::Stale),
                "service_profile: Stale",
            ),
            (
                ServiceUseError::Store(AuthStoreError::Core(AuthDocumentError::Conflict)),
                "service_store: Core(Conflict)",
            ),
            (ServiceUseError::WrongMode, "service: \"WrongMode\""),
        ] {
            let error = ModelServiceError::caused(ModelErrorKind::Authentication, cause);
            assert!(format!("{error:?}").contains(diagnostic));
            assert!(error.source().unwrap().is::<ServiceUseError>());
        }
        let private_cause = std::io::Error::other("private-model-input-secret-canary");
        let error = ModelServiceError::caused(ModelErrorKind::Authentication, private_cause);
        assert!(!format!("{error:?}").contains("private-model-input-secret-canary"));
        assert!(
            !error
                .to_string()
                .contains("private-model-input-secret-canary")
        );
        assert!(error.source().unwrap().is::<std::io::Error>());
        let wire = serde_json::to_string(&error.kind()).unwrap();
        assert_eq!(wire, "\"Authentication\"");
        assert!(!wire.contains("canary"));
    }

    #[test]
    fn deadline_debug_distinguishes_observed_origin_without_leaking_arbitrary_causes() {
        for (error, expected) in [
            (
                ModelServiceError::operation_deadline(),
                DeadlineCause::DeadlineLocalOperation,
            ),
            (
                ModelServiceError::remote_category(ModelErrorKind::Deadline),
                DeadlineCause::RemoteDeadlineCategory,
            ),
        ] {
            assert_eq!(error.kind(), ModelErrorKind::Deadline);
            assert_eq!(
                error.source().unwrap().downcast_ref::<DeadlineCause>(),
                Some(&expected),
            );
            assert!(format!("{error:?}").contains(&format!("deadline: {expected:?}")));
            assert_eq!(error.to_string(), "model service: Deadline");
            assert_eq!(
                serde_json::to_string(&error.kind()).unwrap(),
                "\"Deadline\""
            );
        }
        let error = ModelServiceError::caused(
            ModelErrorKind::Deadline,
            io::Error::new(
                io::ErrorKind::TimedOut,
                "private-deadline-payload-secret-canary",
            ),
        );
        let debug = format!("{error:?}");
        assert!(!debug.contains("canary"));
        assert!(!debug.contains("DeadlineLocalOperation"));
        assert!(!debug.contains("RemoteDeadlineCategory"));
        assert!(!error.to_string().contains("canary"));
        assert!(error.source().unwrap().is::<io::Error>());
        assert!(
            ModelServiceError::remote_category(ModelErrorKind::Native)
                .source()
                .is_none()
        );
    }
}
