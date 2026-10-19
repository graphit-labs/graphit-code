//! Compose the approved local PROJECT Now wire from real authenticated head readers.
//!
//! Admissions retain the original caller and request. Each corpus has its own SDK
//! pin; this result does not grant authority or promise a cross-corpus transaction.
//!
//! ```
//! use graphit_daemon::project_now::{NowErrorCode, NowLocal, SectionAvailability};
//! fn serialize_observed(now: &NowLocal) -> serde_json::Value {
//!     let _project = now.project_id();
//!     let _time = now.generated_at();
//!     let tasks = now.sections().tasks();
//!     let _metadata = (tasks.availability(), tasks.total(), tasks.has_more(), tasks.error());
//!     serde_json::to_value(now).unwrap()
//! }
//! assert_eq!(serde_json::to_string(&NowErrorCode::Forbidden).unwrap(), "\"forbidden\"");
//! assert_eq!(serde_json::to_string(&SectionAvailability::NoCorpus).unwrap(), "\"no_corpus\"");
//! ```
#![cfg(any(target_os = "linux", target_os = "macos", windows))]

use graphit_auth::{
    AuthError, AuthenticatedCaller, ProjectCapture, ProjectReadAdmission, ProjectReadModule,
    ProjectRecordAuthority, ScopeInput, VerifiedRoleGrant,
};
use graphit_core::{
    artifact_store::{ScopeContext, StoreOwner},
    file_io::SnapshotContext,
    project_identity::ProjectId,
};
use graphit_data::knowledge_index::{
    KnowledgeHead, KnowledgeHeadRead, KnowledgeHeadReadLimits, KnowledgeIndexError,
    read_native_knowledge_heads,
};
use graphit_hub::policy::RequestContext;
use graphit_records::{
    gates::TaskStatus,
    memory_store::{
        MemoryActivityHead, MemoryHeadRead, MemoryHeadReadLimits, MemoryId, MemoryStoreError,
        NativeMemoryStore,
    },
    task_spec::TaskId,
    task_store::{
        NativeTaskStore, Ownership, SessionHead, SessionId, TaskActivityHead, TaskHeadObservation,
        TaskStoreError, TaskStoreReadLimits,
    },
};
use serde::Serialize;
use std::{
    error::Error,
    fmt, io,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

const SAFE_COUNT_MAX: u64 = 9_007_199_254_740_991;
const SECTION_ITEMS: usize = 20;

/// Explicit producer limits; none is a display limit or an authority decision.
#[derive(Clone, Copy)]
pub struct ProjectNowLimits {
    pub task: TaskStoreReadLimits,
    pub memory: MemoryHeadReadLimits,
    pub knowledge: KnowledgeHeadReadLimits,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionAvailability {
    Available,
    NoCorpus,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NowErrorCode {
    Forbidden,
    Unavailable,
    InvalidData,
    LimitExceeded,
    CountsUnavailable,
}

/// A read-only, finite wire section. Failed sections cannot retain items or counts.
#[derive(Clone, Debug, Serialize)]
pub struct NowSection<T> {
    availability: SectionAvailability,
    items: Vec<T>,
    total: Option<u64>,
    has_more: Option<bool>,
    error: Option<NowErrorCode>,
}
impl<T> NowSection<T> {
    pub fn availability(&self) -> SectionAvailability {
        self.availability
    }
    pub fn items(&self) -> &[T] {
        &self.items
    }
    pub fn total(&self) -> Option<u64> {
        self.total
    }
    pub fn has_more(&self) -> Option<bool> {
        self.has_more
    }
    pub fn error(&self) -> Option<NowErrorCode> {
        self.error
    }
    fn failed(error: NowErrorCode) -> Self {
        Self {
            availability: SectionAvailability::Failed,
            items: vec![],
            total: None,
            has_more: None,
            error: Some(error),
        }
    }
    fn absent() -> Self {
        Self {
            availability: SectionAvailability::NoCorpus,
            items: vec![],
            total: Some(0),
            has_more: Some(false),
            error: None,
        }
    }
    fn available(items: Vec<T>, total: u64) -> Self {
        let has_more = u128::from(total) > items.len() as u128;
        Self {
            availability: SectionAvailability::Available,
            items,
            total: Some(total),
            has_more: Some(has_more),
            error: None,
        }
    }
}

/// Allowlisted data, deliberately separate from the stored Task object.
#[derive(Clone, Debug, Serialize)]
pub struct NowTask {
    pub id: TaskId,
    pub session_id: SessionId,
    pub title: String,
    pub owner: String,
    pub lease_expires_at: String,
    pub updated_at: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct NowSession {
    pub id: SessionId,
    pub title: String,
    pub owner: String,
    pub lease_expires_at: String,
    pub updated_at: String,
    pub completed_tasks: u64,
    pub total_tasks: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct NowMemory {
    pub id: MemoryId,
    pub title: String,
    pub updated_by: String,
    pub updated_at: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct NowKnowledge {
    pub slug: String,
    pub title: String,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NowSections {
    tasks: NowSection<NowTask>,
    sessions: NowSection<NowSession>,
    memories: NowSection<NowMemory>,
    knowledge: NowSection<NowKnowledge>,
}
impl NowSections {
    pub fn tasks(&self) -> &NowSection<NowTask> {
        &self.tasks
    }
    pub fn sessions(&self) -> &NowSection<NowSession> {
        &self.sessions
    }
    pub fn memories(&self) -> &NowSection<NowMemory> {
        &self.memories
    }
    pub fn knowledge(&self) -> &NowSection<NowKnowledge> {
        &self.knowledge
    }
}

/// Owned output data. Scope, version, SDK handles and authority stay internal.
#[derive(Clone, Debug, Serialize)]
pub struct NowLocal {
    project_id: ProjectId,
    generated_at: String,
    sections: NowSections,
}
impl NowLocal {
    pub fn project_id(&self) -> ProjectId {
        self.project_id
    }
    pub fn generated_at(&self) -> &str {
        &self.generated_at
    }
    pub fn sections(&self) -> &NowSections {
        &self.sections
    }
}

/// Whole-operation failures retain the local cause, without a serializable wire.
#[derive(Debug)]
pub enum ProjectNowError {
    Request(io::Error),
    Scope(io::Error),
    Clock(io::Error),
}
impl fmt::Display for ProjectNowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Request(_) => "Now original request failed",
            Self::Scope(_) => "Now original project identity failed",
            Self::Clock(_) => "Now observation clock failed",
        })
    }
}
impl Error for ProjectNowError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Request(e) | Self::Scope(e) | Self::Clock(e) => e,
        })
    }
}

struct Observation {
    generated_at: String,
    milliseconds: u64,
}
impl Observation {
    fn at(time: SystemTime) -> io::Result<Self> {
        let elapsed = time
            .duration_since(UNIX_EPOCH)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let milliseconds = u64::try_from(elapsed.as_millis())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Self {
            generated_at: system_time_ns(time)?,
            milliseconds,
        })
    }
}
fn system_time_ns(time: SystemTime) -> io::Result<String> {
    let (duration, negative) = match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => (duration, false),
        Err(error) => (error.duration(), true),
    };
    let nanos = i128::try_from(duration.as_nanos())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let value = if negative {
        nanos.checked_neg().ok_or(io::ErrorKind::InvalidData)?
    } else {
        nanos
    };
    Ok(value.to_string())
}
fn milliseconds_ns(time: u64) -> String {
    // Every u64 millisecond value fits in u128 nanoseconds.
    u128::from(time)
        .checked_mul(1_000_000)
        .expect("u64 milliseconds fit in u128 nanoseconds")
        .to_string()
}
fn safe_count(value: usize) -> Result<u64, NowErrorCode> {
    u64::try_from(value)
        .ok()
        .filter(|n| *n <= SAFE_COUNT_MAX)
        .ok_or(NowErrorCode::LimitExceeded)
}
fn active(status: TaskStatus, ownership: &Ownership, observation: u64) -> bool {
    status == TaskStatus::InProgress
        && ownership.owner.is_some()
        && ownership
            .lease_expires_at
            .is_some_and(|end| end > observation)
}

fn auth_code(error: &AuthError) -> NowErrorCode {
    match error {
        AuthError::Denied | AuthError::Expired => NowErrorCode::Forbidden,
        AuthError::Limit => NowErrorCode::LimitExceeded,
        AuthError::Invalid
        | AuthError::InvalidSetting(_)
        | AuthError::Request
        | AuthError::Entropy
        | AuthError::State
        | AuthError::Io => NowErrorCode::Unavailable,
    }
}
fn task_code(error: &TaskStoreError) -> NowErrorCode {
    match error {
        TaskStoreError::Auth(e) => auth_code(e),
        TaskStoreError::WrongAdmission | TaskStoreError::DifferentRequest => {
            NowErrorCode::Forbidden
        }
        TaskStoreError::Invalid(_)
        | TaskStoreError::MissingSession
        | TaskStoreError::IncompleteCorpus
        | TaskStoreError::VectorDistance(_) => NowErrorCode::InvalidData,
        TaskStoreError::CountsUnavailable => NowErrorCode::CountsUnavailable,
        TaskStoreError::Artifact(_)
        | TaskStoreError::Request(_)
        | TaskStoreError::Arrow(_)
        | TaskStoreError::Json(_)
        | TaskStoreError::Specification(_)
        | TaskStoreError::Reference(_)
        | TaskStoreError::Gate(_)
        | TaskStoreError::Store(_)
        | TaskStoreError::Branch(_)
        | TaskStoreError::Commit(_)
        | TaskStoreError::WriterAcquisition(_)
        | TaskStoreError::IdempotencyConflict
        | TaskStoreError::MissingTask
        | TaskStoreError::InvalidCursor
        | TaskStoreError::AlreadyClaimed
        | TaskStoreError::LeaseExpired
        | TaskStoreError::FenceMismatch
        | TaskStoreError::RevisionConflict
        | TaskStoreError::Random(_)
        | TaskStoreError::Confirmed { .. }
        | TaskStoreError::LexicalIndexConfirmed { .. } => NowErrorCode::Unavailable,
    }
}
fn memory_code(error: &MemoryStoreError) -> NowErrorCode {
    match error {
        MemoryStoreError::Auth(e) => auth_code(e),
        MemoryStoreError::WrongAdmission | MemoryStoreError::DifferentRequest => {
            NowErrorCode::Forbidden
        }
        MemoryStoreError::Invalid(_)
        | MemoryStoreError::IncompleteCorpus
        | MemoryStoreError::VectorDistance(_) => NowErrorCode::InvalidData,
        MemoryStoreError::Limit(_) => NowErrorCode::LimitExceeded,
        MemoryStoreError::Artifact(_)
        | MemoryStoreError::Request(_)
        | MemoryStoreError::Arrow(_)
        | MemoryStoreError::Json(_)
        | MemoryStoreError::References(_)
        | MemoryStoreError::Store(_)
        | MemoryStoreError::Branch(_)
        | MemoryStoreError::Commit(_)
        | MemoryStoreError::WriterAcquisition(_)
        | MemoryStoreError::IdempotencyConflict
        | MemoryStoreError::NotFound
        | MemoryStoreError::RevisionConflict { .. }
        | MemoryStoreError::NoChange
        | MemoryStoreError::LexicalIndexUnavailable(_)
        | MemoryStoreError::IndexConfirmed { .. }
        | MemoryStoreError::UserConfirmed { .. }
        | MemoryStoreError::UserIndexConfirmed { .. }
        | MemoryStoreError::UserEmbeddingConfirmed { .. }
        | MemoryStoreError::EmbeddingConfirmed { .. }
        | MemoryStoreError::Confirmed { .. } => NowErrorCode::Unavailable,
    }
}
fn knowledge_code(error: &KnowledgeIndexError) -> NowErrorCode {
    match error {
        KnowledgeIndexError::Auth(e) => auth_code(e),
        KnowledgeIndexError::InputStream(stream) => {
            let mut cause: Option<&(dyn Error + 'static)> = Some(stream.as_ref());
            while let Some(error) = cause {
                if let Some(index) = error.downcast_ref::<KnowledgeIndexError>() {
                    return knowledge_code(index);
                }
                if let Some(auth) = error.downcast_ref::<AuthError>() {
                    return auth_code(auth);
                }
                cause = error.source();
            }
            NowErrorCode::Unavailable
        }
        KnowledgeIndexError::WrongAdmission | KnowledgeIndexError::DifferentRequest => {
            NowErrorCode::Forbidden
        }
        KnowledgeIndexError::InvalidField(_) | KnowledgeIndexError::DuplicateSlug => {
            NowErrorCode::InvalidData
        }
        KnowledgeIndexError::LimitExceeded(_) | KnowledgeIndexError::Overflow(_) => {
            NowErrorCode::LimitExceeded
        }
        KnowledgeIndexError::Artifact(_)
        | KnowledgeIndexError::Request(_)
        | KnowledgeIndexError::Projection(_)
        | KnowledgeIndexError::Arrow(_)
        | KnowledgeIndexError::ReferenceSerialization(_)
        | KnowledgeIndexError::Store(_)
        | KnowledgeIndexError::Branch(_)
        | KnowledgeIndexError::WriterAcquisition(_)
        | KnowledgeIndexError::InitialTargetChanged
        | KnowledgeIndexError::PartialDefault { .. } => NowErrorCode::Unavailable,
    }
}

fn check_original(original: &SnapshotContext) -> Result<(), ProjectNowError> {
    original.check().map_err(ProjectNowError::Request)
}
fn project_identity(
    original: &SnapshotContext,
    project: &ProjectCapture<'_>,
) -> Result<ProjectId, ProjectNowError> {
    check_original(original)?;
    let outcome = (|| {
        let StoreOwner::Project { project_id } = project.scope.live.owner() else {
            return Err(io::ErrorKind::InvalidInput.into());
        };
        if project.version.scope() != project.scope.live.owner_root() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let context = ScopeContext::from_snapshot(original);
        project.scope.live.revalidate_bounded(&context)?;
        if !project
            .scope
            .record
            .matches_scope_bounded(project.scope.live, &context)?
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(*project_id)
    })();
    check_original(original)?;
    outcome.map_err(ProjectNowError::Scope)
}
fn check_admission(
    original: &Arc<SnapshotContext>,
    expected: &ProjectCapture<'_>,
    admission: &ProjectReadAdmission<'_>,
    module: ProjectReadModule,
) -> Result<(), NowErrorCode> {
    if admission.module() != module {
        return Err(NowErrorCode::Forbidden);
    }
    let (actual, request) = admission.project_and_request().map_err(|e| auth_code(&e))?;
    if !Arc::ptr_eq(original, request.snapshot()) {
        return Err(NowErrorCode::Forbidden);
    }
    if actual.scope.live.owner() != expected.scope.live.owner()
        || actual.scope.live.brand().key() != expected.scope.live.brand().key()
        || actual.scope.live.owner_root() != expected.scope.live.owner_root()
        || actual.scope.live.runtime_root() != expected.scope.live.runtime_root()
        || actual.version != expected.version
    {
        return Err(NowErrorCode::Forbidden);
    }
    Ok(())
}

fn tasks_section(mut heads: Vec<TaskActivityHead>, observation: u64) -> NowSection<NowTask> {
    heads.retain(|h| active(h.status, &h.ownership, observation));
    let total = match safe_count(heads.len()) {
        Ok(n) => n,
        Err(e) => return NowSection::failed(e),
    };
    heads.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.id.get().cmp(&b.id.get()))
    });
    let items = heads
        .into_iter()
        .take(SECTION_ITEMS)
        .map(|h| NowTask {
            id: h.id,
            session_id: h.session_id,
            title: h.title,
            owner: h.ownership.owner.expect("active owner"),
            lease_expires_at: milliseconds_ns(h.ownership.lease_expires_at.expect("active lease")),
            updated_at: milliseconds_ns(h.updated_at),
        })
        .collect();
    NowSection::available(items, total)
}
fn sessions_section(mut heads: Vec<SessionHead>, observation: u64) -> NowSection<NowSession> {
    heads.retain(|h| active(h.status, &h.ownership, observation));
    let total = match safe_count(heads.len()) {
        Ok(n) => n,
        Err(e) => return NowSection::failed(e),
    };
    // Validate the full active section before selecting its display rows.
    if heads
        .iter()
        .any(|h| safe_count(h.completed_tasks).is_err() || safe_count(h.total_tasks).is_err())
    {
        return NowSection::failed(NowErrorCode::LimitExceeded);
    }
    heads.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.id.get().cmp(&b.id.get()))
    });
    let items = heads
        .into_iter()
        .take(SECTION_ITEMS)
        .map(|h| NowSession {
            id: h.id,
            title: h.title,
            owner: h.ownership.owner.expect("active owner"),
            lease_expires_at: milliseconds_ns(h.ownership.lease_expires_at.expect("active lease")),
            updated_at: milliseconds_ns(h.updated_at),
            completed_tasks: safe_count(h.completed_tasks).expect("checked count"),
            total_tasks: safe_count(h.total_tasks).expect("checked count"),
        })
        .collect();
    NowSection::available(items, total)
}
fn memories_section(mut heads: Vec<MemoryActivityHead>) -> NowSection<NowMemory> {
    let total = match safe_count(heads.len()) {
        Ok(n) => n,
        Err(e) => return NowSection::failed(e),
    };
    heads.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    let items = heads
        .into_iter()
        .take(SECTION_ITEMS)
        .map(|h| NowMemory {
            id: h.id,
            title: h.title,
            updated_by: h.updated_by,
            updated_at: milliseconds_ns(h.updated_at),
        })
        .collect();
    NowSection::available(items, total)
}
fn knowledge_section(mut heads: Vec<KnowledgeHead>) -> NowSection<NowKnowledge> {
    let total = match safe_count(heads.len()) {
        Ok(n) => n,
        Err(e) => return NowSection::failed(e),
    };
    heads.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.slug.cmp(&b.slug))
    });
    let items: Result<Vec<_>, _> = heads
        .into_iter()
        .take(SECTION_ITEMS)
        .map(|h| {
            let updated_at = h.updated_at.map(system_time_ns).transpose()?;
            Ok::<_, io::Error>(NowKnowledge {
                slug: h.slug,
                title: h.title,
                updated_at,
            })
        })
        .collect();
    match items {
        Ok(items) => NowSection::available(items, total),
        Err(_) => NowSection::failed(NowErrorCode::LimitExceeded),
    }
}
fn task_sections(
    observation: TaskHeadObservation,
    at: u64,
) -> (NowSection<NowTask>, NowSection<NowSession>) {
    match observation {
        TaskHeadObservation::NoCorpus => (NowSection::absent(), NowSection::absent()),
        TaskHeadObservation::Published {
            tasks, sessions, ..
        } => (
            match tasks {
                Ok(heads) => tasks_section(heads, at),
                Err(e) => NowSection::failed(task_code(&e)),
            },
            match sessions {
                Ok(heads) => sessions_section(heads, at),
                Err(e) => NowSection::failed(task_code(&e)),
            },
        ),
    }
}

fn finish(
    original: &Arc<SnapshotContext>,
    project: &ProjectCapture<'_>,
    task: Option<&ProjectReadAdmission<'_>>,
    memory: Option<&ProjectReadAdmission<'_>>,
    knowledge: Option<&ProjectReadAdmission<'_>>,
    mut snapshot: NowLocal,
) -> Result<NowLocal, ProjectNowError> {
    check_original(original)?;
    if let Some(admission) = task
        && let Err(error) = check_admission(original, project, admission, ProjectReadModule::Task)
    {
        check_original(original)?;
        snapshot.sections.tasks = NowSection::failed(error);
        snapshot.sections.sessions = NowSection::failed(error);
    }
    if let Some(admission) = memory
        && let Err(error) = check_admission(original, project, admission, ProjectReadModule::Memory)
    {
        check_original(original)?;
        snapshot.sections.memories = NowSection::failed(error);
    }
    if let Some(admission) = knowledge
        && let Err(error) =
            check_admission(original, project, admission, ProjectReadModule::Knowledge)
    {
        check_original(original)?;
        snapshot.sections.knowledge = NowSection::failed(error);
    }
    if project_identity(original, project)? != snapshot.project_id {
        return Err(ProjectNowError::Scope(io::ErrorKind::InvalidData.into()));
    }
    check_original(original)?;
    Ok(snapshot)
}

/// Read only the selected PROJECT/version through the existing Runtime/Local caller.
/// No input is converted into a role or grant; all module admissions come from Auth.
pub async fn read_project_now<'a>(
    authority: &'a ProjectRecordAuthority<'_>,
    caller: &'a AuthenticatedCaller<'a>,
    request: &'a RequestContext<'a>,
    scope: ScopeInput<'a>,
    limits: ProjectNowLimits,
) -> Result<NowLocal, ProjectNowError> {
    let original = request.snapshot();
    check_original(original)?;
    let project = scope
        .project
        .ok_or_else(|| ProjectNowError::Scope(io::ErrorKind::InvalidInput.into()))?;
    let project_id = project_identity(original, &project)?;
    let observation = Observation::at(SystemTime::now()).map_err(ProjectNowError::Clock)?;
    let task = authority.admit_authenticated_read(caller, request, scope, ProjectReadModule::Task);
    check_original(original)?;
    let memory =
        authority.admit_authenticated_read(caller, request, scope, ProjectReadModule::Memory);
    check_original(original)?;
    let knowledge =
        authority.admit_authenticated_read(caller, request, scope, ProjectReadModule::Knowledge);
    check_original(original)?;

    read_admitted_now(
        original,
        project,
        project_id,
        observation,
        limits,
        [task, memory, knowledge],
    )
    .await
}

/// Recheck a channel's genuine observation without changing its observation time.
/// Failed fresh admissions mask only their own sections; no data is read here.
pub(crate) fn reauthorize_project_now<'a>(
    authority: &'a ProjectRecordAuthority<'_>,
    caller: &'a AuthenticatedCaller<'a>,
    request: &'a RequestContext<'a>,
    scope: ScopeInput<'a>,
    observed: &NowLocal,
) -> Result<NowLocal, ProjectNowError> {
    let original = request.snapshot();
    check_original(original)?;
    let project = scope
        .project
        .ok_or_else(|| ProjectNowError::Scope(io::ErrorKind::InvalidInput.into()))?;
    if project_identity(original, &project)? != observed.project_id {
        return Err(ProjectNowError::Scope(io::ErrorKind::InvalidData.into()));
    }
    let task = authority.admit_authenticated_read(caller, request, scope, ProjectReadModule::Task);
    check_original(original)?;
    let memory =
        authority.admit_authenticated_read(caller, request, scope, ProjectReadModule::Memory);
    check_original(original)?;
    let knowledge =
        authority.admit_authenticated_read(caller, request, scope, ProjectReadModule::Knowledge);
    check_original(original)?;
    let mut snapshot = observed.clone();
    if let Err(error) = &task {
        snapshot.sections.tasks = NowSection::failed(auth_code(error));
        snapshot.sections.sessions = NowSection::failed(auth_code(error));
    }
    if let Err(error) = &memory {
        snapshot.sections.memories = NowSection::failed(auth_code(error));
    }
    if let Err(error) = &knowledge {
        snapshot.sections.knowledge = NowSection::failed(auth_code(error));
    }
    finish(
        original,
        &project,
        task.as_ref().ok(),
        memory.as_ref().ok(),
        knowledge.as_ref().ok(),
        snapshot,
    )
}

/// Read local heads using a Role grant verified for this same request.
/// Each module keeps its own authorization; this does not authorize remote Presence.
/// ```
/// use graphit_auth::{ProjectRecordAuthority, ScopeInput, VerifiedRoleGrant};
/// use graphit_daemon::project_now::{NowLocal, ProjectNowError, ProjectNowLimits, read_project_now_role};
/// use graphit_hub::policy::RequestContext;
/// async fn read_role<'a>(
///     authority: &'a ProjectRecordAuthority<'_>, role: &'a VerifiedRoleGrant<'a>,
///     request: &'a RequestContext<'a>, scope: ScopeInput<'a>, limits: ProjectNowLimits,
/// ) -> Result<NowLocal, ProjectNowError> {
///     read_project_now_role(authority, role, request, scope, limits).await
/// }
/// ```
pub async fn read_project_now_role<'a>(
    authority: &'a ProjectRecordAuthority<'_>,
    role: &'a VerifiedRoleGrant<'a>,
    request: &'a RequestContext<'a>,
    scope: ScopeInput<'a>,
    limits: ProjectNowLimits,
) -> Result<NowLocal, ProjectNowError> {
    let original = request.snapshot();
    check_original(original)?;
    let project = scope
        .project
        .ok_or_else(|| ProjectNowError::Scope(io::ErrorKind::InvalidInput.into()))?;
    let project_id = project_identity(original, &project)?;
    let observation = Observation::at(SystemTime::now()).map_err(ProjectNowError::Clock)?;
    let task = authority.admit_read(role, request, ProjectReadModule::Task);
    check_original(original)?;
    let memory = authority.admit_read(role, request, ProjectReadModule::Memory);
    check_original(original)?;
    let knowledge = authority.admit_read(role, request, ProjectReadModule::Knowledge);
    check_original(original)?;
    read_admitted_now(
        original,
        project,
        project_id,
        observation,
        limits,
        [task, memory, knowledge],
    )
    .await
}

#[tracing::instrument(level = "debug", skip_all)]
async fn read_admitted_now(
    original: &Arc<SnapshotContext>,
    project: ProjectCapture<'_>,
    project_id: ProjectId,
    observation: Observation,
    limits: ProjectNowLimits,
    [task, memory, knowledge]: [Result<ProjectReadAdmission<'_>, AuthError>; 3],
) -> Result<NowLocal, ProjectNowError> {
    tracing::debug!("Task begin");
    let (tasks, sessions) = match &task {
        Err(error) => (
            NowSection::failed(auth_code(error)),
            NowSection::failed(auth_code(error)),
        ),
        Ok(admission) => {
            tracing::debug!("Task admission begin");
            let checked = check_admission(original, &project, admission, ProjectReadModule::Task);
            tracing::debug!("Task admission end");
            match checked {
                Err(error) => (NowSection::failed(error), NowSection::failed(error)),
                Ok(()) => {
                    let result =
                        NativeTaskStore::observe_heads(original, admission, limits.task).await;
                    check_original(original)?;
                    match result {
                        Ok(heads) => task_sections(heads, observation.milliseconds),
                        Err(error) => (
                            NowSection::failed(task_code(&error)),
                            NowSection::failed(task_code(&error)),
                        ),
                    }
                }
            }
        }
    };
    tracing::debug!("Task end");
    check_original(original)?;
    tracing::debug!("Memory begin");
    let memories = match &memory {
        Err(error) => NowSection::failed(auth_code(error)),
        Ok(admission) => {
            tracing::debug!("Memory admission begin");
            let checked = check_admission(original, &project, admission, ProjectReadModule::Memory);
            tracing::debug!("Memory admission end");
            match checked {
                Err(error) => NowSection::failed(error),
                Ok(()) => {
                    let result =
                        NativeMemoryStore::observe_heads(original, admission, limits.memory).await;
                    check_original(original)?;
                    match result {
                        Ok(MemoryHeadRead::NoCorpus) => NowSection::absent(),
                        Ok(MemoryHeadRead::Published { heads, .. }) => memories_section(heads),
                        Err(error) => NowSection::failed(memory_code(&error)),
                    }
                }
            }
        }
    };
    tracing::debug!("Memory end");
    check_original(original)?;
    tracing::debug!("Knowledge begin");
    let knowledge_section = match &knowledge {
        Err(error) => NowSection::failed(auth_code(error)),
        Ok(admission) => {
            tracing::debug!("Knowledge admission begin");
            let checked =
                check_admission(original, &project, admission, ProjectReadModule::Knowledge);
            tracing::debug!("Knowledge admission end");
            match checked {
                Err(error) => NowSection::failed(error),
                Ok(()) => {
                    let result =
                        read_native_knowledge_heads(original, admission, limits.knowledge).await;
                    check_original(original)?;
                    match result {
                        Ok(KnowledgeHeadRead::NoCorpus) => NowSection::absent(),
                        Ok(KnowledgeHeadRead::Published { heads, .. }) => knowledge_section(heads),
                        Err(error) => NowSection::failed(knowledge_code(&error)),
                    }
                }
            }
        }
    };
    tracing::debug!("Knowledge end");
    finish(
        original,
        &project,
        task.as_ref().ok(),
        memory.as_ref().ok(),
        knowledge.as_ref().ok(),
        NowLocal {
            project_id,
            generated_at: observation.generated_at,
            sections: NowSections {
                tasks,
                sessions,
                memories,
                knowledge: knowledge_section,
            },
        },
    )
}

#[cfg(test)]
pub(crate) mod tests;
