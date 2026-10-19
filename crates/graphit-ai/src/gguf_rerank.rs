//! Paired BGE reranking through the pinned local GGUF classification head.
use crate::{
    context::{ContextError, InferenceContext},
    local_execution::{LocalDeviceFallback, LocalDeviceRegistration, LocalProvider},
    model_artifact::{GgufModelSource, ModelArtifactErrorKind, capture_gguf},
    model_manifest::{BgeRerankRecipe, RerankPrecision, RerankScoreTransform},
    native_embedding::{RequestState, bridge::ffi},
    native_rerank::{NativeRerankError, NativeRerankLimits},
};
use graphit_core::native_document_file::CapturedDocumentDirectory;
use std::{error::Error, fmt, num::NonZeroU32};

#[derive(Clone, Copy)]
pub struct GgufRerankOptions {
    pub threads: NonZeroU32,
    pub max_tokens: NonZeroU32,
    pub provider: LocalProvider,
    pub device_id: i32,
    pub precision: RerankPrecision,
    pub score: RerankScoreTransform,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GgufRerankErrorKind {
    Context(ContextError),
    Configuration,
    DeviceUnavailable,
    Input(NativeRerankError),
    Artifact(ModelArtifactErrorKind),
    ArtifactLimit,
    Native,
    Tokenizer,
    Output,
}
pub struct GgufRerankError {
    kind: GgufRerankErrorKind,
    native_status: Option<i32>,
    cause: Option<Box<dyn Error + Send + Sync>>,
}
impl GgufRerankError {
    fn invalid(kind: GgufRerankErrorKind) -> Self {
        Self {
            kind,
            native_status: None,
            cause: None,
        }
    }
    fn caused(kind: GgufRerankErrorKind, cause: impl Error + Send + Sync + 'static) -> Self {
        Self {
            kind,
            native_status: None,
            cause: Some(Box::new(cause)),
        }
    }
    fn original(mut self, context: InferenceContext<'_>) -> Self {
        if let Err(cause) = context.check() {
            self.kind = GgufRerankErrorKind::Context(cause);
        }
        self
    }
    pub fn kind(&self) -> GgufRerankErrorKind {
        self.kind
    }
    pub fn native_status(&self) -> Option<i32> {
        self.native_status
    }
}
impl fmt::Debug for GgufRerankError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("GgufRerankError")
            .field("kind", &self.kind)
            .field("native_status", &self.native_status)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for GgufRerankError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "GGUF rerank {:?}", self.kind)
    }
}
impl Error for GgufRerankError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause.as_deref().map(|e| e as _)
    }
}
impl From<ContextError> for GgufRerankError {
    fn from(error: ContextError) -> Self {
        Self::caused(GgufRerankErrorKind::Context(error), error)
    }
}
impl From<NativeRerankError> for GgufRerankError {
    fn from(error: NativeRerankError) -> Self {
        let kind = match error {
            NativeRerankError::Context(cause) => GgufRerankErrorKind::Context(cause),
            other => GgufRerankErrorKind::Input(other),
        };
        Self::caused(kind, error)
    }
}
type Result<T> = std::result::Result<T, GgufRerankError>;
#[derive(Debug)]
struct NativeFailure {
    kind: u8,
    status: Option<i32>,
}
impl fmt::Display for NativeFailure {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "native rerank kind={} status={:?}",
            self.kind, self.status
        )
    }
}
impl Error for NativeFailure {}

/// A non-Send thread-local model; the SDK bootstrap outlives this owner's final drop.
pub struct GgufRerankClient {
    native: cxx::UniquePtr<ffi::NativeOwner>,
    registration: LocalDeviceRegistration,
    model: String,
    revision: String,
    options: GgufRerankOptions,
    limits: NativeRerankLimits,
    // Native mappings finish before the verified bytes are removed.
    _verified_bundle: tempfile::TempDir,
}
impl GgufRerankClient {
    pub fn validate_options(options: GgufRerankOptions) -> Result<()> {
        if options.provider == LocalProvider::Metal && !cfg!(target_os = "macos") {
            return Err(GgufRerankError::invalid(
                GgufRerankErrorKind::DeviceUnavailable,
            ));
        }
        if options.threads.get() > i32::MAX as u32
            || options.max_tokens.get() < 4
            || u64::from(options.max_tokens.get()) > BgeRerankRecipe::MAX_TOKENS
            || options.device_id < 0
            || (matches!(options.provider, LocalProvider::CPU | LocalProvider::Metal)
                && options.device_id != 0)
        {
            return Err(GgufRerankError::invalid(GgufRerankErrorKind::Configuration));
        }
        Ok(())
    }
    pub fn validate_request(
        options: GgufRerankOptions,
        limits: NativeRerankLimits,
        query: &str,
        documents: &[&str],
        context: InferenceContext<'_>,
    ) -> Result<()> {
        context.check()?;
        Self::validate_options(options)?;
        limits.validate_input(query, documents, context)?;
        if query.len() > i32::MAX as usize
            || documents.iter().any(|doc| doc.len() > i32::MAX as usize)
        {
            return Err(NativeRerankError::Limit.into());
        }
        context.check()?;
        Ok(())
    }
    pub fn load(
        snapshot_parent: &CapturedDocumentDirectory,
        source: GgufModelSource,
        options: GgufRerankOptions,
        limits: NativeRerankLimits,
        context: InferenceContext<'_>,
    ) -> Result<Self> {
        context.check()?;
        Self::validate_request(options, limits, "", &[], context)?;
        let captured = capture_gguf(snapshot_parent, source, context).map_err(|cause| {
            let kind = match cause.kind() {
                ModelArtifactErrorKind::Context(error) => GgufRerankErrorKind::Context(error),
                ModelArtifactErrorKind::Configuration => GgufRerankErrorKind::Configuration,
                ModelArtifactErrorKind::Limit => GgufRerankErrorKind::ArtifactLimit,
                kind => GgufRerankErrorKind::Artifact(kind),
            };
            GgufRerankError::caused(kind, cause).original(context)
        })?;
        let path = captured.bundle.path().join("model.gguf");
        let path = path
            .to_str()
            .ok_or_else(|| GgufRerankError::invalid(GgufRerankErrorKind::Configuration))?;
        let provider = match options.provider {
            LocalProvider::CPU => 0,
            LocalProvider::CUDA => 1,
            LocalProvider::Metal => 2,
            LocalProvider::Auto => 3,
        };
        let precision = match options.precision {
            RerankPrecision::F16 => 2,
        };
        let native = ffi::make_owner(
            path,
            &RequestState::new(context),
            options.threads.get(),
            precision,
            provider,
            options.device_id as u32,
            1,
        )
        .map_err(|cause| {
            let kind = if cause.what() == "native_device_unavailable" {
                GgufRerankErrorKind::DeviceUnavailable
            } else {
                GgufRerankErrorKind::Native
            };
            GgufRerankError::caused(kind, cause).original(context)
        })?;
        if native.is_null() {
            return Err(GgufRerankError::invalid(GgufRerankErrorKind::Native).original(context));
        }
        context.check()?;
        let info = native.registered_device();
        let actual = match info.provider {
            0 => LocalProvider::CPU,
            1 => LocalProvider::CUDA,
            2 => LocalProvider::Metal,
            _ => return Err(GgufRerankError::invalid(GgufRerankErrorKind::Output)),
        };
        let registration = LocalDeviceRegistration {
            requested_provider: options.provider,
            requested_device_id: options.device_id,
            provider: actual,
            device_id: info.device_id as i32,
            fallback_reason: (options.provider == LocalProvider::Auto
                && actual == LocalProvider::CPU)
                .then_some(LocalDeviceFallback::AcceleratorUnavailable),
        };
        if !registration.matches_request(options.provider, options.device_id) {
            return Err(GgufRerankError::invalid(GgufRerankErrorKind::Output));
        }
        Ok(Self {
            native,
            registration,
            model: captured.model,
            revision: captured.revision,
            options,
            limits,
            _verified_bundle: captured.bundle,
        })
    }
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn revision(&self) -> &str {
        &self.revision
    }
    pub fn registered_device(&self) -> LocalDeviceRegistration {
        self.registration
    }
    #[cfg(test)]
    pub(crate) fn verified_bundle(&self) -> &std::path::Path {
        self._verified_bundle.path()
    }
    pub fn score(
        &mut self,
        query: &str,
        documents: &[&str],
        context: InferenceContext<'_>,
    ) -> Result<Vec<f64>> {
        self.score_with_state(query, documents, &RequestState::new(context))
    }
    pub(crate) fn score_with_state(
        &mut self,
        query: &str,
        documents: &[&str],
        request: &RequestState,
    ) -> Result<Vec<f64>> {
        let context = request.context();
        Self::validate_request(self.options, self.limits, query, documents, context)?;
        let mut scores = Vec::new();
        scores
            .try_reserve_exact(documents.len())
            .map_err(|_| GgufRerankError::from(NativeRerankError::Allocation))?;
        for document in documents {
            context.check()?;
            let pair = self
                .native
                .pin_mut()
                .tokenize_pair(query, document, request, self.options.max_tokens.get())
                .map_err(|cause| {
                    GgufRerankError::caused(GgufRerankErrorKind::Native, cause).original(context)
                })?;
            if pair.kind != 0 {
                let kind = match pair.kind {
                    2 => GgufRerankErrorKind::Input(NativeRerankError::Limit),
                    3 => GgufRerankErrorKind::Tokenizer,
                    _ => GgufRerankErrorKind::Native,
                };
                return Err(GgufRerankError::caused(
                    kind,
                    NativeFailure {
                        kind: pair.kind,
                        status: None,
                    },
                )
                .original(context));
            }
            context.check()?;
            let result = self
                .native
                .pin_mut()
                .rerank_tokens(&pair.ids, request)
                .map_err(|cause| {
                    GgufRerankError::caused(GgufRerankErrorKind::Native, cause).original(context)
                })?;
            if result.kind != 0 || result.native_status != 0 {
                let kind = match result.kind {
                    2 => GgufRerankErrorKind::Input(NativeRerankError::Limit),
                    3 => GgufRerankErrorKind::Tokenizer,
                    9 => GgufRerankErrorKind::Output,
                    _ => GgufRerankErrorKind::Native,
                };
                let status = result.processed.then_some(result.native_status);
                return Err(GgufRerankError {
                    kind,
                    native_status: status,
                    cause: Some(Box::new(NativeFailure {
                        kind: result.kind,
                        status,
                    })),
                }
                .original(context));
            }
            context.check()?;
            if !result.score.is_finite()
                || result.token_count as usize != pair.ids.len()
                || result.context_tokens < result.token_count
            {
                return Err(GgufRerankError::invalid(GgufRerankErrorKind::Output));
            }
            let raw = f64::from(result.score);
            let score = match self.options.score {
                RerankScoreTransform::RawLogit => raw,
                RerankScoreTransform::Sigmoid if raw >= 0.0 => 1.0 / (1.0 + (-raw).exp()),
                RerankScoreTransform::Sigmoid => {
                    let exp = raw.exp();
                    exp / (1.0 + exp)
                }
            };
            if !score.is_finite() {
                return Err(GgufRerankError::invalid(GgufRerankErrorKind::Output));
            }
            scores.push(score);
        }
        context.check()?;
        Ok(scores)
    }
}

#[cfg(test)]
#[path = "gguf_rerank_tests.rs"]
mod tests;
