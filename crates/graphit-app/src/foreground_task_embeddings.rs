//! Task embeddings attach to current searchable records in finite background batches.
use crate::foreground_memory_jobs::{MemoryJobResources, background_embedding_request};
use graphit_auth::{
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation, ScopeCapture,
    ScopeInput,
};
use graphit_core::{
    artifact_store::CapturedPublicationScope, file_lock::Cancellation, project_paths::ProjectPaths,
};
use graphit_daemon::{
    model_service::{EmbedInput, EmbedOperation},
    supervisor::ProjectContext,
};
use graphit_hub::policy::RequestContext;
use graphit_records::task_store::{
    NativeTaskStore, TASK_DOCUMENT_RECIPE, TaskEmbeddingAttachment, TaskEmbeddingCorpusRead,
    TaskEmbeddingIdentity,
};
use std::{
    io,
    num::{NonZeroU32, NonZeroUsize},
};

impl MemoryJobResources {
    pub(crate) async fn index_tasks(
        &self,
        project: &ProjectPaths,
        selected: &ProjectContext,
        cancellation: &Cancellation,
    ) -> io::Result<()> {
        let request = RequestContext::service(self.policy.content(), cancellation)
            .map_err(io::Error::other)?;
        if project.root() != selected.root() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let captured = CapturedPublicationScope::project(
            project,
            selected.id,
            self.scope,
            request.snapshot(),
        )?;
        let record = captured.binding()?;
        let scope = ScopeInput {
            installation: self.auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &captured,
                    record: &record,
                },
                version: &selected.version,
            }),
            providers: &[],
            binding: self.binding,
            global_version: "",
        };
        let runtime = self
            .auth
            .runtime()
            .check(request.snapshot())
            .map_err(io::Error::other)?;
        let credential = std::str::from_utf8(
            runtime
                .secret(request.snapshot())
                .map_err(io::Error::other)?,
        )
        .map_err(io::Error::other)?;
        let caller = self
            .auth
            .issuer()
            .authenticate_runtime(credential, &request)
            .map_err(io::Error::other)?;
        let authority = ProjectRecordAuthority::new(self.auth.issuer());
        let read = authority
            .admit_authenticated_read(&caller, &request, scope, ProjectReadModule::Task)
            .map_err(io::Error::other)?;
        let write = authority
            .admit_authenticated_write(&caller, &request, scope, ProjectWriteOperation::Task)
            .map_err(io::Error::other)?;
        let model_request =
            background_embedding_request(&self.policy, request.snapshot(), cancellation)?;
        let metadata = self
            .metadata(&model_request)
            .await
            .map_err(crate::embedding_service::EmbeddingServiceError::into_io)?;
        let dimensions = u32::try_from(metadata.dimensions)
            .ok()
            .and_then(NonZeroU32::new)
            .ok_or(io::ErrorKind::InvalidData)?;
        let identity =
            TaskEmbeddingIdentity::new(metadata.vector_id, dimensions, TASK_DOCUMENT_RECIPE.into())
                .map_err(io::Error::other)?;
        let rpc = self.rpc_limits();
        let batch_size = NonZeroUsize::new(
            rpc.max_texts
                .min(rpc.max_output_values / dimensions.get() as usize),
        )
        .ok_or(io::ErrorKind::InvalidInput)?;
        let policy = crate::foreground_tasks::task_policy(
            &self.policy,
            self.scope,
            self.native,
            self.binding,
        )?;
        let corpus =
            NativeTaskStore::capture_embeddings(request.snapshot(), &read, identity, policy.read)
                .await
                .map_err(io::Error::other)?;
        let TaskEmbeddingCorpusRead::Published(mut corpus) = corpus else {
            return Ok(());
        };
        while let Some(documents) = corpus
            .next_documents(request.snapshot(), &read, batch_size)
            .await
            .map_err(io::Error::other)?
        {
            request.check()?;
            let mut pending = documents.into_iter().peekable();
            while pending.peek().is_some() {
                let mut documents = Vec::new();
                let mut bytes = 0usize;
                while let Some(document) = pending.peek() {
                    let next = bytes
                        .checked_add(document.text().len())
                        .ok_or(io::ErrorKind::InvalidData)?;
                    if next > rpc.max_input_bytes {
                        if documents.is_empty() {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "Task embedding document exceeds model request size",
                            ));
                        }
                        break;
                    }
                    bytes = next;
                    documents.push(pending.next().expect("peeked document"));
                }
                let operation_request =
                    background_embedding_request(&self.policy, request.snapshot(), cancellation)?;
                let vectors = self
                    .embed_batch(
                        request.snapshot(),
                        &metadata,
                        EmbedOperation {
                            input: EmbedInput::Document,
                            texts: documents
                                .iter_mut()
                                .map(|document| document.take_text())
                                .collect(),
                            deadline: operation_request.snapshot().required_deadline()?,
                        },
                    )
                    .await
                    .map_err(crate::embedding_service::EmbeddingServiceError::into_io)?;
                if vectors.len() != documents.len()
                    || vectors.iter().any(|vector| {
                        vector.len() != dimensions.get() as usize
                            || vector.iter().any(|v| !v.is_finite())
                    })
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid Task embedding response",
                    ));
                }
                let attachments = documents
                    .into_iter()
                    .zip(vectors)
                    .map(|(document, vector)| TaskEmbeddingAttachment { document, vector })
                    .collect();
                NativeTaskStore::attach_embeddings(
                    request.snapshot(),
                    &write,
                    attachments,
                    policy.write,
                )
                .await
                .map_err(io::Error::other)?;
            }
        }
        Ok(())
    }
}
