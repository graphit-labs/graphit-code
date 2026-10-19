//! Explicit device EmbeddingGemma2 SDK client; authority and worker admission belong to consumers.
pub(crate) mod bridge;
pub mod worker;
use crate::{
    context::{ContextError, InferenceContext},
    embedding::{EmbeddingBatch, EmbeddingError, EmbeddingLimits},
    local_execution::{LocalDeviceFallback, LocalDeviceRegistration, LocalProvider},
    model_artifact::{GgufModelSource, ModelArtifactErrorKind, capture_gguf},
};
use bridge::ffi;
use graphit_core::file_lock::Cancellation;
use graphit_core::native_document_file::CapturedDocumentDirectory;
use std::{error::Error, fmt, num::NonZeroU32, time::Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeEmbeddingInput {
    Query,
    Document,
    CodeQuery,
    CodeDocument,
}
impl NativeEmbeddingInput {
    fn prefix(self) -> &'static str {
        match self {
            Self::Query => "task: search result | query: ",
            Self::CodeQuery => "task: code retrieval | query: ",
            Self::Document | Self::CodeDocument => "title: none | text: ",
        }
    }
}
/// The caller supplies the canonical device selector; discovery occurs only during load.
#[derive(Clone, Copy)]
pub struct NativeEmbeddingOptions {
    pub provider: LocalProvider,
    pub device_id: i32,
    pub threads: NonZeroU32,
    pub max_tokens: NonZeroU32,
    pub dimensions: usize,
    pub precision: NativeWeightPrecision,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeWeightPrecision {
    Bf16,
    F32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeEmbeddingErrorKind {
    Context(ContextError),
    Configuration,
    DeviceUnavailable,
    Embedding(EmbeddingError),
    Artifact(ModelArtifactErrorKind),
    ArtifactLimit,
    Native,
    Tokenizer,
    TokenLimit { actual: u32, maximum: u32 },
    Output,
    Normalization,
}
pub struct NativeEmbeddingError {
    kind: NativeEmbeddingErrorKind,
    native_status: Option<i32>,
    cause: Option<Box<dyn Error + Send + Sync>>,
}
impl NativeEmbeddingError {
    fn invalid(kind: NativeEmbeddingErrorKind) -> Self {
        Self {
            kind,
            native_status: None,
            cause: None,
        }
    }
    fn caused(kind: NativeEmbeddingErrorKind, cause: impl Error + Send + Sync + 'static) -> Self {
        Self {
            kind,
            native_status: None,
            cause: Some(Box::new(cause)),
        }
    }
    fn original(mut self, context: InferenceContext<'_>) -> Self {
        if let Err(error) = context.check() {
            self.kind = NativeEmbeddingErrorKind::Context(error);
        }
        self
    }
    pub fn kind(&self) -> NativeEmbeddingErrorKind {
        self.kind
    }
    pub fn native_status(&self) -> Option<i32> {
        self.native_status
    }
}
impl fmt::Debug for NativeEmbeddingError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("NativeEmbeddingError")
            .field("kind", &self.kind)
            .field("native_status", &self.native_status)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for NativeEmbeddingError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "native embedding {:?}", self.kind)
    }
}
impl Error for NativeEmbeddingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause
            .as_deref()
            .map(|cause| cause as &(dyn Error + 'static))
    }
}
impl From<ContextError> for NativeEmbeddingError {
    fn from(error: ContextError) -> Self {
        Self::caused(NativeEmbeddingErrorKind::Context(error), error)
    }
}
impl From<EmbeddingError> for NativeEmbeddingError {
    fn from(error: EmbeddingError) -> Self {
        Self::caused(NativeEmbeddingErrorKind::Embedding(error), error)
    }
}
type Result<T> = std::result::Result<T, NativeEmbeddingError>;

/// Keep this non-Send guard in the process bootstrap until every native model worker joins.
/// Closing it seals new owners. Existing C++ owners retain the kernel through their final drop.
pub struct NativeSdkRoot {
    _native: cxx::UniquePtr<ffi::NativeSdkRoot>,
}
impl NativeSdkRoot {
    pub fn new() -> Result<Self> {
        let native = ffi::make_sdk_root()
            .map_err(|e| NativeEmbeddingError::caused(NativeEmbeddingErrorKind::Native, e))?;
        if native.is_null() {
            return Err(NativeEmbeddingError::invalid(
                NativeEmbeddingErrorKind::Native,
            ));
        }
        Ok(Self { _native: native })
    }
}

pub(crate) struct RequestState {
    cancellation: Cancellation,
    deadline: Instant,
    #[cfg(test)]
    trigger: Option<tests::Trigger>,
}
impl RequestState {
    pub(crate) fn new(context: InferenceContext<'_>) -> Self {
        Self {
            cancellation: context.cancellation().clone(),
            deadline: context.deadline(),
            #[cfg(test)]
            trigger: None,
        }
    }
    #[cfg(test)]
    pub(crate) fn cancel_after_native_callbacks(
        context: InferenceContext<'_>,
        after: usize,
    ) -> Self {
        let mut request = Self::new(context);
        request.trigger = Some(tests::Trigger::new(2, after, false));
        request
    }
    pub(crate) fn context(&self) -> InferenceContext<'_> {
        InferenceContext::new(&self.cancellation, self.deadline)
    }
}
fn request_check(request: &RequestState, _phase: u8) -> bool {
    #[cfg(test)]
    if let Some(trigger) = &request.trigger {
        trigger.check(request, _phase);
    }
    request.context().check().is_ok()
}

/// The native model/context and its verified file mappings stay on their creating thread.
pub struct NativeEmbeddingClient {
    native: cxx::UniquePtr<ffi::NativeOwner>,
    registration: LocalDeviceRegistration,
    model: String,
    revision: String,
    options: NativeEmbeddingOptions,
    limits: EmbeddingLimits,
    // Fields drop in order: native mappings finish before this directory is removed.
    _verified_bundle: tempfile::TempDir,
}
impl NativeEmbeddingClient {
    pub fn load(
        snapshot_parent: &CapturedDocumentDirectory,
        source: GgufModelSource,
        options: NativeEmbeddingOptions,
        limits: EmbeddingLimits,
        context: InferenceContext<'_>,
    ) -> Result<Self> {
        Self::load_with_state(
            snapshot_parent,
            source,
            options,
            limits,
            &RequestState::new(context),
        )
    }
    fn load_with_state(
        snapshot_parent: &CapturedDocumentDirectory,
        source: GgufModelSource,
        options: NativeEmbeddingOptions,
        limits: EmbeddingLimits,
        request: &RequestState,
    ) -> Result<Self> {
        let context = request.context();
        context.check()?;
        Self::validate_options(options, limits)?;
        let captured = capture_gguf(snapshot_parent, source, context).map_err(|error| {
            let kind = match error.kind() {
                ModelArtifactErrorKind::Context(cause) => NativeEmbeddingErrorKind::Context(cause),
                ModelArtifactErrorKind::Configuration => NativeEmbeddingErrorKind::Configuration,
                ModelArtifactErrorKind::Limit => NativeEmbeddingErrorKind::ArtifactLimit,
                kind => NativeEmbeddingErrorKind::Artifact(kind),
            };
            NativeEmbeddingError::caused(kind, error).original(context)
        })?;
        let path = captured.bundle.path().join("model.gguf");
        let path = path.to_str().ok_or_else(|| {
            NativeEmbeddingError::invalid(NativeEmbeddingErrorKind::Configuration)
        })?;
        let native = Self::load_native(path, options, request)?;
        context.check()?;
        let info = native.registered_device();
        let provider = match info.provider {
            0 => LocalProvider::CPU,
            1 => LocalProvider::CUDA,
            2 => LocalProvider::Metal,
            _ => {
                return Err(NativeEmbeddingError::invalid(
                    NativeEmbeddingErrorKind::Output,
                ));
            }
        };
        let registration = LocalDeviceRegistration {
            requested_provider: options.provider,
            requested_device_id: options.device_id,
            provider,
            device_id: info.device_id as i32,
            fallback_reason: (options.provider == LocalProvider::Auto
                && provider == LocalProvider::CPU)
                .then_some(LocalDeviceFallback::AcceleratorUnavailable),
        };
        if !registration.matches_request(options.provider, options.device_id) {
            return Err(NativeEmbeddingError::invalid(
                NativeEmbeddingErrorKind::Output,
            ));
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
    fn load_native(
        path: &str,
        options: NativeEmbeddingOptions,
        request: &RequestState,
    ) -> Result<cxx::UniquePtr<ffi::NativeOwner>> {
        let context = request.context();
        context.check()?;
        let precision = match options.precision {
            NativeWeightPrecision::Bf16 => 0,
            NativeWeightPrecision::F32 => 1,
        };
        let provider = match options.provider {
            LocalProvider::CPU => 0,
            LocalProvider::CUDA => 1,
            LocalProvider::Metal => 2,
            LocalProvider::Auto => 3,
        };
        let native = ffi::make_owner(
            path,
            request,
            options.threads.get(),
            precision,
            provider,
            options.device_id as u32,
            0,
        )
        .map_err(|e| {
            let kind = if e.what() == "native_device_unavailable" {
                NativeEmbeddingErrorKind::DeviceUnavailable
            } else {
                NativeEmbeddingErrorKind::Native
            };
            NativeEmbeddingError::caused(kind, e).original(context)
        })?;
        if native.is_null() {
            return Err(
                NativeEmbeddingError::invalid(NativeEmbeddingErrorKind::Native).original(context),
            );
        }
        context.check()?;
        Ok(native)
    }
    fn validate_options(options: NativeEmbeddingOptions, limits: EmbeddingLimits) -> Result<()> {
        if options.provider == LocalProvider::Metal && !cfg!(target_os = "macos") {
            return Err(NativeEmbeddingError::invalid(
                NativeEmbeddingErrorKind::DeviceUnavailable,
            ));
        }
        if options.device_id < 0
            || (matches!(options.provider, LocalProvider::CPU | LocalProvider::Metal)
                && options.device_id != 0)
            || options.threads.get() > i32::MAX as u32
            || options.max_tokens.get() > 8192
            || ![128, 256, 512, 768].contains(&options.dimensions)
        {
            return Err(NativeEmbeddingError::invalid(
                NativeEmbeddingErrorKind::Configuration,
            ));
        }
        if options.dimensions > limits.max_dimensions {
            return Err(EmbeddingError::Limit.into());
        }
        Ok(())
    }
    /// Whole-request preflight is available before a consumer admits or loads a worker.
    pub fn validate_request(
        options: NativeEmbeddingOptions,
        limits: EmbeddingLimits,
        texts: &[&str],
        input: NativeEmbeddingInput,
        context: InferenceContext<'_>,
    ) -> Result<()> {
        context.check()?;
        Self::validate_options(options, limits)?;
        limits.validate_input(texts, options.dimensions, context)?;
        let mut prepared_bytes = 0usize;
        for text in texts {
            context.check()?;
            if text.is_empty() {
                return Err(EmbeddingError::InvalidRequest.into());
            }
            let bytes = text
                .len()
                .checked_add(input.prefix().len())
                .filter(|n| *n <= i32::MAX as usize)
                .ok_or(EmbeddingError::Limit)?;
            prepared_bytes = prepared_bytes
                .checked_add(bytes)
                .ok_or(EmbeddingError::Limit)?;
            if prepared_bytes > limits.max_input_bytes {
                return Err(EmbeddingError::Limit.into());
            }
        }
        context.check()?;
        Ok(())
    }
    pub fn registered_device(&self) -> LocalDeviceRegistration {
        self.registration
    }
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn revision(&self) -> &str {
        &self.revision
    }
    pub fn dimensions(&self) -> usize {
        self.options.dimensions
    }
    pub fn embed(
        &mut self,
        texts: &[&str],
        input: NativeEmbeddingInput,
        context: InferenceContext<'_>,
    ) -> Result<EmbeddingBatch<'_>> {
        self.embed_with_state(texts, input, &RequestState::new(context))
    }
    fn embed_with_state(
        &mut self,
        texts: &[&str],
        input: NativeEmbeddingInput,
        request: &RequestState,
    ) -> Result<EmbeddingBatch<'_>> {
        let context = request.context();
        Self::validate_request(self.options, self.limits, texts, input, context)?;
        let mut vectors = Vec::new();
        vectors
            .try_reserve_exact(texts.len())
            .map_err(|_| EmbeddingError::Allocation)?;
        for text in texts {
            context.check()?;
            let mut prepared = String::new();
            prepared
                .try_reserve_exact(input.prefix().len() + text.len())
                .map_err(|_| EmbeddingError::Allocation)?;
            prepared.push_str(input.prefix());
            prepared.push_str(text);
            vectors.push(self.run_one(&prepared, input, request)?);
        }
        context.check()?;
        Ok(EmbeddingBatch::new(
            &self.model,
            self.options.dimensions,
            vectors,
        ))
    }
    fn run_one(
        &mut self,
        prepared: &str,
        input: NativeEmbeddingInput,
        request: &RequestState,
    ) -> Result<Vec<f32>> {
        let context = request.context();
        context.check()?;
        let result = self
            .native
            .pin_mut()
            .embed(
                prepared,
                request,
                self.options.max_tokens.get(),
                match input {
                    NativeEmbeddingInput::Document | NativeEmbeddingInput::CodeDocument => {
                        input.prefix().len() as u32
                    }
                    NativeEmbeddingInput::Query | NativeEmbeddingInput::CodeQuery => 0,
                },
            )
            .map_err(|e| {
                NativeEmbeddingError::caused(NativeEmbeddingErrorKind::Native, e).original(context)
            })?;
        #[cfg(test)]
        if let Some(trigger) = &request.trigger {
            trigger.record_tokens(result.token_count);
        }
        if result.kind != 0 || result.native_status != 0 {
            let kind = match result.kind {
                3 => NativeEmbeddingErrorKind::Tokenizer,
                4 => NativeEmbeddingErrorKind::TokenLimit {
                    actual: result.token_count,
                    maximum: self.options.max_tokens.get(),
                },
                9 => NativeEmbeddingErrorKind::Output,
                10 => NativeEmbeddingErrorKind::Normalization,
                _ => NativeEmbeddingErrorKind::Native,
            };
            return Err(NativeEmbeddingError {
                kind,
                native_status: result.processed.then_some(result.native_status),
                cause: Some(Box::new(NativeFailure {
                    kind: result.kind,
                    status: result.processed.then_some(result.native_status),
                })),
            }
            .original(context));
        }
        context.check()?;
        if result.values.len() != 768 || result.values.iter().any(|x| !x.is_finite()) {
            return Err(NativeEmbeddingError::invalid(
                NativeEmbeddingErrorKind::Output,
            ));
        }
        let mut values = result.values;
        if self.options.dimensions != 768 {
            values.truncate(self.options.dimensions);
            let norm = values.iter().map(|x| x * x).sum::<f32>().sqrt();
            if !norm.is_finite() || norm <= 0.0 {
                return Err(NativeEmbeddingError::invalid(
                    NativeEmbeddingErrorKind::Normalization,
                ));
            }
            for value in &mut values {
                *value /= norm;
            }
        }
        request_check(request, 4);
        context.check()?;
        Ok(values)
    }
}
#[derive(Debug)]
struct NativeFailure {
    kind: u8,
    status: Option<i32>,
}
impl fmt::Display for NativeFailure {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "native result kind={} status={:?}",
            self.kind, self.status
        )
    }
}
impl Error for NativeFailure {}
#[cfg(test)]
mod tests;
