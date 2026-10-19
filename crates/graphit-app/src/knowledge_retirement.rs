//! Authorized selection cleanup followed by one Knowledge corpus retirement.
//! Shared Core version facts and HTTP/CLI host routing remain with their owners.
#![cfg(target_os = "linux")]
use graphit_auth::{
    AuthError, AuthenticatedCaller, Obligation, Permission, ProjectRecordAuthority,
    ProjectWriteAdmission, ProjectWriteOperation, ScopeInput,
};
use graphit_core::artifact_store::StoreOwner;
use graphit_daemon::supervisor::{ProjectSupervisors, SupervisionKey};
use graphit_data::{
    knowledge_index::{
        KnowledgeBranchRetirement, KnowledgeIndexError, KnowledgeRetirementFailure,
        KnowledgeRetirementLimits, retire_native_knowledge_index,
    },
    lance_branch_store::BranchError,
};
use graphit_hub::policy::RequestContext;
use std::{error::Error, fmt, io};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeRetirementPhase {
    Admission,
    Stop,
    Index,
}

/// These results describe Knowledge only; they do not remove a Core version fact.
#[derive(Debug)]
pub struct KnowledgeSelectionRetirement {
    pub index: KnowledgeBranchRetirement,
}

pub enum KnowledgeSelectionRetirementError {
    Admission(Box<KnowledgeIndexError>),
    Stop(io::Error),
    Index(KnowledgeRetirementFailure),
}
impl fmt::Debug for KnowledgeSelectionRetirementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Admission(_) => "admission",
            Self::Stop(_) => "selection cleanup",
            Self::Index(_) => "index retirement",
        })
    }
}
impl fmt::Display for KnowledgeSelectionRetirementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Knowledge selection retirement: {self:?}")
    }
}
impl Error for KnowledgeSelectionRetirementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Admission(error) => error.as_ref(),
            Self::Stop(error) => error,
            Self::Index(error) => error,
        })
    }
}

/// A failed boundary keeps actual earlier results without replay or rollback.
pub struct KnowledgeSelectionRetirementFailure {
    phase: KnowledgeRetirementPhase,
    error: Box<KnowledgeSelectionRetirementError>,
}
impl KnowledgeSelectionRetirementFailure {
    pub fn phase(&self) -> KnowledgeRetirementPhase {
        self.phase
    }
    pub fn error(&self) -> &KnowledgeSelectionRetirementError {
        &self.error
    }
    pub fn index(&self) -> Option<&KnowledgeBranchRetirement> {
        match self.error.as_ref() {
            KnowledgeSelectionRetirementError::Index(error) => error.confirmed(),
            _ => None,
        }
    }
}
impl fmt::Debug for KnowledgeSelectionRetirementFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KnowledgeSelectionRetirementFailure")
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for KnowledgeSelectionRetirementFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Knowledge selection retirement failed")
    }
}
impl Error for KnowledgeSelectionRetirementFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.error.as_ref())
    }
}

fn failure(
    phase: KnowledgeRetirementPhase,
    error: KnowledgeSelectionRetirementError,
) -> KnowledgeSelectionRetirementFailure {
    KnowledgeSelectionRetirementFailure {
        phase,
        error: Box::new(error),
    }
}
fn admission_failure(
    phase: KnowledgeRetirementPhase,
    error: impl Into<KnowledgeIndexError>,
) -> KnowledgeSelectionRetirementFailure {
    failure(
        phase,
        KnowledgeSelectionRetirementError::Admission(Box::new(error.into())),
    )
}
fn configure(
    caller: &AuthenticatedCaller<'_>,
    original: &RequestContext<'_>,
) -> Result<(), KnowledgeIndexError> {
    original.snapshot().check()?;
    if caller.authorize(Permission::Configure)? != Obligation::Unconditional {
        return Err(AuthError::Denied.into());
    }
    Ok(())
}
fn after_join(
    caller: &AuthenticatedCaller<'_>,
    admission: &ProjectWriteAdmission<'_>,
    original: &RequestContext<'_>,
) -> Result<(), KnowledgeIndexError> {
    configure(caller, original)?;
    admission.recheck()?;
    Ok(())
}

/// Configure authorizes generic supervisor cleanup; Knowledge authorizes its corpora.
/// The caller retains the registry while this operation awaits its actual owners.
/// Await cleanup even if the Original expires; the subsequent check prevents Data.
/// Drop during a pending join leaves SDK ownership in that same borrowed registry.
pub async fn retire_knowledge_selection<'a>(
    authority: &'a ProjectRecordAuthority<'_>,
    caller: &'a AuthenticatedCaller<'a>,
    scope: ScopeInput<'a>,
    supervisors: &mut ProjectSupervisors,
    limits: KnowledgeRetirementLimits,
    original: &'a RequestContext<'a>,
) -> Result<KnowledgeSelectionRetirement, KnowledgeSelectionRetirementFailure> {
    use KnowledgeRetirementPhase as Phase;
    configure(caller, original).map_err(|e| admission_failure(Phase::Admission, e))?;
    let admission = authority
        .admit_authenticated_write(caller, original, scope, ProjectWriteOperation::Knowledge)
        .map_err(|e| admission_failure(Phase::Admission, e))?;
    let project = admission
        .project()
        .map_err(|e| admission_failure(Phase::Admission, e))?;
    if project.version.lance_branch().is_empty() {
        return Err(admission_failure(
            Phase::Admission,
            KnowledgeIndexError::Branch(BranchError::DefaultSnapshot),
        ));
    }
    let StoreOwner::Project { project_id } = project.scope.live.owner() else {
        return Err(admission_failure(
            Phase::Admission,
            KnowledgeIndexError::InvalidField("project_owner"),
        ));
    };
    // These are ProjectContext::key's fields, derived without cloning VersionContext.
    let key = SupervisionKey {
        id: *project_id,
        checkout: project.version.scope().to_owned(),
        version: project.version.version().to_owned(),
    };
    supervisors
        .stop_selection(&key)
        .await
        .map_err(|e| failure(Phase::Stop, KnowledgeSelectionRetirementError::Stop(e)))?;
    after_join(caller, &admission, original).map_err(|e| admission_failure(Phase::Index, e))?;
    let index = retire_native_knowledge_index(original.snapshot(), &admission, limits)
        .await
        .map_err(|e| failure(Phase::Index, KnowledgeSelectionRetirementError::Index(e)))?;
    Ok(KnowledgeSelectionRetirement { index })
}
