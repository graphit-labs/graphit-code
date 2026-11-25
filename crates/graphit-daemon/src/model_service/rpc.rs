use super::{
    ConfigId, ModelErrorKind, ModelLimitReason, ModelResult, ModelService, ModelServiceError,
};
use crate::model_child::GlobalModelConfig;
use bincode::Options;
use futures::{Sink, Stream, TryStreamExt};
use graphit_ai::context::InferenceContext;
use graphit_core::artifact_store::SnapshotContext;
use graphit_core::file_lock::Cancellation;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::BTreeMap, fmt, io, sync::Arc};
use subtle::ConstantTimeEq;
use tarpc::{ClientMessage, Response, server::Channel};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::LengthDelimitedCodec;
use zeroize::Zeroizing;

pub struct RuntimeSecret(Zeroizing<[u8; 64]>);
impl RuntimeSecret {
    pub fn new(value: Zeroizing<[u8; 64]>) -> ModelResult<Self> {
        validate_secret(&value[..])?;
        Ok(Self(value))
    }
    fn accepts(&self, value: &RuntimeCredential) -> bool {
        value.0.len() == 64 && bool::from(self.0[..].ct_eq(&value.0[..]))
    }
}
impl fmt::Debug for RuntimeSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RuntimeSecret(<redacted>)")
    }
}
pub struct RuntimeCredential(Zeroizing<Vec<u8>>);
fn validate_secret(value: &[u8]) -> ModelResult<()> {
    if value.len() != 64
        || !value
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    {
        return Err(ModelServiceError::new(ModelErrorKind::Authentication));
    }
    Ok(())
}
impl RuntimeCredential {
    pub fn from_secret(value: &[u8; 64]) -> ModelResult<Self> {
        validate_secret(value)?;
        Ok(Self(Zeroizing::new(value.to_vec())))
    }
}
impl fmt::Debug for RuntimeCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RuntimeCredential(<redacted>)")
    }
}
impl Serialize for RuntimeCredential {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_slice().serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for RuntimeCredential {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Zeroizing::new(Vec::<u8>::deserialize(deserializer)?);
        validate_secret(&value)
            .map_err(|_| serde::de::Error::custom("invalid runtime credential"))?;
        Ok(Self(value))
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum EmbedInput {
    Query,
    Document,
    CodeQuery,
    CodeDocument,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct EmbedOutput {
    pub config_id: ConfigId,
    pub device: graphit_ai::local_execution::LocalDeviceRegistration,
    pub generation: u64,
    pub model: String,
    pub revision: String,
    pub dimensions: u64,
    pub vectors: Vec<Vec<f32>>,
}
/// Only finite diagnostics cross this private embedding RPC boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelEmbedError {
    pub kind: ModelErrorKind,
    pub limit: Option<ModelLimitReason>,
}
impl From<ModelServiceError> for ModelEmbedError {
    fn from(error: ModelServiceError) -> Self {
        Self {
            kind: error.kind(),
            limit: error.limit_reason(),
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct RerankOutput {
    pub config_id: ConfigId,
    pub device: graphit_ai::local_execution::LocalDeviceRegistration,
    pub generation: u64,
    pub model: String,
    pub revision: String,
    pub scores: Vec<f64>,
}
/// Acknowledges the canonical map of demanded slots, including an empty retired map.
#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshOutput {
    pub config_ids: BTreeMap<String, ConfigId>,
    pub generation: u64,
}
#[derive(Clone, Copy)]
pub struct RpcLimits {
    pub max_frame_bytes: usize,
    pub max_texts: usize,
    pub max_input_bytes: usize,
    pub max_output_values: usize,
}
impl RpcLimits {
    pub fn new(frame: usize, texts: usize, input: usize, output: usize) -> ModelResult<Self> {
        if [frame, texts, input, output]
            .iter()
            .any(|n| *n == 0 || *n > isize::MAX as usize)
            || frame > u32::MAX as usize
        {
            return Err(ModelServiceError::new(ModelErrorKind::Limit));
        }
        Ok(Self {
            max_frame_bytes: frame,
            max_texts: texts,
            max_input_bytes: input,
            max_output_values: output,
        })
    }
    pub(crate) fn input(self, query: Option<&str>, texts: &[String]) -> ModelResult<()> {
        if texts.len() > self.max_texts {
            return Err(ModelServiceError::new(ModelErrorKind::Limit));
        }
        let mut bytes = query.map_or(0, str::len);
        for text in texts {
            bytes = bytes
                .checked_add(text.len())
                .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Limit))?;
        }
        if bytes > self.max_input_bytes {
            return Err(ModelServiceError::new(ModelErrorKind::Limit));
        }
        Ok(())
    }
    pub(crate) fn embedding(self, out: &EmbedOutput, count: usize) -> ModelResult<()> {
        let dimensions = usize::try_from(out.dimensions)
            .map_err(|_| ModelServiceError::new(ModelErrorKind::Output))?;
        if dimensions == 0
            || out.vectors.len() != count
            || out
                .vectors
                .iter()
                .any(|v| v.len() != dimensions || v.iter().any(|x| !x.is_finite()))
        {
            return Err(ModelServiceError::new(ModelErrorKind::Output));
        }
        if count
            .checked_mul(dimensions)
            .is_none_or(|n| n > self.max_output_values)
        {
            return Err(ModelServiceError::new(ModelErrorKind::Limit));
        }
        Ok(())
    }
    pub(crate) fn rerank(self, out: &RerankOutput, count: usize) -> ModelResult<()> {
        if out.scores.len() != count || out.scores.iter().any(|x| !x.is_finite()) {
            return Err(ModelServiceError::new(ModelErrorKind::Output));
        }
        if count > self.max_output_values {
            return Err(ModelServiceError::new(ModelErrorKind::Limit));
        }
        Ok(())
    }
}
#[tarpc::service]
pub trait ModelRpc {
    async fn embed(
        credential: RuntimeCredential,
        slot: String,
        expected_config_id: ConfigId,
        input: EmbedInput,
        texts: Vec<String>,
    ) -> Result<EmbedOutput, ModelEmbedError>;
    async fn rerank(
        credential: RuntimeCredential,
        slot: String,
        expected_config_id: ConfigId,
        query: String,
        documents: Vec<String>,
    ) -> Result<RerankOutput, ModelErrorKind>;
    async fn refresh(
        credential: RuntimeCredential,
        expected_config_ids: BTreeMap<String, ConfigId>,
    ) -> Result<RefreshOutput, ModelErrorKind>;
    async fn shutdown(credential: RuntimeCredential) -> Result<(), ModelErrorKind>;
}

pub fn server_transport<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    limits: RpcLimits,
) -> ModelResult<
    impl Stream<Item = io::Result<ClientMessage<ModelRpcRequest>>>
    + Sink<Response<ModelRpcResponse>, Error = io::Error>
    + Unpin,
> {
    let limits = RpcLimits::new(
        limits.max_frame_bytes,
        limits.max_texts,
        limits.max_input_bytes,
        limits.max_output_values,
    )?;
    let framed = LengthDelimitedCodec::builder()
        .max_frame_length(limits.max_frame_bytes)
        .new_framed(stream);
    let codec = tarpc::tokio_serde::formats::Bincode::from(
        bincode::DefaultOptions::new()
            .with_limit(limits.max_frame_bytes as u64)
            .reject_trailing_bytes(),
    );
    let transport: tarpc::serde_transport::Transport<
        S,
        ClientMessage<ModelRpcRequest>,
        Response<ModelRpcResponse>,
        _,
    > = tarpc::serde_transport::new(framed, codec);
    Ok(transport)
}
pub fn client_transport<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    limits: RpcLimits,
) -> ModelResult<
    impl Stream<Item = io::Result<Response<ModelRpcResponse>>>
    + Sink<ClientMessage<ModelRpcRequest>, Error = io::Error>
    + Unpin,
> {
    let limits = RpcLimits::new(
        limits.max_frame_bytes,
        limits.max_texts,
        limits.max_input_bytes,
        limits.max_output_values,
    )?;
    let framed = LengthDelimitedCodec::builder()
        .max_frame_length(limits.max_frame_bytes)
        .new_framed(stream);
    let codec = tarpc::tokio_serde::formats::Bincode::from(
        bincode::DefaultOptions::new()
            .with_limit(limits.max_frame_bytes as u64)
            .reject_trailing_bytes(),
    );
    let transport: tarpc::serde_transport::Transport<
        S,
        Response<ModelRpcResponse>,
        ClientMessage<ModelRpcRequest>,
        _,
    > = tarpc::serde_transport::new(framed, codec);
    Ok(transport)
}
#[derive(Clone)]
struct RpcServer {
    service: ModelService,
    secret: Arc<RuntimeSecret>,
    limits: RpcLimits,
    global: Arc<GlobalModelConfig>,
}
struct RequestCancellation(Cancellation);
impl Drop for RequestCancellation {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
impl ModelRpc for RpcServer {
    async fn embed(
        self,
        ctx: tarpc::context::Context,
        credential: RuntimeCredential,
        slot: String,
        expected_config_id: ConfigId,
        input: EmbedInput,
        texts: Vec<String>,
    ) -> Result<EmbedOutput, ModelEmbedError> {
        if !self.secret.accepts(&credential) {
            return Err(ModelServiceError::new(ModelErrorKind::Authentication).into());
        }
        drop(credential);
        let guard = RequestCancellation(Cancellation::default());
        let context = InferenceContext::new(&guard.0, ctx.deadline);
        let count = texts.len();
        self.limits
            .input(None, &texts)
            .map_err(ModelServiceError::local_input_limit)
            .map_err(ModelEmbedError::from)?;
        if !self
            .service
            .has_model(
                &slot,
                expected_config_id,
                graphit_ai::model_manifest::ModelTask::Embedding,
                context,
            )
            .map_err(ModelEmbedError::from)?
        {
            let original = Arc::new(SnapshotContext::new(ctx.deadline, guard.0.clone()));
            self.global
                .ensure_model(
                    self.service.clone(),
                    slot.clone(),
                    expected_config_id,
                    graphit_ai::model_manifest::ModelTask::Embedding,
                    original,
                )
                .await
                .map_err(ModelEmbedError::from)?;
        }
        let output = self
            .service
            .embed(&slot, expected_config_id, input, texts, context)
            .await
            .map_err(ModelEmbedError::from)?;
        self.limits
            .embedding(&output, count)
            .map_err(ModelServiceError::local_output_limit)
            .map_err(ModelEmbedError::from)?;
        context
            .check()
            .map_err(ModelServiceError::from)
            .map_err(ModelEmbedError::from)?;
        Ok(output)
    }
    async fn rerank(
        self,
        ctx: tarpc::context::Context,
        credential: RuntimeCredential,
        slot: String,
        expected_config_id: ConfigId,
        query: String,
        documents: Vec<String>,
    ) -> Result<RerankOutput, ModelErrorKind> {
        if !self.secret.accepts(&credential) {
            return Err(ModelErrorKind::Authentication);
        }
        drop(credential);
        let guard = RequestCancellation(Cancellation::default());
        let context = InferenceContext::new(&guard.0, ctx.deadline);
        let count = documents.len();
        self.limits
            .input(Some(&query), &documents)
            .map_err(|e| e.kind())?;
        self.service
            .preflight_rerank(&query, &documents, context)
            .map_err(|e| e.kind())?;
        if !self
            .service
            .has_model(
                &slot,
                expected_config_id,
                graphit_ai::model_manifest::ModelTask::Rerank,
                context,
            )
            .map_err(|e| e.kind())?
        {
            let original = Arc::new(SnapshotContext::new(ctx.deadline, guard.0.clone()));
            self.global
                .ensure_model(
                    self.service.clone(),
                    slot.clone(),
                    expected_config_id,
                    graphit_ai::model_manifest::ModelTask::Rerank,
                    original,
                )
                .await
                .map_err(|e| e.kind())?;
        }
        let output = self
            .service
            .rerank(&slot, expected_config_id, query, documents, context)
            .await
            .map_err(|e| e.kind())?;
        self.limits.rerank(&output, count).map_err(|e| e.kind())?;
        context
            .check()
            .map_err(|e| ModelServiceError::from(e).kind())?;
        Ok(output)
    }
    async fn refresh(
        self,
        ctx: tarpc::context::Context,
        credential: RuntimeCredential,
        expected_config_ids: BTreeMap<String, ConfigId>,
    ) -> Result<RefreshOutput, ModelErrorKind> {
        if !self.secret.accepts(&credential) {
            return Err(ModelErrorKind::Authentication);
        }
        drop(credential);
        let guard = RequestCancellation(Cancellation::default());
        let original = Arc::new(SnapshotContext::new(ctx.deadline, guard.0.clone()));
        self.global
            .refresh(self.service, expected_config_ids, original)
            .await
            .map_err(|error| error.kind())
    }
    async fn shutdown(
        self,
        ctx: tarpc::context::Context,
        credential: RuntimeCredential,
    ) -> Result<(), ModelErrorKind> {
        if !self.secret.accepts(&credential) {
            return Err(ModelErrorKind::Authentication);
        }
        drop(credential);
        let guard = RequestCancellation(Cancellation::default());
        self.service
            .shutdown(InferenceContext::new(&guard.0, ctx.deadline))
            .await
            .map_err(|e| e.kind())
    }
}
/// E must signal this exact token on transport EOF/error/Drop. EOF by itself
/// does not guarantee cancellation of tarpc's pending handler futures.
pub async fn serve_connection<T>(
    transport: T,
    service: ModelService,
    secret: Arc<RuntimeSecret>,
    limits: RpcLimits,
    global: Arc<GlobalModelConfig>,
    connection: Cancellation,
) -> ModelResult<()>
where
    T: Stream<Item = io::Result<ClientMessage<ModelRpcRequest>>>
        + Sink<Response<ModelRpcResponse>, Error = io::Error>
        + Unpin,
{
    let limits = RpcLimits::new(
        limits.max_frame_bytes,
        limits.max_texts,
        limits.max_input_bytes,
        limits.max_output_values,
    )?;
    let config = tarpc::server::Config {
        pending_response_buffer: 1,
    };
    let channel = tarpc::server::BaseChannel::new(config, transport).max_concurrent_requests(1);
    let server = RpcServer {
        service,
        secret,
        limits,
        global,
    };
    let work = channel
        .requests()
        .try_for_each_concurrent(None, move |request| {
            let server = server.clone();
            async move {
                request.execute(server.serve()).await;
                Ok(())
            }
        });
    tokio::select! {
        biased;
        result = work => result.map_err(|error|
            ModelServiceError::caused(ModelErrorKind::Transport,error)),
        _ = connection.cancelled() => Err(ModelServiceError::new(ModelErrorKind::Cancelled)),
    }
}

#[cfg(test)]
#[path = "reload_tests.rs"]
mod reload_tests;
