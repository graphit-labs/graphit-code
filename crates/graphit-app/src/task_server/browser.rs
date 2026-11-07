//! Browser identity and fresh PROJECT admission for the existing typed Task reader.
use super::*;
use axum::response::{IntoResponse, Response};
use graphit_auth::broker::{
    BrokerVerifier,
    oauth::web::AuthenticatedWebCaller,
    project_access::{
        BrokerProjectAccess, BrokerProjectCaller, BrokerProjectError, BrokerProjectScope,
        BrokerRecordRead, BrokerRecordWrite,
    },
};

#[cfg(all(test, target_os = "linux"))]
pub(crate) mod user_memory_fixture;

enum BrowserReadInput {
    Protocol(Box<graphit_records::task_protocol::ReadRequest>),
    References(Box<graphit_records::task_store::TaskReferencesQuery>),
    Memory(Box<crate::foreground_memory::Query>),
    Knowledge(Box<crate::foreground_knowledge::Query>),
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    KnowledgeAsk(crate::foreground_knowledge::ask::Input),
}

pub(crate) enum BrowserReadOutput {
    Json(Value),
    Encoded(Vec<u8>),
    MemoryWrite(crate::foreground_memory::mutation::Output),
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    KnowledgeAsk(rmcp::model::CallToolResult),
}
impl IntoResponse for BrowserReadOutput {
    fn into_response(self) -> Response {
        match self {
            Self::Json(value) => Json(value).into_response(),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::KnowledgeAsk(value) => Json(
                value
                    .structured_content
                    .unwrap_or_else(|| json!({"status":"failed","error_content":value.content})),
            )
            .into_response(),
            Self::Encoded(bytes) => (
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                bytes,
            )
                .into_response(),
            Self::MemoryWrite(output) => match output.encode() {
                Ok(bytes) => (
                    [(axum::http::header::CONTENT_TYPE, "application/json")],
                    bytes,
                )
                    .into_response(),
                Err(error) => output.encoding_failure(error),
            },
        }
    }
}
pub(crate) struct ReadFailure(Kind);
enum Kind {
    Task(Failure),
    Memory(Box<graphit_records::memory_store::MemoryStoreError>),
    MemorySearch(Box<crate::foreground_memory::search::Error>),
    Knowledge(crate::foreground_knowledge::Failure),
    Project(BrokerProjectError),
    UserCapture(Box<crate::foreground_user_memory::CaptureError>),
    Records(crate::reference_explorer::Failure),
}
impl ReadFailure {
    fn task(error: Failure) -> Self {
        Self(Kind::Task(error))
    }
    fn knowledge(error: crate::foreground_knowledge::Failure) -> Self {
        Self(Kind::Knowledge(error))
    }
    fn memory(error: graphit_records::memory_store::MemoryStoreError) -> Self {
        Self(Kind::Memory(Box::new(error)))
    }
    fn memory_search(error: crate::foreground_memory::search::Error) -> Self {
        match error {
            crate::foreground_memory::search::Error::Store(error) => Self::memory(error),
            error => Self(Kind::MemorySearch(Box::new(error))),
        }
    }
    fn project(error: BrokerProjectError) -> Self {
        Self(Kind::Project(error))
    }
}
impl IntoResponse for ReadFailure {
    fn into_response(self) -> Response {
        let (status, error) = match self.0 {
            Kind::MemorySearch(error) => {
                use crate::foreground_memory::search::Error as E;
                let (status, code) = match *error {
                    E::Service(error)
                        if matches!(
                            crate::embedding_service::service_kind(&error),
                            graphit_daemon::model_service::ModelErrorKind::Cancelled
                                | graphit_daemon::model_service::ModelErrorKind::Deadline
                        ) =>
                    {
                        (StatusCode::REQUEST_TIMEOUT, "request_cancelled_or_expired")
                    }
                    E::Model(error)
                        if matches!(
                            error.kind(),
                            graphit_daemon::model_service::ModelErrorKind::Cancelled
                                | graphit_daemon::model_service::ModelErrorKind::Deadline
                        ) =>
                    {
                        (StatusCode::REQUEST_TIMEOUT, "request_cancelled_or_expired")
                    }
                    E::Auth(_) | E::Service(_) => (StatusCode::FORBIDDEN, "model_access_denied"),
                    E::Model(_) => (StatusCode::BAD_GATEWAY, "model_failure"),
                    E::Operation(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::Interrupted | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        (StatusCode::REQUEST_TIMEOUT, "request_cancelled_or_expired")
                    }
                    E::Json(error)
                        if error.io_error_kind() == Some(std::io::ErrorKind::FileTooLarge) =>
                    {
                        (StatusCode::INTERNAL_SERVER_ERROR, "response_limit")
                    }
                    E::Operation(_) | E::Json(_) => {
                        (StatusCode::INTERNAL_SERVER_ERROR, "memory_search_failure")
                    }
                    E::Store(_) => {
                        unreachable!("native Memory failures use their existing mapping")
                    }
                };
                (status, Failure::new(code))
            }
            Kind::Records(error) => {
                let (status, code) = error.response();
                (status, Failure::new(code))
            }
            Kind::Task(error) => (error.status(), error),
            Kind::Memory(error) => {
                use graphit_records::memory_store::MemoryStoreError as E;
                if let E::LexicalIndexUnavailable(_) = error.as_ref() {
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        Json(json!({"error":{
                            "code":"lexical_index_unavailable","message":error.to_string()
                        }})),
                    )
                        .into_response();
                }
                if let E::RevisionConflict { expected, actual } = error.as_ref() {
                    return (
                        StatusCode::CONFLICT,
                        Json(json!({"error":{
                            "code":"revision_conflict","expected":expected,"actual":actual
                        }})),
                    )
                        .into_response();
                }
                let status = match error.as_ref() {
                    E::IdempotencyConflict | E::RevisionConflict { .. } | E::NoChange => {
                        Some(StatusCode::CONFLICT)
                    }
                    E::NotFound => Some(StatusCode::NOT_FOUND),
                    _ => None,
                };
                let code = match *error {
                    E::Auth(_) => "authentication_or_permission_denied",
                    E::Request(_) => "request_cancelled_or_expired",
                    E::References(
                        graphit_records::reference_projection::ReferenceProjectionError::Request(_),
                    ) => "request_cancelled_or_expired",
                    E::WrongAdmission => "wrong_admission",
                    E::IdempotencyConflict => "idempotency_conflict",
                    E::RevisionConflict { .. } => "revision_conflict",
                    E::NotFound => "memory_not_found",
                    E::NoChange => "no_change",
                    E::DifferentRequest => "different_original_request",
                    E::IncompleteCorpus => "incomplete_corpus",
                    E::Limit(_) => "memory_read_limit",
                    E::Invalid(_) => "invalid_memory_corpus",
                    E::Artifact(_) => "artifact_failure",
                    E::Arrow(_) => "arrow_failure",
                    E::Json(error)
                        if error.io_error_kind() == Some(std::io::ErrorKind::FileTooLarge) =>
                    {
                        "response_limit"
                    }
                    E::Json(error) if error.is_io() => "response_encoding_failed",
                    E::Json(_) => "persisted_json_failure",
                    E::References(_) => "reference_projection_failure",
                    E::Store(_) => "store_failure",
                    E::Branch(_) => "branch_failure",
                    _ => "memory_store_failure",
                };
                let error = Failure::new(code);
                (status.unwrap_or_else(|| error.status()), error)
            }
            Kind::Knowledge(error) => {
                use crate::foreground_knowledge::Failure as E;
                use graphit_data::knowledge_index::KnowledgeIndexError as K;
                let (status, code)=match error {
                    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
                    E::Semantic(error) => {
                        let (status,code)=crate::foreground_knowledge::search::semantic_response(&error);
                        (Some(status),code)
                    }
                    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
                    E::Search(error) => {
                        let (status, code) = crate::foreground_knowledge::search::response(&error);
                        (Some(status), code)
                    }
                    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
                    E::Rerank(error) => {
                        let code = error.code();
                        let status = match code {
                            "request_cancelled_or_expired" => axum::http::StatusCode::REQUEST_TIMEOUT,
                            "authentication_or_permission_denied" => axum::http::StatusCode::FORBIDDEN,
                            "rerank_limit" => axum::http::StatusCode::PAYLOAD_TOO_LARGE,
                            "rerank_service_unavailable" => axum::http::StatusCode::SERVICE_UNAVAILABLE,
                            _ => axum::http::StatusCode::BAD_GATEWAY,
                        };
                        (Some(status), code)
                    }
                    E::References(e) => {
                        let (status, code) = crate::reference_explorer::Failure::KnowledgeReferences(e).response();
                        (Some(status), code)
                    }
                    E::Encoding(e) if e.io_error_kind()==Some(std::io::ErrorKind::FileTooLarge)=>(None, "response_limit"),
                    E::Encoding(_)=>(None, "response_encoding_failed"),
                    E::Native(e)=>(None, match *e {
                        K::Auth(_)=>"authentication_or_permission_denied",
                        K::Request(ref error) if matches!(error.kind(),std::io::ErrorKind::Interrupted|std::io::ErrorKind::TimedOut)=>"request_cancelled_or_expired",
                        K::Request(ref error) if error.kind()==std::io::ErrorKind::InvalidData=>"invalid_knowledge_corpus",
                        K::Request(_)=>"knowledge_index_failure",
                        K::Projection(graphit_data::knowledge_document_projection::KnowledgeDocumentProjectionError::Request(_))=>"request_cancelled_or_expired",
                        K::WrongAdmission=>"wrong_admission",K::DifferentRequest=>"different_original_request",
                        K::LimitExceeded(_)=>"knowledge_read_limit",K::Overflow(_)=>"knowledge_read_overflow",
                        K::InvalidField(_)|K::DuplicateSlug=>"invalid_knowledge_corpus",
                        K::Artifact(_)=>"artifact_failure",K::Arrow(_)=>"arrow_failure",
                        K::Store(_)=>"store_failure",K::Branch(_)=>"branch_failure",
                        _=>"knowledge_index_failure",
                    }),
                };
                let error = Failure::new(code);
                (status.unwrap_or_else(|| error.status()), error)
            }
            Kind::UserCapture(error) => {
                use crate::foreground_user_memory::CaptureError as E;
                use graphit_auth::{ServiceUseError, profile::ProfileError};
                let (status, code) = match *error {
                    E::Auth(_) => (StatusCode::FORBIDDEN, "authentication_or_permission_denied"),
                    E::Subject(ServiceUseError::Profile(
                        ProfileError::Cancelled | ProfileError::Deadline,
                    )) => (StatusCode::REQUEST_TIMEOUT, "request_cancelled_or_expired"),
                    E::Subject(ServiceUseError::Broker(_)) => {
                        (StatusCode::UNAUTHORIZED, "invalid_web_identity")
                    }
                    E::Subject(ServiceUseError::Store(_)) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "authentication_store_failure",
                    ),
                    E::Subject(_) => (StatusCode::FORBIDDEN, "authentication_or_permission_denied"),
                    E::Physical(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::Interrupted | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        (StatusCode::REQUEST_TIMEOUT, "request_cancelled_or_expired")
                    }
                    E::Physical(_) => (StatusCode::INTERNAL_SERVER_ERROR, "user_capture_failure"),
                };
                (status, Failure::new(code))
            }
            Kind::Project(error) => {
                let (status, code) = match error {
                    BrokerProjectError::Authentication(_) => {
                        (StatusCode::UNAUTHORIZED, "invalid_web_identity")
                    }
                    BrokerProjectError::Denied | BrokerProjectError::Authority(_) => {
                        (StatusCode::FORBIDDEN, "project_access_denied")
                    }
                    BrokerProjectError::Store(_) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "authentication_store_failure",
                    ),
                    BrokerProjectError::Network(_) => {
                        (StatusCode::BAD_GATEWAY, "project_network_failure")
                    }
                    BrokerProjectError::InvalidResponse => {
                        (StatusCode::BAD_GATEWAY, "invalid_project_response")
                    }
                    BrokerProjectError::Http(_) => {
                        (StatusCode::BAD_GATEWAY, "project_backend_failure")
                    }
                };
                (status, Failure::new(code))
            }
        };
        (status, Json(json!({"error":error}))).into_response()
    }
}
impl TaskServer {
    pub(crate) fn record_snapshot(
        &self,
    ) -> Result<Arc<crate::live_project_selections::LiveProjectSnapshot>, ReadFailure> {
        self.0.live_projects.snapshot().map_err(|error| {
            ReadFailure(Kind::Records(crate::reference_explorer::Failure::Physical(
                error,
            )))
        })
    }
    pub(crate) fn record_resources<'a>(
        &'a self,
        snapshot: &'a crate::live_project_selections::LiveProjectSnapshot,
    ) -> crate::reference_explorer::Resources<'a> {
        crate::reference_explorer::Resources {
            server: self,
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            native_http: &self.0.native_http,
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            native_allow_loopback_http: self.0.native_allow_loopback_http,
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            native_verifiers: self.0.native_verifiers.as_deref(),
            live_projects: &self.0.live_projects,
            auth: &self.0.auth,
            policy: &self.0.policy,
            stop: &self.0.stop,
            projects: snapshot.projects.as_ref(),
            current: snapshot.current,
        }
    }
    #[cfg(test)]
    pub(crate) fn references_test_policy(&self, content: ContentLimits, timeout: Duration) -> Self {
        let snapshot = self
            .record_snapshot()
            .unwrap_or_else(|_| panic!("live references fixture"));
        let mut policy = self.0.policy.clone();
        policy.content = content;
        policy.timeout = timeout;
        Self::new(
            Arc::clone(&self.0.auth),
            Arc::clone(&snapshot.projects),
            snapshot.current.expect("active references fixture"),
            policy,
            CancellationToken::new(),
        )
        .unwrap()
    }
    pub(crate) fn browser_policy(&self) -> (ContentLimits, Duration, CancellationToken) {
        (
            self.0.policy.content,
            self.0.policy.timeout,
            self.0.stop.clone(),
        )
    }
    pub(crate) async fn read_broker<'a>(
        &'a self,
        arguments: Arguments,
        operation: &str,
        identity: (&'a AuthenticatedWebCaller<'a>, &'a BrokerVerifier, &'a str),
        backend: &'a BrokerProjectAccess,
        original: &'a RequestContext<'a>,
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))] ask: Option<
            crate::foreground_knowledge::ask::Transport,
        >,
    ) -> Result<BrowserReadOutput, ReadFailure> {
        let (caller, verifier, provider) = identity;
        let failure = |code| ReadFailure::task(Failure::new(code));
        if self.0.stop.is_cancelled() {
            return Err(failure("stopped"));
        }
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        if operation == "records_references" {
            let snapshot = self.record_snapshot()?;
            return crate::foreground_record_references::read(
                self.record_resources(&snapshot),
                arguments,
                identity,
                backend,
                original,
            )
            .await
            .map(BrowserReadOutput::Encoded)
            .map_err(|e| ReadFailure(Kind::Records(e)));
        }
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        if matches!(
            operation,
            "imported_knowledge_contexts"
                | "imported_knowledge_heads"
                | "imported_knowledge_page"
                | "imported_knowledge_search"
        ) {
            let snapshot = self.record_snapshot()?;
            return crate::reference_explorer::read(
                self.record_resources(&snapshot),
                arguments,
                identity,
                original,
                || self.memory_model(),
            )
            .await
            .map(BrowserReadOutput::Encoded)
            .map_err(|e| ReadFailure(Kind::Records(e)));
        }
        if matches!(operation, "memory_contexts" | "knowledge_contexts") {
            if arguments.input.is_some()
                || arguments.selection.project_dir.is_some()
                || arguments.selection.project_id.is_some()
                || arguments.selection.project_version.is_some()
            {
                return Err(failure("invalid_arguments"));
            }
            let module = if operation == "memory_contexts" {
                ProjectReadModule::Memory
            } else {
                ProjectReadModule::Knowledge
            };
            let snapshot = self.record_snapshot()?;
            return crate::reference_explorer::contexts(
                self.record_resources(&snapshot),
                module,
                identity,
                backend,
                original,
            )
            .await
            .map(BrowserReadOutput::Encoded)
            .map_err(|e| ReadFailure(Kind::Records(e)));
        }
        if operation.starts_with("user_memory_") {
            if arguments.selection.project_dir.is_some()
                || arguments.selection.project_id.is_some()
                || arguments.selection.project_version.is_some()
            {
                return Err(failure("invalid_arguments"));
            }
            let query: crate::foreground_memory::Query = serde_json::from_value(
                arguments
                    .input
                    .ok_or_else(|| failure("invalid_arguments"))?,
            )
            .map_err(|_| failure("invalid_arguments"))?;
            let issuer = self.0.auth.issuer();
            let scope = crate::foreground_user_memory::capture(
                issuer,
                self.0.auth.installation(),
                caller,
                verifier,
                original,
                &self.0.policy,
            )
            .map_err(|e| ReadFailure(Kind::UserCapture(Box::new(e))))?;
            let binding = scope.binding().map_err(|e| {
                ReadFailure(Kind::UserCapture(Box::new(
                    crate::foreground_user_memory::CaptureError::Physical(e),
                )))
            })?;
            let query = match query {
                crate::foreground_memory::Query::Mutation(input) => {
                    let admission = issuer
                        .admit_web_user_memory_write(
                            caller,
                            verifier,
                            original,
                            provider,
                            ScopeCapture {
                                live: &scope,
                                record: &binding,
                            },
                            graphit_auth::UserMemoryWriteOperation::Memory,
                        )
                        .map_err(|e| {
                            ReadFailure(Kind::UserCapture(Box::new(
                                crate::foreground_user_memory::CaptureError::Auth(e),
                            )))
                        })?;
                    return Box::pin(crate::foreground_memory::mutation::user(
                        original.snapshot(),
                        &admission,
                        &self.0.policy,
                        *input,
                    ))
                    .await
                    .map(BrowserReadOutput::MemoryWrite)
                    .map_err(ReadFailure::memory);
                }
                query => query,
            };
            let admission = issuer
                .admit_web_user_memory_read(
                    caller,
                    verifier,
                    original,
                    provider,
                    ScopeCapture {
                        live: &scope,
                        record: &binding,
                    },
                )
                .map_err(|e| {
                    ReadFailure(Kind::UserCapture(Box::new(
                        crate::foreground_user_memory::CaptureError::Auth(e),
                    )))
                })?;
            let bytes = match query {
                crate::foreground_memory::Query::Search(input) => {
                    let resources = self.memory_model().map_err(|_| failure("model_failure"))?;
                    Box::pin(crate::foreground_memory::search::user(
                        original,
                        &admission,
                        &self.0.policy,
                        input,
                        resources.as_deref(),
                    ))
                    .await
                    .map_err(ReadFailure::memory_search)?
                }
                query => Box::pin(crate::foreground_user_memory::read(
                    original.snapshot(),
                    &admission,
                    &self.0.policy,
                    query,
                ))
                .await
                .map_err(ReadFailure::memory)?,
            };
            admission.recheck().map_err(|e| {
                ReadFailure(Kind::UserCapture(Box::new(
                    crate::foreground_user_memory::CaptureError::Auth(e),
                )))
            })?;
            original
                .snapshot()
                .check()
                .map_err(|_| failure("request_cancelled_or_expired"))?;
            if self.0.stop.is_cancelled() {
                return Err(failure("stopped"));
            }
            return Ok(BrowserReadOutput::Encoded(bytes));
        }
        let snapshot = self.record_snapshot()?;
        if operation == "contexts" {
            if arguments.input.is_some()
                || arguments.selection.project_dir.is_some()
                || arguments.selection.project_id.is_some()
                || arguments.selection.project_version.is_some()
            {
                return Err(failure("invalid_arguments"));
            }
            if snapshot.projects.len() > self.0.policy.content.max_collection_entries {
                return Err(failure("response_limit"));
            }
            let mut contexts = Vec::with_capacity(snapshot.projects.len());
            for selected in snapshot.projects.iter() {
                let BrowserReadOutput::Json(value) = Box::pin(self.read_selected_broker(
                    selected,
                    None,
                    identity,
                    backend,
                    original,
                    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
                    None,
                ))
                .await?
                else {
                    return Err(failure("response_encoding_failed"));
                };
                contexts.push(value);
            }
            let current = snapshot
                .current
                .and_then(|current| contexts.get(current))
                .cloned()
                .ok_or_else(|| failure("selection_unavailable"))?;
            let projects = crate::live_project_selections::project_names(
                snapshot.projects.iter(),
                original.snapshot(),
            )
            .map_err(|error| failure(request_failure_code(&error)))?;
            let value = json!({"contexts": contexts, "current": current, "projects": projects});
            self.check_browser_response(&value)?;
            original
                .snapshot()
                .check()
                .map_err(|_| failure("request_cancelled_or_expired"))?;
            if self.0.stop.is_cancelled() {
                return Err(failure("stopped"));
            }
            return Ok(BrowserReadOutput::Json(value));
        }
        if (operation == "references"
            || operation.starts_with("memory_")
            || operation.starts_with("knowledge_"))
            && (arguments.selection.project_dir.is_none()
                || arguments.selection.project_id.is_none()
                || arguments.selection.project_version.is_none())
        {
            return Err(failure("invalid_selection"));
        }
        let selected = arguments
            .selection
            .select_read(
                &snapshot.projects,
                snapshot
                    .current
                    .ok_or_else(|| failure("selection_unavailable"))?,
            )
            .map_err(failure)?;
        let input = if operation == "context" {
            if arguments.input.is_some() {
                return Err(failure("invalid_arguments"));
            }
            None
        } else if operation.starts_with("knowledge_") {
            let input = arguments
                .input
                .ok_or_else(|| failure("invalid_arguments"))?;
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            if operation == "knowledge_ask" {
                let input: crate::foreground_knowledge::ask::Input =
                    serde_json::from_value(input).map_err(|_| failure("invalid_arguments"))?;
                input.validate(self.0.policy.content).map_err(failure)?;
                Some(BrowserReadInput::KnowledgeAsk(input))
            } else {
                Some(BrowserReadInput::Knowledge(Box::new(
                    serde_json::from_value(input).map_err(|_| failure("invalid_arguments"))?,
                )))
            }
            #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
            Some(BrowserReadInput::Knowledge(Box::new(
                serde_json::from_value(input).map_err(|_| failure("invalid_arguments"))?,
            )))
        } else if operation.starts_with("memory_") {
            Some(BrowserReadInput::Memory(Box::new(
                serde_json::from_value(
                    arguments
                        .input
                        .ok_or_else(|| failure("invalid_arguments"))?,
                )
                .map_err(|_| failure("invalid_arguments"))?,
            )))
        } else if operation == "references" {
            Some(BrowserReadInput::References(Box::new(
                serde_json::from_value(
                    arguments
                        .input
                        .ok_or_else(|| failure("invalid_arguments"))?,
                )
                .map_err(|_| failure("invalid_arguments"))?,
            )))
        } else {
            let Operation::Read(input) =
                Operation::parse(operation, false, arguments.input).map_err(failure)?
            else {
                unreachable!("read-only parser")
            };
            Some(BrowserReadInput::Protocol(Box::new(input)))
        };
        Box::pin(self.read_selected_broker(
            &snapshot.projects[selected],
            input,
            (caller, verifier, provider),
            backend,
            original,
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            ask,
        ))
        .await
    }

    async fn read_selected_broker<'a>(
        &'a self,
        selected: &(
            graphit_core::project_paths::ProjectPaths,
            graphit_core::project_identity::ProjectId,
            graphit_core::version_context::VersionContext,
        ),
        input: Option<BrowserReadInput>,
        (caller, verifier, provider): (&'a AuthenticatedWebCaller<'a>, &'a BrokerVerifier, &'a str),
        backend: &'a BrokerProjectAccess,
        original: &'a RequestContext<'a>,
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))] ask: Option<
            crate::foreground_knowledge::ask::Transport,
        >,
    ) -> Result<BrowserReadOutput, ReadFailure> {
        let failure = |code| ReadFailure::task(Failure::new(code));
        original
            .snapshot()
            .check()
            .map_err(|_| failure("request_cancelled_or_expired"))?;
        if self.0.stop.is_cancelled() {
            return Err(failure("stopped"));
        }
        let (paths, id, version) = selected;
        let capture_failure = || match original.snapshot().check() {
            Err(_) => failure("request_cancelled_or_expired"),
            Ok(()) => ReadFailure::task(Failure::auth(AuthError::Denied)),
        };
        let project =
            CapturedPublicationScope::project(paths, *id, self.0.policy.scope, original.snapshot())
                .map_err(|_| capture_failure())?;
        let binding = project.binding().map_err(|_| capture_failure())?;
        let selection = || -> Result<codec::AdmittedSelection, ReadFailure> {
            Ok(codec::AdmittedSelection {
                project_dir: graphit_core::native_path::NativePath::new(paths.root().to_owned())
                    .map_err(|_| failure("invalid_selection"))?,
                project_id: *id,
                project_version: version.version().to_owned(),
            })
        };
        let input =
            match input {
                Some(BrowserReadInput::Memory(query)) => match *query {
                    crate::foreground_memory::Query::Mutation(input) => {
                        let authority = ProjectRecordAuthority::new(self.0.auth.issuer());
                        let admission = authority
                            .admit_broker_write(BrokerRecordWrite {
                                caller,
                                verifier,
                                backend,
                                original,
                                scope: BrokerProjectScope {
                                    installation: self.0.auth.installation(),
                                    project: ProjectCapture {
                                        scope: ScopeCapture {
                                            live: &project,
                                            record: &binding,
                                        },
                                        version,
                                    },
                                    providers: &[],
                                },
                                provider,
                                operation: graphit_auth::ProjectWriteOperation::Memory,
                            })
                            .await
                            .map_err(ReadFailure::project)?;
                        // Keep the native writer's state on the heap instead of embedding
                        // it in the shared Browser dispatcher alongside other domains.
                        let result = Box::pin(crate::foreground_memory::mutation::project(
                            original.snapshot(),
                            &admission,
                            &self.0.policy,
                            selection()?,
                            *input,
                        ))
                        .await;
                        if result.is_ok() || result.as_ref().is_err_and(|error| {
                            matches!(
                                error,
                                graphit_records::memory_store::MemoryStoreError::Confirmed { .. }
                            )
                        }) {
                            TaskServer::wake_embeddings(&self.0, paths, *id, version);
                        }
                        return result
                            .map(BrowserReadOutput::MemoryWrite)
                            .map_err(ReadFailure::memory);
                    }
                    query => Some(BrowserReadInput::Memory(Box::new(query))),
                },
                input => input,
            };
        let module = match &input {
            Some(BrowserReadInput::Memory(_)) => ProjectReadModule::Memory,
            Some(BrowserReadInput::Knowledge(_)) => ProjectReadModule::Knowledge,
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Some(BrowserReadInput::KnowledgeAsk(_)) => ProjectReadModule::Knowledge,
            _ => ProjectReadModule::Task,
        };
        let authority = ProjectRecordAuthority::new(self.0.auth.issuer());
        let admission = authority
            .admit_broker_read(BrokerRecordRead {
                caller: BrokerProjectCaller::Web(caller),
                verifier,
                backend,
                original,
                scope: BrokerProjectScope {
                    installation: self.0.auth.installation(),
                    project: ProjectCapture {
                        scope: ScopeCapture {
                            live: &project,
                            record: &binding,
                        },
                        version,
                    },
                    providers: &[],
                },
                provider,
                module,
            })
            .await
            .map_err(ReadFailure::project)?;
        let input = match input {
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Some(BrowserReadInput::KnowledgeAsk(input)) => {
                let transport = ask.ok_or_else(|| failure("invalid_arguments"))?;
                let availability = self
                    .ask()
                    .map_err(|_| failure("ask_unavailable"))?
                    .ok_or_else(|| failure("ask_unavailable"))?;
                let service = availability
                    .service()
                    .ok_or_else(|| failure("ask_unavailable"))?;
                let allowed = || {
                    (transport.session_live)()
                        && !self.0.stop.is_cancelled()
                        && matches!(
                            self.0.live_projects.is_active(paths, *id, version),
                            Ok(true)
                        )
                        && admission.recheck().is_ok()
                };
                if !allowed() {
                    return Err(failure("authentication_or_permission_denied"));
                }
                let needs_model = input.retrieval.as_ref().is_some_and(|retrieval| {
                    retrieval.mode != crate::foreground_knowledge::search::Mode::Fts
                });
                let mut resources = if needs_model {
                    self.memory_model().map_err(|_| failure("model_failure"))?
                } else {
                    None
                };
                if resources.is_some()
                    && !self
                        .selected_embedding_enabled(paths, original.snapshot())
                        .map_err(|_| failure("model_configuration_failure"))?
                {
                    resources = None;
                }
                let mut embedding_service = match resources.as_deref() {
                    Some(resources) => Some(
                        resources
                            .auth
                            .issuer()
                            .capture_web_embedding_service(caller, &admission, original)
                            .map_err(|error| {
                                ReadFailure::knowledge(
                                    crate::knowledge_semantic::KnowledgeSemanticError::from(error)
                                        .into(),
                                )
                            })?,
                    ),
                    None => None,
                };
                transport
                    .ready
                    .send(())
                    .map_err(|_| failure("request_cancelled_or_expired"))?;
                let mut sink = crate::source_server::knowledge_ask::ProgressSink::browser(
                    transport.progress,
                    &allowed,
                );
                let instruction = crate::knowledge_question::capture_question_instruction(
                    self.0.auth.issuer(),
                    paths,
                    original,
                )
                .map_err(|_| failure("knowledge_instruction_unavailable"))?;
                let result = crate::source_server::knowledge_ask::execute_admitted(
                    service,
                    original,
                    &admission,
                    paths.root(),
                    &input,
                    crate::source_server::knowledge_ask::QuestionRetrieval {
                        content: self.0.policy.content,
                        instruction: &instruction,
                        model: resources.as_deref().zip(embedding_service.as_mut()).map(
                            |(resources, service)| {
                                crate::foreground_knowledge::search::QuestionModel {
                                    resources,
                                    service,
                                    rpc_timeout: self.0.policy.timeout,
                                }
                            },
                        ),
                    },
                    &mut sink,
                )
                .await;
                let result = if allowed() {
                    result
                } else {
                    crate::source_server::knowledge_ask::delivery_denied(
                        result,
                        "ProjectDeliveryDenied",
                    )
                };
                return Ok(BrowserReadOutput::KnowledgeAsk(result));
            }
            Some(BrowserReadInput::Knowledge(query)) => {
                let needs_embedding = matches!(query.as_ref(), crate::foreground_knowledge::Query::Search(input)
                    if input.mode != crate::foreground_knowledge::search::Mode::Fts);
                let mut resources = if needs_embedding {
                    self.memory_model().map_err(|_| failure("model_failure"))?
                } else {
                    None
                };
                if matches!(query.as_ref(), crate::foreground_knowledge::Query::Search(input) if input.mode != crate::foreground_knowledge::search::Mode::Fts)
                    && resources.is_some()
                    && !self
                        .selected_embedding_enabled(paths, original.snapshot())
                        .map_err(|_| failure("model_configuration_failure"))?
                {
                    resources = None;
                }
                let context = crate::foreground_knowledge::search::BrowserSearchContext {
                    server: self,
                    auth: &self.0.auth,
                    project: paths,
                    verifiers: self.0.native_verifiers.as_deref(),
                };
                let bytes = crate::foreground_knowledge::read(
                    original,
                    caller,
                    &admission,
                    &self.0.policy,
                    &selection()?,
                    *query,
                    resources.as_deref(),
                    &context,
                )
                .await
                .map_err(ReadFailure::knowledge)?;
                admission
                    .recheck()
                    .map_err(|e| ReadFailure::task(Failure::auth(e)))?;
                if self.0.stop.is_cancelled() {
                    return Err(failure("stopped"));
                }
                return Ok(BrowserReadOutput::Encoded(bytes));
            }
            Some(BrowserReadInput::Memory(query)) => {
                let selected = selection()?;
                let bytes = match *query {
                    crate::foreground_memory::Query::Search(input) => {
                        let resources =
                            self.memory_model().map_err(|_| failure("model_failure"))?;
                        Box::pin(crate::foreground_memory::search::project(
                            original,
                            &admission,
                            &self.0.policy,
                            &selected,
                            input,
                            resources.as_deref(),
                        ))
                        .await
                        .map_err(ReadFailure::memory_search)?
                    }
                    query => Box::pin(crate::foreground_memory::read(
                        original.snapshot(),
                        &admission,
                        &self.0.policy,
                        &selected,
                        query,
                    ))
                    .await
                    .map_err(ReadFailure::memory)?,
                };
                admission
                    .recheck()
                    .map_err(|e| ReadFailure::task(Failure::auth(e)))?;
                if self.0.stop.is_cancelled() {
                    return Err(failure("stopped"));
                }
                return Ok(BrowserReadOutput::Encoded(bytes));
            }
            input => input,
        };
        let mut value = match input {
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Some(BrowserReadInput::KnowledgeAsk(_)) => {
                unreachable!("Ask was handled before Task adaptation")
            }
            Some(BrowserReadInput::Knowledge(_)) => {
                unreachable!("Knowledge was handled before Task adaptation")
            }
            Some(BrowserReadInput::Memory(_)) => {
                unreachable!("Memory was handled before Task adaptation")
            }
            Some(BrowserReadInput::Protocol(input)) => {
                let semantic = matches!(input.as_ref(), graphit_records::task_protocol::ReadRequest::Search(query)
                    if query.mode != graphit_records::task_store::TaskSearchMode::Fts);
                let service = if semantic {
                    Some(
                        self.0
                            .auth
                            .issuer()
                            .capture_web_embedding_service(caller, &admission, original)
                            .map_err(|_| failure("authentication_or_permission_denied"))?,
                    )
                } else {
                    None
                };
                let output =
                    super::execute_task_read(&self.0, paths, *input, original, &admission, service)
                        .await
                        .map_err(ReadFailure::task)?;
                super::bounded_value(output, self.0.policy.content.max_output_bytes).map_err(
                    |error| {
                        failure(
                            if error.io_error_kind() == Some(std::io::ErrorKind::FileTooLarge) {
                                "response_limit"
                            } else {
                                "response_encoding_failed"
                            },
                        )
                    },
                )?
            }
            Some(BrowserReadInput::References(query)) => crate::foreground_task_references::read(
                original.snapshot(),
                &admission,
                &self.0.policy,
                *query,
            )
            .await
            .map_err(|e| ReadFailure::task(Failure::store(e)))?,
            None => {
                let root = graphit_core::native_path::NativePath::new(paths.root().to_owned())
                    .map_err(|_| failure("invalid_selection"))?;
                json!({"result":{"project_dir":root,"project_id":id,"project_version":version.version()}})
            }
        };
        self.check_browser_response(&value)?;
        admission
            .recheck()
            .map_err(|e| ReadFailure::task(Failure::auth(e)))?;
        if self.0.stop.is_cancelled() {
            return Err(failure("stopped"));
        }
        Ok(BrowserReadOutput::Json(value["result"].take()))
    }

    fn check_browser_response(&self, value: &Value) -> Result<(), ReadFailure> {
        let failure = |code| ReadFailure::task(Failure::new(code));
        codec::encode_response(value, self.0.policy.content.max_output_bytes)
            .map(|_| ())
            .map_err(|error| {
                failure(
                    if error.io_error_kind() == Some(std::io::ErrorKind::FileTooLarge) {
                        "response_limit"
                    } else {
                        "response_encoding_failed"
                    },
                )
            })
    }
}

#[cfg(all(test, target_os = "linux"))]
pub(crate) mod memory_fixture;

#[cfg(all(test, target_os = "linux"))]
pub(crate) mod knowledge_fixture;
#[cfg(all(test, target_os = "linux"))]
pub(crate) mod records_fixture;

pub(crate) mod codec;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub(crate) mod live;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod live_execute;

#[cfg(test)]
mod response_tests {
    use super::*;
    use graphit_data::lance_branch_store::JsonFtsIndexState;
    use graphit_records::memory_store::MemoryStoreError;

    #[tokio::test]
    async fn nested_store_interruption_preserves_memory_http_status_and_code() {
        use crate::foreground_memory::search::Error;
        use graphit_auth::{ServiceUseError, store::AuthStoreError};
        use graphit_core::auth_document::AuthDocumentError;
        for (cause, interrupted) in [
            (AuthDocumentError::Cancelled, true),
            (AuthDocumentError::Deadline, true),
            (AuthDocumentError::Denied, false),
            (AuthDocumentError::Conflict, false),
        ] {
            for model in [false, true] {
                let service = ServiceUseError::Store(AuthStoreError::Core(cause));
                let error = if model {
                    Error::Model(crate::embedding_service::EmbeddingServiceError::Service(
                        service,
                    ))
                } else {
                    Error::Service(service)
                };
                let response = ReadFailure::memory_search(error).into_response();
                let (status, code) = if interrupted {
                    (StatusCode::REQUEST_TIMEOUT, "request_cancelled_or_expired")
                } else if model {
                    (StatusCode::BAD_GATEWAY, "model_failure")
                } else {
                    (StatusCode::FORBIDDEN, "model_access_denied")
                };
                assert_eq!(response.status(), status);
                let body = axum::body::to_bytes(response.into_body(), 4096)
                    .await
                    .unwrap();
                assert_eq!(
                    serde_json::from_slice::<Value>(&body).unwrap()["error"]["code"],
                    code
                );
            }
        }
    }

    #[tokio::test]
    async fn memory_lexical_unavailable_response_preserves_cause() {
        for state in [JsonFtsIndexState::Missing, JsonFtsIndexState::Incompatible] {
            let error = MemoryStoreError::LexicalIndexUnavailable(state);
            let message = error.to_string();
            let response =
                ReadFailure::memory_search(crate::foreground_memory::search::Error::Store(error))
                    .into_response();
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            let body = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&body).unwrap(),
                json!({"error":{"code":"lexical_index_unavailable","message":message}})
            );
        }

        let response = ReadFailure::memory_search(crate::foreground_memory::search::Error::Store(
            MemoryStoreError::IncompleteCorpus,
        ))
        .into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            json!({"error":{"code":"incomplete_corpus"}})
        );
    }
}
