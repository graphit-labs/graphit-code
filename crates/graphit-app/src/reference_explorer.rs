//! Readonly browser discovery and pages from the retained installation registry.
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
#[path = "imported_knowledge_heads.rs"]
mod imported_knowledge_heads;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod remote;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
use crate::foreground_knowledge::reference_binding::ReferenceBinding;
use crate::{
    foreground_knowledge,
    task_server::{
        TaskPolicy,
        browser::codec,
        request::{Arguments, Selection},
    },
};
use graphit_auth::{
    AuthError, ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ScopeCapture,
    broker::{
        BrokerVerifier,
        oauth::web::AuthenticatedWebCaller,
        project_access::{
            BrokerProjectAccess, BrokerProjectCaller, BrokerProjectError, BrokerProjectScope,
            BrokerRecordRead,
        },
    },
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, StoreOwner},
    file_io::SnapshotContext,
    native_path::NativePath,
    project_identity::ProjectId,
    project_paths::ProjectPaths,
    version_context::VersionContext,
};
use graphit_daemon::auth_startup::DaemonAuth;
use graphit_data::knowledge_index::{KnowledgeGeneration, KnowledgeIndexError, validate_slug};
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
use graphit_data::knowledge_snapshot::KnowledgeSnapshotPage;
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget,
    native::ContentLimits,
    policy::RequestContext,
    registry::{HubScope, InstalledBackend, InstalledPayload},
};
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
use graphit_knowledge::imported_knowledge::{
    ImportedKnowledgeInput, ImportedKnowledgeOpen, ImportedKnowledgeReadError,
    ImportedKnowledgeReadLimits, open_imported_knowledge,
};
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use std::{io, sync::Arc};
use tokio_util::sync::CancellationToken;

pub(crate) fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArtifactSelector {
    pub(crate) id: String,
    pub(crate) version: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) context_alias: Option<String>,
}
impl ArtifactSelector {
    pub(crate) fn validate(&self, content: ContentLimits) -> Result<(), &'static str> {
        text(&self.id, content)?;
        text(&self.version, content)?;
        if let Some(alias) = &self.context_alias {
            text(alias, content)?;
        }
        Ok(())
    }
    pub(crate) fn native(
        &self,
        policy: &TaskPolicy,
    ) -> Result<(ArtifactId, ConcreteVersion), Failure> {
        Ok((
            ArtifactId::new(&self.id, policy.role_reference)
                .map_err(|_| Failure::Invalid("invalid_arguments"))?,
            ConcreteVersion::new(&self.version, policy.role_reference)
                .map_err(|_| Failure::Invalid("invalid_arguments"))?,
        ))
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum SourceKey {
    ProjectTask,
    ProjectMemory,
    ProjectKnowledge,
    UserMemory,
    ImportedKnowledge { artifact: ArtifactSelector },
}
pub(crate) fn text(value: &str, limits: ContentLimits) -> Result<(), &'static str> {
    if value.is_empty() || value.contains('\0') || value.len() > limits.max_scalar_bytes {
        Err("invalid_arguments")
    } else {
        Ok(())
    }
}
pub(crate) struct Resources<'a> {
    pub(crate) server: &'a crate::task_server::TaskServer,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) native_http: &'a reqwest::Client,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) native_allow_loopback_http: bool,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) native_verifiers: Option<&'a graphit_daemon::broker_verifiers::BrokerVerifierCache>,
    pub(crate) live_projects: &'a crate::live_project_selections::LiveProjectSelections,
    pub(crate) auth: &'a DaemonAuth,
    pub(crate) policy: &'a TaskPolicy,
    pub(crate) stop: &'a CancellationToken,
    pub(crate) projects: &'a Vec<(ProjectPaths, ProjectId, VersionContext)>,
    pub(crate) current: Option<usize>,
}
impl<'a> Resources<'a> {
    pub(crate) fn select_exact(
        &self,
        selection: &Selection,
    ) -> Result<&'a (ProjectPaths, ProjectId, VersionContext), Failure> {
        let (Some(root), Some(id), Some(version)) = (
            selection.project_dir.as_ref(),
            selection.project_id,
            selection.project_version.as_deref(),
        ) else {
            return Err(Failure::Invalid("invalid_selection"));
        };
        let mut matches = self.projects.iter().filter(|(paths, identity, context)| {
            paths.root() == root.as_path() && *identity == id && context.version() == version
        });
        let selected = matches
            .next()
            .ok_or(Failure::Invalid("selection_unavailable"))?;
        if matches.next().is_some() {
            return Err(Failure::Invalid("selection_ambiguous"));
        }
        Ok(selected)
    }
    pub(crate) fn is_active(
        &self,
        selected: &(ProjectPaths, ProjectId, VersionContext),
    ) -> Result<bool, Failure> {
        self.live_projects
            .is_active(&selected.0, selected.1, &selected.2)
            .map_err(Failure::Physical)
    }
    pub(crate) fn require_active(
        &self,
        selected: &(ProjectPaths, ProjectId, VersionContext),
    ) -> Result<(), Failure> {
        if self.is_active(selected)? {
            Ok(())
        } else {
            Err(Failure::Invalid("inactive_project_selection"))
        }
    }
    pub(crate) fn check(&self, original: &RequestContext<'_>) -> Result<(), Failure> {
        original.snapshot().check().map_err(Failure::Physical)?;
        if self.stop.is_cancelled() {
            return Err(Failure::Stopped);
        }
        Ok(())
    }
}
pub(crate) type Identity<'a> = (&'a AuthenticatedWebCaller<'a>, &'a BrokerVerifier, &'a str);

#[derive(Debug)]
pub(crate) enum Failure {
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    Remote(Box<crate::remote_native_mount::ErrorKind>),
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    ImportedSource(Box<graphit_knowledge::imported_knowledge::ImportedKnowledgeSourceError>),
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    ImportedSearch(Box<graphit_knowledge::imported_knowledge::ImportedKnowledgeSearchError>),
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    Semantic(Box<crate::knowledge_semantic::KnowledgeSemanticError>),
    Auth(AuthError),
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    Imported(Box<ImportedKnowledgeReadError>),
    Knowledge(Box<KnowledgeIndexError>),
    Project(Box<BrokerProjectError>),
    Physical(io::Error),
    Encoding(serde_json::Error),
    Invalid(&'static str),
    Unavailable,
    Stopped,
    Task(Box<graphit_records::task_store::TaskStoreError>),
    Memory(Box<graphit_records::memory_store::MemoryStoreError>),
    UserCapture(Box<crate::foreground_user_memory::CaptureError>),
    KnowledgeReferences(Box<graphit_data::knowledge_reference_query::KnowledgeReferenceQueryError>),
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    ImportedReferences(
        Box<graphit_knowledge::imported_reference_query::ImportedKnowledgeReferenceError>,
    ),
}
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
impl From<crate::remote_native_mount::ErrorKind> for Failure {
    fn from(error: crate::remote_native_mount::ErrorKind) -> Self {
        Self::Remote(Box::new(error))
    }
}
impl From<graphit_records::task_store::TaskStoreError> for Failure {
    fn from(e: graphit_records::task_store::TaskStoreError) -> Self {
        Self::Task(Box::new(e))
    }
}
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
impl From<graphit_knowledge::imported_knowledge::ImportedKnowledgeSourceError> for Failure {
    fn from(error: graphit_knowledge::imported_knowledge::ImportedKnowledgeSourceError) -> Self {
        Self::ImportedSource(Box::new(error))
    }
}
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
impl From<graphit_knowledge::imported_knowledge::ImportedKnowledgeSearchError> for Failure {
    fn from(error: graphit_knowledge::imported_knowledge::ImportedKnowledgeSearchError) -> Self {
        Self::ImportedSearch(Box::new(error))
    }
}
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
impl From<crate::knowledge_semantic::KnowledgeSemanticError> for Failure {
    fn from(error: crate::knowledge_semantic::KnowledgeSemanticError) -> Self {
        Self::Semantic(Box::new(error))
    }
}
impl From<graphit_records::memory_store::MemoryStoreError> for Failure {
    fn from(e: graphit_records::memory_store::MemoryStoreError) -> Self {
        Self::Memory(Box::new(e))
    }
}
impl From<graphit_data::knowledge_reference_query::KnowledgeReferenceQueryError> for Failure {
    fn from(e: graphit_data::knowledge_reference_query::KnowledgeReferenceQueryError) -> Self {
        Self::KnowledgeReferences(Box::new(e))
    }
}
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
impl From<graphit_knowledge::imported_reference_query::ImportedKnowledgeReferenceError>
    for Failure
{
    fn from(
        e: graphit_knowledge::imported_reference_query::ImportedKnowledgeReferenceError,
    ) -> Self {
        Self::ImportedReferences(Box::new(e))
    }
}
impl From<crate::foreground_user_memory::CaptureError> for Failure {
    fn from(e: crate::foreground_user_memory::CaptureError) -> Self {
        Self::UserCapture(Box::new(e))
    }
}
impl From<AuthError> for Failure {
    fn from(e: AuthError) -> Self {
        Self::Auth(e)
    }
}
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
impl From<ImportedKnowledgeReadError> for Failure {
    fn from(e: ImportedKnowledgeReadError) -> Self {
        Self::Imported(Box::new(e))
    }
}
impl From<KnowledgeIndexError> for Failure {
    fn from(e: KnowledgeIndexError) -> Self {
        Self::Knowledge(Box::new(e))
    }
}
impl From<BrokerProjectError> for Failure {
    fn from(e: BrokerProjectError) -> Self {
        Self::Project(Box::new(e))
    }
}
impl From<serde_json::Error> for Failure {
    fn from(e: serde_json::Error) -> Self {
        Self::Encoding(e)
    }
}
impl Failure {
    pub(crate) fn response(&self) -> (axum::http::StatusCode, &'static str) {
        use axum::http::StatusCode as S;
        match self {
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::Remote(error) => (error.status(), error.code()),
            Self::Task(e) => {
                use graphit_records::task_store::TaskStoreError as E;
                match e.as_ref() {
                    E::Auth(e) => Self::auth_response(e),
                    E::Request(_) => (S::REQUEST_TIMEOUT, "request_cancelled_or_expired"),
                    E::InvalidCursor => (S::BAD_REQUEST, "invalid_task_cursor"),
                    E::Invalid(_) => (S::BAD_REQUEST, "invalid_task_query_or_corpus"),
                    _ => (S::INTERNAL_SERVER_ERROR, "task_store_failure"),
                }
            }
            Self::Memory(e) => {
                use graphit_records::memory_store::MemoryStoreError as E;
                match e.as_ref() {
                    E::Auth(e) => Self::auth_response(e),
                    E::Request(_) => (S::REQUEST_TIMEOUT, "request_cancelled_or_expired"),
                    E::References(
                        graphit_records::reference_projection::ReferenceProjectionError::Request(_),
                    ) => (S::REQUEST_TIMEOUT, "request_cancelled_or_expired"),
                    E::Invalid(_) => (S::BAD_REQUEST, "invalid_memory_query_or_corpus"),
                    E::Limit(_) => (S::PAYLOAD_TOO_LARGE, "memory_read_limit"),
                    _ => (S::INTERNAL_SERVER_ERROR, "memory_store_failure"),
                }
            }
            Self::UserCapture(e) => match e.as_ref() {
                crate::foreground_user_memory::CaptureError::Auth(e) => Self::auth_response(e),
                crate::foreground_user_memory::CaptureError::Physical(e) => Self::io_response(e),
                crate::foreground_user_memory::CaptureError::Subject(
                    graphit_auth::ServiceUseError::Profile(
                        graphit_auth::profile::ProfileError::Cancelled
                        | graphit_auth::profile::ProfileError::Deadline,
                    ),
                ) => (S::REQUEST_TIMEOUT, "request_cancelled_or_expired"),
                _ => (S::INTERNAL_SERVER_ERROR, "user_capture_failure"),
            },
            Self::KnowledgeReferences(e) => Self::references_response(e),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::ImportedReferences(e) => {
                use graphit_knowledge::imported_reference_query::ImportedKnowledgeReferenceError as E;
                match e.as_ref() {
                    E::Reader(e) => match e {
                        ImportedKnowledgeReadError::Auth(e) => Self::auth_response(e),
                        ImportedKnowledgeReadError::Data(e) => Self::knowledge_response(e),
                        ImportedKnowledgeReadError::Registry(e)
                        | ImportedKnowledgeReadError::Capture(e) => Self::io_response(e),
                        ImportedKnowledgeReadError::SourceSelectionChanged => {
                            (S::CONFLICT, "import_selection_changed")
                        }
                        _ => (S::BAD_REQUEST, "invalid_import_selection"),
                    },
                    E::Query(e) => Self::references_response(e),
                    E::InvalidCursor => (S::BAD_REQUEST, "invalid_import_cursor"),
                    E::Request(e) => Self::io_response(e),
                    E::BindingSerialization(_) => (
                        S::INTERNAL_SERVER_ERROR,
                        "reference_binding_encoding_failure",
                    ),
                }
            }
            Self::Auth(AuthError::Request) => (S::REQUEST_TIMEOUT, "request_cancelled_or_expired"),
            Self::Auth(AuthError::State | AuthError::Io) => {
                (S::INTERNAL_SERVER_ERROR, "authentication_store_failure")
            }
            Self::Auth(_) => (S::FORBIDDEN, "authentication_or_permission_denied"),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::Imported(e) => Self::imported_response(e),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::ImportedSource(error) => imported_knowledge_heads::response(error),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::Semantic(error) => foreground_knowledge::search::semantic_response(error),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::ImportedSearch(error) => match error.as_ref() {
                graphit_knowledge::imported_knowledge::ImportedKnowledgeSearchError::Reader(
                    error,
                ) => Self::imported_response(error),
                graphit_knowledge::imported_knowledge::ImportedKnowledgeSearchError::Query(
                    error,
                ) => foreground_knowledge::search::response(error),
            },
            Self::Knowledge(e) => Self::knowledge_response(e),
            Self::Project(e) => match e.as_ref() {
                BrokerProjectError::Denied => (S::FORBIDDEN, "project_access_denied"),
                BrokerProjectError::Authority(e) => Self::auth_response(e),
                BrokerProjectError::Authentication(_) => (S::UNAUTHORIZED, "invalid_web_identity"),
                BrokerProjectError::Store(_) => {
                    (S::INTERNAL_SERVER_ERROR, "authentication_store_failure")
                }
                _ => (S::BAD_GATEWAY, "project_backend_failure"),
            },
            Self::Physical(e) => Self::io_response(e),
            Self::Encoding(e) => match e.io_error_kind() {
                Some(io::ErrorKind::FileTooLarge) => (S::PAYLOAD_TOO_LARGE, "response_limit"),
                Some(io::ErrorKind::Interrupted | io::ErrorKind::TimedOut) => {
                    (S::REQUEST_TIMEOUT, "request_cancelled_or_expired")
                }
                _ => (S::INTERNAL_SERVER_ERROR, "response_encoding_failed"),
            },
            Self::Invalid(code) => (S::BAD_REQUEST, code),
            Self::Unavailable => (S::SERVICE_UNAVAILABLE, "import_reader_unavailable"),
            Self::Stopped => (S::SERVICE_UNAVAILABLE, "stopped"),
        }
    }
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn imported_response(
        e: &ImportedKnowledgeReadError,
    ) -> (axum::http::StatusCode, &'static str) {
        use axum::http::StatusCode as S;
        match e {
            ImportedKnowledgeReadError::Auth(e) => Self::auth_response(e),
            ImportedKnowledgeReadError::Data(e) => Self::knowledge_response(e),
            ImportedKnowledgeReadError::Registry(e) | ImportedKnowledgeReadError::Capture(e) => {
                Self::io_response(e)
            }
            ImportedKnowledgeReadError::SourceSelectionChanged => {
                (S::CONFLICT, "import_selection_changed")
            }
            _ => (S::BAD_REQUEST, "invalid_import_selection"),
        }
    }
    fn auth_response(e: &AuthError) -> (axum::http::StatusCode, &'static str) {
        use axum::http::StatusCode as S;
        match e {
            AuthError::Request => (S::REQUEST_TIMEOUT, "request_cancelled_or_expired"),
            AuthError::State | AuthError::Io => {
                (S::INTERNAL_SERVER_ERROR, "authentication_store_failure")
            }
            _ => (S::FORBIDDEN, "authentication_or_permission_denied"),
        }
    }
    fn io_response(e: &io::Error) -> (axum::http::StatusCode, &'static str) {
        use axum::http::StatusCode as S;
        if matches!(
            e.kind(),
            io::ErrorKind::Interrupted | io::ErrorKind::TimedOut
        ) {
            (S::REQUEST_TIMEOUT, "request_cancelled_or_expired")
        } else {
            (
                S::INTERNAL_SERVER_ERROR,
                "native_capture_or_registry_failure",
            )
        }
    }
    pub(crate) fn knowledge_response(
        e: &KnowledgeIndexError,
    ) -> (axum::http::StatusCode, &'static str) {
        use axum::http::StatusCode as S;
        match e {
            KnowledgeIndexError::Auth(e) => Self::auth_response(e),
            KnowledgeIndexError::Request(_) => (S::REQUEST_TIMEOUT, "request_cancelled_or_expired"),
            KnowledgeIndexError::Projection(graphit_data::knowledge_document_projection::KnowledgeDocumentProjectionError::Request(_)) => (S::REQUEST_TIMEOUT, "request_cancelled_or_expired"),
            KnowledgeIndexError::LimitExceeded(_) => (S::PAYLOAD_TOO_LARGE, "knowledge_read_limit"),
            _ => (S::INTERNAL_SERVER_ERROR, "knowledge_index_failure"),
        }
    }
    fn references_response(
        e: &graphit_data::knowledge_reference_query::KnowledgeReferenceQueryError,
    ) -> (axum::http::StatusCode, &'static str) {
        use axum::http::StatusCode as S;
        use graphit_data::knowledge_reference_query::KnowledgeReferenceQueryError as E;
        match e {
            E::Data(e) => Self::knowledge_response(e),
            E::InvalidCursor => (S::BAD_REQUEST, "invalid_knowledge_cursor"),
            E::InvalidQuery(_) => (S::BAD_REQUEST, "invalid_reference_query"),
            E::UnknownSource => (S::NOT_FOUND, "unknown_reference_source"),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Body<T> {
    input: T,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PageInput {
    source: SourceKey,
    slug: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct HeadsInput {
    source: SourceKey,
}
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SearchInput {
    source: SourceKey,
    search: foreground_knowledge::search::Input,
}
#[derive(Deserialize, Serialize)]
enum Query {
    Contexts,
    Heads(HeadsInput),
    Page(PageInput),
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    Search(SearchInput),
}
fn arguments<T: Serialize>(input: T) -> Result<Arguments, &'static str> {
    Ok(Arguments {
        selection: Selection::default(),
        input: Some(serde_json::to_value(input).map_err(|_| "invalid_arguments")?),
    })
}
pub(crate) fn parse(
    bytes: &[u8],
    limits: ContentLimits,
    operation: &str,
) -> Result<Arguments, &'static str> {
    match operation {
        "imported_knowledge_heads" => {
            let body: Body<HeadsInput> = codec::decode(bytes, limits)?;
            let SourceKey::ImportedKnowledge { artifact } = &body.input.source else {
                return Err("invalid_arguments");
            };
            artifact.validate(limits)?;
            arguments(Query::Heads(body.input))
        }
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        "imported_knowledge_search" => {
            let body: Body<SearchInput> = codec::decode(bytes, limits)?;
            let SourceKey::ImportedKnowledge { artifact } = &body.input.source else {
                return Err("invalid_arguments");
            };
            artifact.validate(limits)?;
            body.input.search.validate(limits)?;
            arguments(Query::Search(body.input))
        }
        "imported_knowledge_contexts" => {
            let _: Body<Empty> = codec::decode(bytes, limits)?;
            arguments(Query::Contexts)
        }
        "imported_knowledge_page" => {
            let body: Body<PageInput> = codec::decode(bytes, limits)?;
            let SourceKey::ImportedKnowledge { artifact } = &body.input.source else {
                return Err("invalid_arguments");
            };
            artifact.validate(limits)?;
            text(&body.input.slug, limits)?;
            validate_slug(&body.input.slug).map_err(|_| "invalid_arguments")?;
            arguments(Query::Page(body.input))
        }
        "memory_contexts" | "knowledge_contexts" => {
            let _: Body<Empty> = codec::decode(bytes, limits)?;
            Ok(Arguments {
                selection: Selection::default(),
                input: None,
            })
        }
        _ => Err("invalid_arguments"),
    }
}
pub(crate) fn query<T: DeserializeOwned>(arguments: Arguments) -> Result<T, Failure> {
    if arguments.selection.project_dir.is_some()
        || arguments.selection.project_id.is_some()
        || arguments.selection.project_version.is_some()
    {
        return Err(Failure::Invalid("invalid_arguments"));
    }
    serde_json::from_value(
        arguments
            .input
            .ok_or(Failure::Invalid("invalid_arguments"))?,
    )
    .map_err(|_| Failure::Invalid("invalid_arguments"))
}

/// Serialize one borrowed response while checking the original request and shared stop.
pub(crate) fn encode<T: Serialize>(
    response: &T,
    original: &Arc<SnapshotContext>,
    stop: &CancellationToken,
    maximum: usize,
) -> Result<Vec<u8>, Failure> {
    struct Writer<'a> {
        bytes: Vec<u8>,
        original: &'a SnapshotContext,
        stop: &'a CancellationToken,
        maximum: usize,
        failure: Option<Failure>,
    }
    impl io::Write for Writer<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if let Err(error) = self.original.check() {
                self.failure = Some(Failure::Physical(error));
                return Err(io::Error::other("original request ended"));
            }
            if self.stop.is_cancelled() {
                self.failure = Some(Failure::Stopped);
                return Err(io::Error::other("browser stopped"));
            }
            if bytes.len() > self.maximum - self.bytes.len() {
                return Err(io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    "browser response limit",
                ));
            }
            self.bytes.try_reserve(bytes.len()).map_err(|_| {
                io::Error::new(io::ErrorKind::OutOfMemory, "browser response allocation")
            })?;
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.write(&[]).map(|_| ())
        }
    }
    let mut writer = Writer {
        bytes: Vec::new(),
        original,
        stop,
        maximum,
        failure: None,
    };
    if let Err(error) = serde_json::to_writer(&mut writer, response) {
        return Err(writer.failure.take().unwrap_or(Failure::Encoding(error)));
    }
    original.check().map_err(Failure::Physical)?;
    if stop.is_cancelled() {
        return Err(Failure::Stopped);
    }
    Ok(writer.bytes)
}
#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
enum PageRead<'a> {
    RegistryAbsent,
    Published {
        generation: &'a KnowledgeGeneration,
        page: foreground_knowledge::PageView<'a>,
    },
}
#[derive(Serialize)]
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
struct Contexts {
    contexts: Vec<SourceKey>,
}
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub(crate) async fn read<'a>(
    resources: Resources<'a>,
    arguments: Arguments,
    identity: Identity<'a>,
    original: &'a RequestContext<'a>,
    model: impl FnOnce() -> io::Result<Option<Arc<crate::foreground_memory_jobs::MemoryJobResources>>>,
) -> Result<Vec<u8>, Failure> {
    resources.check(original)?;
    let input: Query = query(arguments)?;
    let rerank = foreground_knowledge::imported_search::ImportedRerankContext {
        auth: resources.auth,
        owner: foreground_knowledge::imported_search::ImportedRerankOwner::Browser(
            resources.server,
        ),
        verifiers: resources.native_verifiers,
    };
    let selected = match &input {
        Query::Heads(input) => match &input.source {
            SourceKey::ImportedKnowledge { artifact } => Some(artifact.native(resources.policy)?),
            _ => return Err(Failure::Invalid("invalid_arguments")),
        },
        Query::Search(input) => match &input.source {
            SourceKey::ImportedKnowledge { artifact } => Some(artifact.native(resources.policy)?),
            _ => return Err(Failure::Invalid("invalid_arguments")),
        },
        Query::Page(input) => match &input.source {
            SourceKey::ImportedKnowledge { artifact } => Some(artifact.native(resources.policy)?),
            _ => return Err(Failure::Invalid("invalid_arguments")),
        },
        Query::Contexts => None,
    };
    let policy = resources
        .policy
        .imported_knowledge
        .as_ref()
        .ok_or(Failure::Unavailable)?;
    let (caller, verifier, provider) = identity;
    let admission = resources.auth.issuer().admit_web_imported_knowledge_read(
        caller,
        verifier,
        original,
        provider,
        resources.auth.installation(),
    )?;
    let scope = admission.installation();
    let StoreOwner::Installation { installation_id } = scope.live.owner() else {
        return Err(ImportedKnowledgeReadError::InvalidRegistryScope.into());
    };
    if policy.registry.scope()
        != &(HubScope::Global {
            installation_id: installation_id.as_str().to_owned(),
        })
        || policy.registry.path().parent() != Some(scope.live.owner_root())
    {
        return Err(ImportedKnowledgeReadError::InvalidRegistryScope.into());
    }
    if let Some((id, version)) = &selected
        && let Some(selection) = crate::remote_native_mount::Selection::capture(
            crate::remote_native_mount::ReadAdmission::Knowledge(&admission),
            &policy.registry,
            id,
            version,
        )?
    {
        let bytes = remote::read(
            &resources, &admission, selection, &input, original, model, &rerank,
        )
        .await?;
        admission.recheck()?;
        resources.check(original)?;
        return Ok(bytes);
    }
    let bytes = match input {
        Query::Heads(input) => {
            let SourceKey::ImportedKnowledge { artifact } = &input.source else {
                return Err(Failure::Invalid("invalid_arguments"));
            };
            let (id, version) = selected.ok_or(Failure::Invalid("invalid_arguments"))?;
            let opened = open_imported_knowledge(
                &admission,
                ImportedKnowledgeInput {
                    registry: &policy.registry,
                    id: &id,
                    version: &version,
                    context_alias: artifact.context_alias.as_deref(),
                },
                ImportedKnowledgeReadLimits {
                    directory: policy.directory,
                    index: foreground_knowledge::limits(resources.policy)?,
                },
            )
            .await?;
            imported_knowledge_heads::read(&opened, &input.source, &resources, original).await?
        }
        Query::Search(input) => {
            let SourceKey::ImportedKnowledge { artifact } = &input.source else {
                return Err(Failure::Invalid("invalid_arguments"));
            };
            input
                .search
                .validate(resources.policy.content)
                .map_err(Failure::Invalid)?;
            let (id, version) = selected.ok_or(Failure::Invalid("invalid_arguments"))?;
            let opened = open_imported_knowledge(
                &admission,
                ImportedKnowledgeInput {
                    registry: &policy.registry,
                    id: &id,
                    version: &version,
                    context_alias: artifact.context_alias.as_deref(),
                },
                ImportedKnowledgeReadLimits {
                    directory: policy.directory,
                    index: foreground_knowledge::limits(resources.policy)?,
                },
            )
            .await?;
            let limits = foreground_knowledge::search::query_limits(
                foreground_knowledge::limits(resources.policy)?,
                resources.policy.content,
            )?;
            let preview_limits =
                foreground_knowledge::search::preview_limits(resources.policy.content)?;
            let page = match &opened {
                ImportedKnowledgeOpen::RegistryAbsent => None,
                ImportedKnowledgeOpen::Published(reader) => {
                    let outcome = if input.search.mode == foreground_knowledge::search::Mode::Fts {
                        foreground_knowledge::search::SearchOutcome::Lexical(
                            reader
                                .search(input.search.native("web"), limits, preview_limits)
                                .await?,
                        )
                    } else {
                        let resources = model().map_err(Failure::Physical)?;
                        let binding = serde_json::to_string(&(
                            "web",
                            input.search.mode,
                            input.search.metric,
                        ))?;
                        foreground_knowledge::imported_search::query(
                            reader,
                            &admission,
                            resources.as_deref(),
                            crate::knowledge_semantic::KnowledgeSemanticSearchOptions {
                                page: input.search.native(&binding),
                                mode: input.search.vector_mode(),
                                metric: input.search.metric.native(),
                                limits,
                                preview: preview_limits,
                            },
                        )
                        .await?
                    };
                    Some(
                        rerank
                            .copied(&admission, reader, &input.search, limits.read, outcome)
                            .await?,
                    )
                }
            };
            let bytes = encode(
                &ReferenceBinding {
                    selection: None,
                    source: &input.source,
                    focused_source: None,
                    native: page.as_ref().map_or(
                        foreground_knowledge::search::ReadView::Empty {
                            state: "registry_absent",
                        },
                        foreground_knowledge::search::SearchOutcome::view,
                    ),
                },
                original.snapshot(),
                resources.stop,
                resources.policy.content.max_output_bytes,
            )?;
            if let ImportedKnowledgeOpen::Published(reader) = &opened {
                reader.recheck()?;
            }
            bytes
        }
        Query::Contexts => {
            admission.recheck()?;
            let document = policy
                .registry
                .load()
                .map_err(ImportedKnowledgeReadError::Registry)?;
            let mut contexts = Vec::new();
            for (kind, _, entry) in document.entries() {
                resources.check(original)?;
                if kind != ArtifactType::Knowledge {
                    continue;
                }
                for installation in &entry.installations {
                    if installation.target != InstallationTarget::GlobalNative
                        || installation.alias.is_some()
                        || installation.entry_name.is_some()
                        || !matches!(
                            installation.backend,
                            InstalledBackend::Native | InstalledBackend::RemoteNative(_)
                        )
                        || !matches!(
                            installation.payload,
                            InstalledPayload::Original {
                                selection_binding: None
                            }
                        )
                    {
                        continue;
                    }
                    let source = &installation.original_source;
                    if source.artifact_type() != ArtifactType::Knowledge || source.id() != &entry.id
                    {
                        return Err(ImportedKnowledgeReadError::UnsupportedBinding.into());
                    }
                    if contexts.len() == resources.policy.content.max_collection_entries {
                        return Err(Failure::Invalid("response_limit"));
                    }
                    contexts.push(SourceKey::ImportedKnowledge {
                        artifact: ArtifactSelector {
                            id: entry.id.as_str().to_owned(),
                            version: source.version().as_str().to_owned(),
                            context_alias: None,
                        },
                    });
                }
            }
            admission.recheck()?;
            encode(
                &Contexts { contexts },
                original.snapshot(),
                resources.stop,
                resources.policy.content.max_output_bytes,
            )?
        }
        Query::Page(input) => {
            let SourceKey::ImportedKnowledge { artifact } = &input.source else {
                return Err(Failure::Invalid("invalid_arguments"));
            };
            let (id, version) = selected.ok_or(Failure::Invalid("invalid_arguments"))?;
            let opened = open_imported_knowledge(
                &admission,
                ImportedKnowledgeInput {
                    registry: &policy.registry,
                    id: &id,
                    version: &version,
                    context_alias: artifact.context_alias.as_deref(),
                },
                ImportedKnowledgeReadLimits {
                    directory: policy.directory,
                    index: foreground_knowledge::limits(resources.policy)?,
                },
            )
            .await?;
            match &opened {
                ImportedKnowledgeOpen::RegistryAbsent => encode(
                    &ReferenceBinding {
                        selection: None,
                        source: &input.source,
                        focused_source: None,
                        native: PageRead::RegistryAbsent,
                    },
                    original.snapshot(),
                    resources.stop,
                    resources.policy.content.max_output_bytes,
                )?,
                ImportedKnowledgeOpen::Published(reader) => {
                    let page = reader
                        .page(&input.slug, foreground_knowledge::limits(resources.policy)?)
                        .await?;
                    let focused_source = match &page {
                        KnowledgeSnapshotPage::Found { focused_source, .. } => {
                            focused_source.as_deref()
                        }
                        _ => None,
                    };
                    let bytes = encode(
                        &ReferenceBinding {
                            selection: None,
                            source: &input.source,
                            focused_source,
                            native: PageRead::Published {
                                generation: &reader.observation().generation,
                                page: foreground_knowledge::PageView(&page),
                            },
                        },
                        original.snapshot(),
                        resources.stop,
                        resources.policy.content.max_output_bytes,
                    )?;
                    reader.recheck()?;
                    bytes
                }
            }
        }
    };
    admission.recheck()?;
    resources.check(original)?;
    Ok(bytes)
}

/// Fixed route module: a request cannot substitute a broader module or grant.
pub(crate) async fn contexts<'a>(
    resources: Resources<'a>,
    module: ProjectReadModule,
    identity: Identity<'a>,
    backend: &'a BrokerProjectAccess,
    original: &'a RequestContext<'a>,
) -> Result<Vec<u8>, Failure> {
    resources.check(original)?;
    let operational_current = resources.current;
    let mut captures = Vec::with_capacity(resources.projects.len());
    for (index, selected) in resources.projects.iter().enumerate() {
        resources.check(original)?;
        if !resources.is_active(selected)? {
            continue;
        }
        let (paths, id, _) = selected;
        let scope = CapturedPublicationScope::project(
            paths,
            *id,
            resources.policy.scope,
            original.snapshot(),
        )
        .map_err(Failure::Physical)?;
        let binding = scope.binding().map_err(Failure::Physical)?;
        captures.push((index, scope, binding));
    }
    let (caller, verifier, provider) = identity;
    let authority = ProjectRecordAuthority::new(resources.auth.issuer());
    let mut admissions = Vec::new();
    let mut contexts = Vec::new();
    let mut admitted_indices = Vec::new();
    let mut current = None;
    for (index, scope, binding) in &captures {
        let selected = &resources.projects[*index];
        resources.require_active(selected)?;
        let (paths, id, version) = selected;
        let admission = match authority
            .admit_broker_read(BrokerRecordRead {
                caller: BrokerProjectCaller::Web(caller),
                verifier,
                backend,
                original,
                scope: BrokerProjectScope {
                    installation: resources.auth.installation(),
                    project: ProjectCapture {
                        scope: ScopeCapture {
                            live: scope,
                            record: binding,
                        },
                        version,
                    },
                    providers: &[],
                },
                provider,
                module,
            })
            .await
        {
            Ok(admission) => admission,
            Err(BrokerProjectError::Denied | BrokerProjectError::Authority(AuthError::Denied)) => {
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        if contexts.len() == resources.policy.content.max_collection_entries {
            return Err(Failure::Invalid("response_limit"));
        }
        admissions.push(admission);
        admitted_indices.push(*index);
        if Some(*index) == operational_current {
            current = Some(contexts.len());
        }
        contexts.push(codec::AdmittedSelection {
            project_dir: NativePath::new(paths.root().to_owned()).map_err(Failure::Physical)?,
            project_id: *id,
            project_version: version.version().to_owned(),
        });
    }
    #[derive(Serialize)]
    struct Response<'a> {
        contexts: &'a [codec::AdmittedSelection],
        current: Option<&'a codec::AdmittedSelection>,
        projects: Vec<crate::live_project_selections::ProjectName>,
    }
    for admission in &admissions {
        admission.recheck()?;
    }
    for index in &admitted_indices {
        resources.require_active(&resources.projects[*index])?;
    }
    let bytes = encode(
        &Response {
            current: current.map(|index| &contexts[index]),
            contexts: &contexts,
            projects: crate::live_project_selections::project_names(
                admitted_indices
                    .iter()
                    .map(|index| &resources.projects[*index]),
                original.snapshot(),
            )
            .map_err(Failure::Physical)?,
        },
        original.snapshot(),
        resources.stop,
        resources.policy.content.max_output_bytes,
    )?;
    for admission in &admissions {
        admission.recheck()?;
    }
    for index in &admitted_indices {
        resources.require_active(&resources.projects[*index])?;
    }
    resources.check(original)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use serde_json::{Value, json};
    use std::time::{Duration, Instant};
    fn limits() -> ContentLimits {
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_scalar_bytes: 4096,
            max_collection_entries: 256,
            max_nodes: 1,
            max_depth: 1,
        }
    }
    fn page() -> Value {
        json!({"input":{"source":{"kind":"imported_knowledge","artifact":{"id":"reference/library","version":"release/literal","context_alias":"literal alias"}},"slug":"literal page"}})
    }
    #[test]
    fn import_and_own_module_discovery_have_no_project_selector() {
        for operation in [
            "imported_knowledge_contexts",
            "memory_contexts",
            "knowledge_contexts",
        ] {
            let parsed = parse(br#"{"input":{}}"#, limits(), operation).unwrap();
            assert!(parsed.selection.project_dir.is_none());
            assert!(parsed.selection.project_id.is_none());
            assert!(parsed.selection.project_version.is_none());
            assert!(parse(br#"{"selection":null,"input":{}}"#, limits(), operation).is_err());
            assert!(parse(br#"{"input":{"module":"task"}}"#, limits(), operation).is_err());
        }
    }
    #[test]
    fn import_selector_keeps_literal_alias_version_and_closed_fields() {
        let bytes = serde_json::to_vec(&page()).unwrap();
        let Query::Page(query) =
            query(parse(&bytes, limits(), "imported_knowledge_page").unwrap()).unwrap()
        else {
            panic!("page query");
        };
        let SourceKey::ImportedKnowledge { artifact } = query.source else {
            panic!("exact import");
        };
        assert_eq!(artifact.version, "release/literal");
        assert_eq!(artifact.context_alias.as_deref(), Some("literal alias"));
        for field in ["context_alias", "id", "version"] {
            let mut body = page();
            body["input"]["source"]["artifact"][field] = Value::Null;
            assert!(
                parse(
                    &serde_json::to_vec(&body).unwrap(),
                    limits(),
                    "imported_knowledge_page"
                )
                .is_err()
            );
        }
        for body in [
            r#"{"input":{"source":{"kind":"imported_knowledge","artifact":{"id":"library","\u0069d":"other","version":"main"}},"slug":"page"}}"#,
            r#"{"input":{"source":{"kind":"project_knowledge"},"slug":"page"}}"#,
            r#"{"input":{"source":{"kind":"imported_knowledge","artifact":{"id":"library","version":"main","namespace":"invented"}},"slug":"page"}}"#,
        ] {
            assert!(parse(body.as_bytes(), limits(), "imported_knowledge_page").is_err());
        }
    }
    #[test]
    fn actual_scalar_and_frame_bounds_do_not_count_json_nodes() {
        let bytes = serde_json::to_vec(&page()).unwrap();
        let mut cap = limits();
        cap.max_input_bytes = bytes.len();
        assert!(parse(&bytes, cap, "imported_knowledge_page").is_ok());
        cap.max_input_bytes -= 1;
        assert!(parse(&bytes, cap, "imported_knowledge_page").is_err());
        let mut body = page();
        body["input"]["slug"] = json!("é".repeat(13));
        cap = limits();
        cap.max_scalar_bytes = 26;
        assert!(
            parse(
                &serde_json::to_vec(&body).unwrap(),
                cap,
                "imported_knowledge_page"
            )
            .is_ok()
        );
        body["input"]["slug"] = json!("é".repeat(14));
        assert!(
            parse(
                &serde_json::to_vec(&body).unwrap(),
                cap,
                "imported_knowledge_page"
            )
            .is_err()
        );
    }
    #[test]
    fn encoder_keeps_u64_without_float_and_checks_original_stop_and_physical_bytes() {
        let cancellation = Cancellation::default();
        let original = RequestContext::new(
            limits(),
            &cancellation,
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
        let stop = CancellationToken::new();
        let expected = u64::MAX.to_string();
        assert_eq!(
            encode(&u64::MAX, original.snapshot(), &stop, expected.len()).unwrap(),
            expected.as_bytes()
        );
        assert_eq!(
            encode(&u64::MAX, original.snapshot(), &stop, expected.len() - 1)
                .unwrap_err()
                .response()
                .1,
            "response_limit"
        );
        stop.cancel();
        assert_eq!(
            encode(&u64::MAX, original.snapshot(), &stop, expected.len())
                .unwrap_err()
                .response(),
            (axum::http::StatusCode::SERVICE_UNAVAILABLE, "stopped")
        );
        cancellation.cancel();
        assert_eq!(
            encode(
                &u64::MAX,
                original.snapshot(),
                &CancellationToken::new(),
                expected.len()
            )
            .unwrap_err()
            .response(),
            (
                axum::http::StatusCode::REQUEST_TIMEOUT,
                "request_cancelled_or_expired"
            )
        );
    }
    #[test]
    fn encoder_preserves_stop_and_original_causes_during_serialization() {
        struct InterruptDuring<F>(F);
        impl<F: Fn()> Serialize for InterruptDuring<F> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeTuple;
                let mut tuple = serializer.serialize_tuple(2)?;
                tuple.serialize_element("written prefix")?;
                (self.0)();
                tuple.serialize_element(&u64::MAX)?;
                tuple.end()
            }
        }
        let cancellation = Cancellation::default();
        let original = RequestContext::new(
            limits(),
            &cancellation,
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
        let stop = CancellationToken::new();
        assert_eq!(
            encode(
                &InterruptDuring(|| stop.cancel()),
                original.snapshot(),
                &stop,
                64
            )
            .unwrap_err()
            .response(),
            (axum::http::StatusCode::SERVICE_UNAVAILABLE, "stopped")
        );
        assert_eq!(
            encode(
                &InterruptDuring(|| cancellation.cancel()),
                original.snapshot(),
                &CancellationToken::new(),
                64
            )
            .unwrap_err()
            .response(),
            (
                axum::http::StatusCode::REQUEST_TIMEOUT,
                "request_cancelled_or_expired"
            )
        );
    }
}
