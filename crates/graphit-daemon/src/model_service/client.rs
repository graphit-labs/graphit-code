use super::config::ResolvedExecution;
use super::{
    ConfigId, EmbedInput, EmbedOutput, ModelErrorKind, ModelResult, ModelRpcClient,
    ModelServiceError, RefreshOutput, RerankOutput, ResolvedConfig, RpcLimits, RuntimeCredential,
    client_transport,
};
use futures::future::poll_fn;
use graphit_ai::{local_execution::LocalDeviceRegistration, model_manifest::ModelTask};
use graphit_auth::{
    LocalServiceLease, ServiceBindingView, ServicePurpose, ServiceUseError,
    profile::{LocalExecution, ProfileError},
};
use graphit_core::{artifact_store::SnapshotContext, runtime_key::RuntimeKeyOwner};
use std::{collections::BTreeMap, future::Future, io, task::Poll, time::Instant};
use tokio::io::{AsyncRead, AsyncWrite};

/// Expected metadata is captured from the caller's validated effective config.
pub struct ExpectedModel {
    slot: String,
    config_id: ConfigId,
    vector_id: [u8; 32],
    model: String,
    revision: String,
    task: ModelTask,
    dimensions: Option<u64>,
    execution: ExpectedExecution,
    auth_provider: String,
    device: LocalExecution,
}
enum ExpectedExecution {
    EmbeddingGemma2,
    BgeRerankerV2M3,
}
impl ExpectedModel {
    pub fn new(config: &ResolvedConfig, slot: &str, auth_provider: &str) -> ModelResult<Self> {
        let model = config
            .slots
            .get(slot)
            .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Selection))?;
        if auth_provider.is_empty() {
            return Err(ModelServiceError::new(ModelErrorKind::Selection));
        }
        let dimensions = match &model.manifest.backend {
            graphit_ai::model_manifest::ModelBackend::BgeRerankerV2M3 { .. } => None,
            graphit_ai::model_manifest::ModelBackend::EmbeddingGemma2 { recipe, .. } => {
                Some(recipe.dimensions)
            }
        };
        Ok(Self {
            slot: slot.to_owned(),
            config_id: model.id,
            vector_id: model.vector_id,
            model: model.manifest.id.clone(),
            revision: model.manifest.revision.clone(),
            task: model.manifest.task,
            dimensions,
            execution: match &model.execution {
                ResolvedExecution::BgeRerankerV2M3 { .. } => ExpectedExecution::BgeRerankerV2M3,
                ResolvedExecution::EmbeddingGemma2 { .. } => ExpectedExecution::EmbeddingGemma2,
            },
            auth_provider: auth_provider.to_owned(),
            device: model.device,
        })
    }
    pub fn config_id(&self) -> ConfigId {
        self.config_id
    }
    /// Portable identity of verified model content and effective vector recipe.
    /// Operational paths and scheduling remain part of config_id(), not this digest.
    pub fn vector_id(&self) -> [u8; 32] {
        self.vector_id
    }
    /// Model identity borrowed from the validated effective configuration.
    pub fn model(&self) -> &str {
        &self.model
    }
    /// Model revision borrowed from the validated effective configuration.
    pub fn revision(&self) -> &str {
        &self.revision
    }
    /// Embedding width observed in the captured effective configuration.
    pub fn dimensions(&self) -> Option<u64> {
        self.dimensions
    }
    /// Whether the captured backend executes one embedding text at a time.
    pub fn embedding_is_scalar(&self) -> bool {
        matches!(self.execution, ExpectedExecution::EmbeddingGemma2)
    }
    fn metadata(
        &self,
        id: ConfigId,
        model: &str,
        revision: &str,
        device: LocalDeviceRegistration,
    ) -> ModelResult<()> {
        if id != self.config_id || model != self.model || revision != self.revision {
            return Err(ModelServiceError::new(ModelErrorKind::ConfigChanged));
        }
        if !device.matches_request(
            super::local_execution::local_provider(self.device.device),
            self.device.device_id as i32,
        ) {
            return Err(ModelServiceError::new(ModelErrorKind::UnsupportedExecution));
        }
        Ok(())
    }
}
fn service_error(e: ServiceUseError) -> ModelServiceError {
    use graphit_auth::store::AuthStoreError;
    use graphit_core::auth_document::AuthDocumentError;
    let kind = match &e {
        ServiceUseError::Profile(ProfileError::Cancelled)
        | ServiceUseError::Store(AuthStoreError::Profile(ProfileError::Cancelled))
        | ServiceUseError::Store(AuthStoreError::Core(AuthDocumentError::Cancelled)) => {
            ModelErrorKind::Cancelled
        }
        ServiceUseError::Profile(ProfileError::Deadline)
        | ServiceUseError::Store(AuthStoreError::Profile(ProfileError::Deadline))
        | ServiceUseError::Store(AuthStoreError::Core(AuthDocumentError::Deadline)) => {
            ModelErrorKind::Deadline
        }
        _ => ModelErrorKind::Authentication,
    };
    ModelServiceError::caused(kind, e)
}
fn check(
    lease: &LocalServiceLease<'_>,
    runtime: &RuntimeKeyOwner,
    expected: &ExpectedModel,
    purpose: ServicePurpose,
    operation_deadline: Instant,
) -> ModelResult<()> {
    lease.check().map_err(service_error)?;
    let ServiceBindingView::Local {
        provider,
        purpose: active,
        execution,
    } = lease.binding().map_err(service_error)?
    else {
        return Err(service_error(ServiceUseError::WrongMode));
    };
    if provider != expected.auth_provider || active != purpose {
        return Err(service_error(ServiceUseError::WrongMode));
    }
    if execution != expected.device {
        return Err(service_error(ServiceUseError::WrongMode));
    }
    let original = lease.original().map_err(service_error)?;
    original.snapshot().check().map_err(ModelServiceError::io)?;
    if Instant::now() >= operation_deadline {
        return Err(ModelServiceError::operation_deadline());
    }
    let key = runtime
        .check(original.snapshot())
        .map_err(ModelServiceError::from)?;
    key.secret(original.snapshot())
        .map_err(ModelServiceError::from)?;
    if Instant::now() >= operation_deadline {
        return Err(ModelServiceError::operation_deadline());
    }
    Ok(())
}
async fn observe<F: Future>(
    lease: &LocalServiceLease<'_>,
    runtime: &RuntimeKeyOwner,
    expected: &ExpectedModel,
    purpose: ServicePurpose,
    operation_deadline: Instant,
    future: F,
) -> ModelResult<F::Output> {
    check(lease, runtime, expected, purpose, operation_deadline)?;
    let original = lease.original().map_err(service_error)?;
    let gate = || check(lease, runtime, expected, purpose, operation_deadline);
    tokio::pin!(future);
    let polled = poll_fn(|cx| {
        if let Err(e) = gate() {
            return Poll::Ready(Err(e));
        }
        future.as_mut().poll(cx).map(Ok)
    });
    let result = tokio::select! {
        biased;
        _ = original.cancellation().cancelled() => {
            gate()?;
            return Err(ModelServiceError::new(ModelErrorKind::Cancelled));
        },
        _ = tokio::time::sleep_until(operation_deadline.into()) => {
            gate()?;
            return Err(ModelServiceError::operation_deadline());
        },
        result = polled => result?,
    };
    gate()?;
    Ok(result)
}
fn client_config() -> tarpc::client::Config {
    let mut config = tarpc::client::Config::default();
    // There is exactly one operation on this short-lived connection.
    config.pending_request_buffer = 1;
    config.max_in_flight_requests = 1;
    config
}
/// One embedding RPC, bounded independently of a continuous service Original.
pub struct EmbedOperation {
    pub input: EmbedInput,
    pub texts: Vec<String>,
    pub deadline: Instant,
}

pub struct ModelServiceClient {
    limits: RpcLimits,
}
impl ModelServiceClient {
    pub fn new(limits: RpcLimits) -> ModelResult<Self> {
        Ok(Self {
            limits: RpcLimits::new(
                limits.max_frame_bytes,
                limits.max_texts,
                limits.max_input_bytes,
                limits.max_output_values,
            )?,
        })
    }
    /// The RPC bounds validated when this client was constructed.
    /// Callers may size batches using this copy; changing it does not change the
    /// client or replace its input/output, transport and authorization checks.
    pub fn limits(&self) -> RpcLimits {
        self.limits
    }

    pub async fn embed<S, F>(
        &self,
        lease: &LocalServiceLease<'_>,
        runtime: &RuntimeKeyOwner,
        expected: &ExpectedModel,
        mut operation: EmbedOperation,
        connect: F,
    ) -> ModelResult<EmbedOutput>
    where
        S: AsyncRead + AsyncWrite + Unpin,
        F: Future<Output = io::Result<S>>,
    {
        let original = lease.original().map_err(service_error)?;
        if let Some(deadline) = original.snapshot().deadline() {
            operation.deadline = operation.deadline.min(deadline);
        }
        observe(
            lease,
            runtime,
            expected,
            ServicePurpose::Embedding,
            operation.deadline,
            self.embed_checked(lease, runtime, expected, operation, connect),
        )
        .await?
    }
    async fn embed_checked<S, F>(
        &self,
        lease: &LocalServiceLease<'_>,
        runtime: &RuntimeKeyOwner,
        expected: &ExpectedModel,
        operation: EmbedOperation,
        connect: F,
    ) -> ModelResult<EmbedOutput>
    where
        S: AsyncRead + AsyncWrite + Unpin,
        F: Future<Output = io::Result<S>>,
    {
        let EmbedOperation {
            input,
            texts,
            deadline,
        } = operation;
        if expected.task != ModelTask::Embedding {
            return Err(ModelServiceError::new(ModelErrorKind::Selection));
        }
        check(
            lease,
            runtime,
            expected,
            ServicePurpose::Embedding,
            deadline,
        )?;
        self.limits
            .input(None, &texts)
            .map_err(ModelServiceError::local_input_limit)?;
        let stream = observe(
            lease,
            runtime,
            expected,
            ServicePurpose::Embedding,
            deadline,
            connect,
        )
        .await?
        .map_err(|e| ModelServiceError::caused(ModelErrorKind::Transport, e))?;
        check(
            lease,
            runtime,
            expected,
            ServicePurpose::Embedding,
            deadline,
        )?;
        let original = lease.original().map_err(service_error)?;
        let borrow = runtime
            .check(original.snapshot())
            .map_err(ModelServiceError::from)?;
        let credential = RuntimeCredential::from_secret(
            borrow
                .secret(original.snapshot())
                .map_err(ModelServiceError::from)?,
        )?;
        let transport = client_transport(stream, self.limits)?;
        let tarpc::client::NewClient { client, dispatch } =
            ModelRpcClient::new(client_config(), transport);
        let mut context = tarpc::context::current();
        context.deadline = deadline;
        let count = texts.len();
        let call = client.embed(
            context,
            credential,
            expected.slot.clone(),
            expected.config_id,
            input,
            texts,
        );
        let work = async {
            tokio::pin!(call, dispatch);
            tokio::select! {
                biased;
                result = &mut call => result.map_err(|e| ModelServiceError::caused(ModelErrorKind::Transport,e)),
                result = &mut dispatch => {
                    result.map_err(|e| ModelServiceError::caused(ModelErrorKind::Transport,e))?;
                    call.await.map_err(|e| ModelServiceError::caused(ModelErrorKind::Transport,e))
                },
            }
        };
        let output = observe(
            lease,
            runtime,
            expected,
            ServicePurpose::Embedding,
            deadline,
            work,
        )
        .await??
        .map_err(ModelServiceError::remote_embedding)?;
        self.limits
            .embedding(&output, count)
            .map_err(ModelServiceError::local_output_limit)?;
        if Some(output.dimensions) != expected.dimensions {
            return Err(ModelServiceError::new(ModelErrorKind::Output));
        }
        expected.metadata(
            output.config_id,
            &output.model,
            &output.revision,
            output.device,
        )?;
        borrow
            .secret(original.snapshot())
            .map_err(ModelServiceError::from)?;
        check(
            lease,
            runtime,
            expected,
            ServicePurpose::Embedding,
            deadline,
        )?;
        Ok(output)
    }
    pub async fn rerank<S, F>(
        &self,
        lease: &LocalServiceLease<'_>,
        runtime: &RuntimeKeyOwner,
        expected: &ExpectedModel,
        query: String,
        documents: Vec<String>,
        connect: F,
    ) -> ModelResult<RerankOutput>
    where
        S: AsyncRead + AsyncWrite + Unpin,
        F: Future<Output = io::Result<S>>,
    {
        let deadline = lease
            .original()
            .map_err(service_error)?
            .snapshot()
            .required_deadline()
            .map_err(ModelServiceError::io)?;
        observe(
            lease,
            runtime,
            expected,
            ServicePurpose::Rerank,
            deadline,
            self.rerank_checked(lease, runtime, expected, query, documents, connect),
        )
        .await?
    }
    async fn rerank_checked<S, F>(
        &self,
        lease: &LocalServiceLease<'_>,
        runtime: &RuntimeKeyOwner,
        expected: &ExpectedModel,
        query: String,
        documents: Vec<String>,
        connect: F,
    ) -> ModelResult<RerankOutput>
    where
        S: AsyncRead + AsyncWrite + Unpin,
        F: Future<Output = io::Result<S>>,
    {
        let deadline = lease
            .original()
            .map_err(service_error)?
            .snapshot()
            .required_deadline()
            .map_err(ModelServiceError::io)?;
        if expected.task != ModelTask::Rerank {
            return Err(ModelServiceError::new(ModelErrorKind::Selection));
        }
        check(lease, runtime, expected, ServicePurpose::Rerank, deadline)?;
        self.limits.input(Some(&query), &documents)?;
        let stream = observe(
            lease,
            runtime,
            expected,
            ServicePurpose::Rerank,
            deadline,
            connect,
        )
        .await?
        .map_err(|e| ModelServiceError::caused(ModelErrorKind::Transport, e))?;
        check(lease, runtime, expected, ServicePurpose::Rerank, deadline)?;
        let original = lease.original().map_err(service_error)?;
        let borrow = runtime
            .check(original.snapshot())
            .map_err(ModelServiceError::from)?;
        let credential = RuntimeCredential::from_secret(
            borrow
                .secret(original.snapshot())
                .map_err(ModelServiceError::from)?,
        )?;
        let tarpc::client::NewClient { client, dispatch } =
            ModelRpcClient::new(client_config(), client_transport(stream, self.limits)?);
        let mut context = tarpc::context::current();
        context.deadline = deadline;
        let count = documents.len();
        let call = client.rerank(
            context,
            credential,
            expected.slot.clone(),
            expected.config_id,
            query,
            documents,
        );
        let work = async {
            tokio::pin!(call, dispatch);
            tokio::select! {
                biased;
                result = &mut call => result.map_err(|e| ModelServiceError::caused(ModelErrorKind::Transport,e)),
                result = &mut dispatch => {
                    result.map_err(|e| ModelServiceError::caused(ModelErrorKind::Transport,e))?;
                    call.await.map_err(|e| ModelServiceError::caused(ModelErrorKind::Transport,e))
                },
            }
        };
        let output = observe(
            lease,
            runtime,
            expected,
            ServicePurpose::Rerank,
            deadline,
            work,
        )
        .await??
        .map_err(ModelServiceError::remote_category)?;
        self.limits.rerank(&output, count)?;
        expected.metadata(
            output.config_id,
            &output.model,
            &output.revision,
            output.device,
        )?;
        borrow
            .secret(original.snapshot())
            .map_err(ModelServiceError::from)?;
        check(lease, runtime, expected, ServicePurpose::Rerank, deadline)?;
        Ok(output)
    }
    /// Refresh the canonical source, requiring every caller-captured slot identity.
    /// The complete ACK can include independent local purposes; it grants no access.
    pub async fn refresh<S, F>(
        &self,
        runtime: &RuntimeKeyOwner,
        original: &SnapshotContext,
        expected_config_ids: BTreeMap<String, ConfigId>,
        connect: F,
    ) -> ModelResult<RefreshOutput>
    where
        S: AsyncRead + AsyncWrite + Unpin,
        F: Future<Output = io::Result<S>>,
    {
        original
            .required_deadline()
            .map_err(ModelServiceError::io)?;
        original.check().map_err(ModelServiceError::io)?;
        let borrow = runtime.check(original).map_err(ModelServiceError::from)?;
        let stream = snapshot_wait(original, connect)
            .await?
            .map_err(|e| ModelServiceError::caused(ModelErrorKind::Transport, e))?;
        let credential = RuntimeCredential::from_secret(
            borrow.secret(original).map_err(ModelServiceError::from)?,
        )?;
        let tarpc::client::NewClient { client, dispatch } =
            ModelRpcClient::new(client_config(), client_transport(stream, self.limits)?);
        let mut context = tarpc::context::current();
        context.deadline = original
            .required_deadline()
            .map_err(ModelServiceError::io)?;
        let call = client.refresh(context, credential, expected_config_ids.clone());
        let work = async {
            tokio::pin!(call, dispatch);
            tokio::select! {
                biased;
                result = &mut call => result.map_err(|e| ModelServiceError::caused(ModelErrorKind::Transport,e)),
                result = &mut dispatch => {
                    result.map_err(|e| ModelServiceError::caused(ModelErrorKind::Transport,e))?;
                    call.await.map_err(|e| ModelServiceError::caused(ModelErrorKind::Transport,e))
                },
            }
        };
        let result = snapshot_wait(original, work).await??;
        borrow.secret(original).map_err(ModelServiceError::from)?;
        original.check().map_err(ModelServiceError::io)?;
        let output = result.map_err(ModelServiceError::remote_category)?;
        if expected_config_ids
            .iter()
            .any(|(slot, id)| output.config_ids.get(slot) != Some(id))
        {
            return Err(ModelServiceError::new(ModelErrorKind::ConfigChanged));
        }
        Ok(output)
    }

    /// Installation shutdown uses the parent's actual runtime owner and original
    /// snapshot; it does not fabricate a project/service lease in the child.
    pub async fn shutdown<S, F>(
        &self,
        runtime: &RuntimeKeyOwner,
        original: &SnapshotContext,
        connect: F,
    ) -> ModelResult<()>
    where
        S: AsyncRead + AsyncWrite + Unpin,
        F: Future<Output = io::Result<S>>,
    {
        original
            .required_deadline()
            .map_err(ModelServiceError::io)?;
        let borrow = runtime.check(original).map_err(ModelServiceError::from)?;
        let stream = snapshot_wait(original, connect)
            .await?
            .map_err(|e| ModelServiceError::caused(ModelErrorKind::Transport, e))?;
        let credential = RuntimeCredential::from_secret(
            borrow.secret(original).map_err(ModelServiceError::from)?,
        )?;
        let tarpc::client::NewClient { client, dispatch } =
            ModelRpcClient::new(client_config(), client_transport(stream, self.limits)?);
        let mut context = tarpc::context::current();
        context.deadline = original
            .required_deadline()
            .map_err(ModelServiceError::io)?;
        let call = client.shutdown(context, credential);
        let work = async {
            tokio::pin!(call, dispatch);
            tokio::select! {
                biased;
                result = &mut call => result.map_err(|e| ModelServiceError::caused(ModelErrorKind::Transport,e)),
                result = &mut dispatch => {
                    result.map_err(|e| ModelServiceError::caused(ModelErrorKind::Transport,e))?;
                    call.await.map_err(|e| ModelServiceError::caused(ModelErrorKind::Transport,e))
                },
            }
        };
        let result = snapshot_wait(original, work).await??;
        borrow.secret(original).map_err(ModelServiceError::from)?;
        original.check().map_err(ModelServiceError::io)?;
        result.map_err(ModelServiceError::remote_category)
    }
}
async fn snapshot_wait<F: Future>(original: &SnapshotContext, future: F) -> ModelResult<F::Output> {
    let operation_deadline = original
        .required_deadline()
        .map_err(ModelServiceError::io)?;
    tokio::pin!(future);
    let checked = poll_fn(|cx| {
        if let Err(e) = original.check() {
            return Poll::Ready(Err(ModelServiceError::io(e)));
        }
        future.as_mut().poll(cx).map(Ok)
    });
    let output = tokio::select! {
        biased;
        _ = original.cancellation().cancelled() => return Err(ModelServiceError::new(ModelErrorKind::Cancelled)),
        _ = tokio::time::sleep_until(operation_deadline.into()) => return Err(ModelServiceError::operation_deadline()),
        output = checked => output?,
    };
    original.check().map_err(ModelServiceError::io)?;
    Ok(output)
}

#[cfg(test)]
mod service_error_tests {
    use super::*;
    use graphit_auth::store::AuthStoreError;
    use graphit_core::auth_document::AuthDocumentError;
    use std::error::Error;

    #[test]
    fn nested_store_interruption_preserves_kind_and_typed_cause() {
        for (store, kind) in [
            (
                AuthStoreError::Core(AuthDocumentError::Cancelled),
                ModelErrorKind::Cancelled,
            ),
            (
                AuthStoreError::Core(AuthDocumentError::Deadline),
                ModelErrorKind::Deadline,
            ),
            (
                AuthStoreError::Profile(ProfileError::Cancelled),
                ModelErrorKind::Cancelled,
            ),
            (
                AuthStoreError::Profile(ProfileError::Deadline),
                ModelErrorKind::Deadline,
            ),
            (
                AuthStoreError::Core(AuthDocumentError::Denied),
                ModelErrorKind::Authentication,
            ),
            (
                AuthStoreError::Core(AuthDocumentError::Conflict),
                ModelErrorKind::Authentication,
            ),
            (
                AuthStoreError::Profile(ProfileError::Stale),
                ModelErrorKind::Authentication,
            ),
        ] {
            let error = service_error(ServiceUseError::Store(store));
            assert_eq!(error.kind(), kind);
            assert!(matches!(
                error.source().and_then(|source| source.downcast_ref::<ServiceUseError>()),
                Some(ServiceUseError::Store(actual)) if *actual == store
            ));
        }
        for cause in [
            ServiceUseError::WrongMode,
            ServiceUseError::Auth(graphit_auth::AuthError::Denied),
        ] {
            assert_eq!(service_error(cause).kind(), ModelErrorKind::Authentication);
        }
    }
}

#[cfg(test)]
mod limits_tests {
    use super::{ModelErrorKind, ModelServiceClient, RpcLimits};

    #[test]
    fn copied_bounds_do_not_change_the_clients_actual_input_limits() {
        let client = ModelServiceClient::new(RpcLimits::new(1024, 2, 3, 4).unwrap()).unwrap();
        let mut copied = client.limits();
        assert_eq!(copied.max_frame_bytes, 1024);
        assert_eq!(copied.max_texts, 2);
        assert_eq!(copied.max_input_bytes, 3);
        assert_eq!(copied.max_output_values, 4);
        copied.max_frame_bytes = 4096;
        copied.max_texts = 3;
        copied.max_input_bytes = 4;
        copied.max_output_values = 8;
        let extra_text = vec![String::new(); 3];
        assert!(copied.input(None, &extra_text).is_ok());
        assert_eq!(
            client.limits().input(None, &extra_text).unwrap_err().kind(),
            ModelErrorKind::Limit,
        );
        let extra_bytes = vec!["éé".to_owned()];
        assert!(copied.input(None, &extra_bytes).is_ok());
        assert_eq!(
            client
                .limits()
                .input(None, &extra_bytes)
                .unwrap_err()
                .kind(),
            ModelErrorKind::Limit,
        );
        assert!(
            client
                .limits()
                .input(None, &["é".into(), "a".into()])
                .is_ok()
        );
        assert_eq!(client.limits().max_frame_bytes, 1024);
        assert_eq!(client.limits().max_output_values, 4);
    }

    #[test]
    fn raw_invalid_bounds_still_fail_client_construction_with_typed_limit() {
        let valid = RpcLimits::new(1024, 2, 3, 4).unwrap();
        let invalid = [
            RpcLimits {
                max_frame_bytes: 0,
                ..valid
            },
            RpcLimits {
                max_texts: 0,
                ..valid
            },
            RpcLimits {
                max_input_bytes: 0,
                ..valid
            },
            RpcLimits {
                max_output_values: 0,
                ..valid
            },
            RpcLimits {
                max_frame_bytes: usize::MAX,
                ..valid
            },
            RpcLimits {
                max_texts: usize::MAX,
                ..valid
            },
            RpcLimits {
                max_input_bytes: usize::MAX,
                ..valid
            },
            RpcLimits {
                max_output_values: usize::MAX,
                ..valid
            },
        ];
        for bounds in invalid {
            match ModelServiceClient::new(bounds) {
                Ok(_) => panic!("invalid RPC bounds constructed a client"),
                Err(cause) => assert_eq!(cause.kind(), ModelErrorKind::Limit),
            }
        }
    }
}
