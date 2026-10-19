//! Pure projection of authored references into qualified logical namespaces.
//!
//! Text remains borrowed from its owner, independently of the temporary entry
//! slice. This projection performs no lookup, access check or store mutation.

use graphit_core::{file_io::SnapshotContext, project_identity::ProjectId};
use std::{error::Error, fmt, io};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogicalScope<'text> {
    Project(ProjectId),
    User(&'text str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceNamespace<'text> {
    pub scope: LogicalScope<'text>,
    pub context: Option<&'text str>,
}

#[derive(Clone, Copy, Debug)]
pub enum AuthoredReferencesField<'entries, 'text> {
    Omitted,
    Null,
    NonSequence,
    Sequence(&'entries [AuthoredReferenceEntry<'text>]),
}

#[derive(Clone, Copy, Debug)]
pub enum AuthoredReferenceEntry<'text> {
    InvalidShape,
    Tuple(ReferenceCandidate<'text>),
}

/// Optional `None` represents omission only; null/non-string fields are InvalidShape.
#[derive(Clone, Copy, Debug)]
pub struct ReferenceCandidate<'text> {
    pub target_type: &'text str,
    pub id: &'text str,
    pub scope: Option<&'text str>,
    pub scope_id: Option<&'text str>,
    pub context: Option<&'text str>,
    pub relation: &'text str,
    pub field: Option<&'text str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferenceTargetKind {
    Task,
    Session,
    Memory,
    Knowledge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogicalReference<'text> {
    pub target_kind: ReferenceTargetKind,
    pub scope: LogicalScope<'text>,
    pub id: &'text str,
    pub context: Option<&'text str>,
    pub relation: &'text str,
    pub field: Option<&'text str>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum AuthoredReferenceUpdate<'text> {
    Preserve,
    Replace(Vec<LogicalReference<'text>>),
}

#[derive(Debug)]
pub enum ReferenceProjectionError {
    Request(io::Error),
    ProjectId(io::Error),
    InvalidShape,
    UnknownTargetType,
    UnknownScope,
    MissingScopeId,
    EmptyUserId,
    EmptyContext,
    ContextNotAllowed,
    EmptyTargetId,
    EmptyRelation,
}

impl fmt::Display for ReferenceProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Request(_) => "reference request interrupted or expired",
            Self::ProjectId(_) => "invalid logical project identity",
            Self::InvalidShape => "references must be a sequence of complete tuples",
            Self::UnknownTargetType => "unknown reference target type",
            Self::UnknownScope => "unknown reference scope",
            Self::MissingScopeId => "reference to another scope kind requires scope_id",
            Self::EmptyUserId => "logical user identity must be nonempty",
            Self::EmptyContext => "knowledge context must be nonempty",
            Self::ContextNotAllowed => "only knowledge references accept a context",
            Self::EmptyTargetId => "reference target id must be nonempty",
            Self::EmptyRelation => "reference relation must be nonempty",
        })
    }
}

impl Error for ReferenceProjectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(error) | Self::ProjectId(error) => Some(error),
            _ => None,
        }
    }
}

/// Validate the complete batch before returning a single preserve/replace decision.
///
/// Output may outlive the entry slice, but never the text owner:
/// ```compile_fail
/// use graphit_records::reference_projection::*;
/// use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
/// use std::time::{Duration, Instant};
/// let original = SnapshotContext::new(Instant::now() + Duration::from_secs(30), Cancellation::default());
/// let output = {
///     let text = String::from("target");
///     let entries = [AuthoredReferenceEntry::Tuple(ReferenceCandidate {
///         target_type: "memory", id: &text, scope: None, scope_id: None,
///         context: None, relation: "supports", field: None,
///     })];
///     project_authored_references(&original,
///         SourceNamespace { scope: LogicalScope::User("subject"), context: None },
///         AuthoredReferencesField::Sequence(&entries)).unwrap()
/// };
/// println!("{output:?}");
/// ```
pub fn project_authored_references<'entries, 'text>(
    original: &SnapshotContext,
    source: SourceNamespace<'text>,
    field: AuthoredReferencesField<'entries, 'text>,
) -> Result<AuthoredReferenceUpdate<'text>, ReferenceProjectionError> {
    original
        .check()
        .map_err(ReferenceProjectionError::Request)?;
    if matches!(source.scope, LogicalScope::User("")) {
        return Err(ReferenceProjectionError::EmptyUserId);
    }
    if source.context == Some("") {
        return Err(ReferenceProjectionError::EmptyContext);
    }
    let update = match field {
        AuthoredReferencesField::Omitted => AuthoredReferenceUpdate::Preserve,
        AuthoredReferencesField::Null | AuthoredReferencesField::NonSequence => {
            return Err(ReferenceProjectionError::InvalidShape);
        }
        AuthoredReferencesField::Sequence(entries) => {
            let mut references = Vec::with_capacity(entries.len());
            for entry in entries {
                original
                    .check()
                    .map_err(ReferenceProjectionError::Request)?;
                let AuthoredReferenceEntry::Tuple(candidate) = entry else {
                    return Err(ReferenceProjectionError::InvalidShape);
                };
                references.push(project_reference(source, *candidate)?);
            }
            AuthoredReferenceUpdate::Replace(references)
        }
    };
    original
        .check()
        .map_err(ReferenceProjectionError::Request)?;
    Ok(update)
}

fn project_reference<'text>(
    source: SourceNamespace<'text>,
    candidate: ReferenceCandidate<'text>,
) -> Result<LogicalReference<'text>, ReferenceProjectionError> {
    use ReferenceProjectionError as E;
    let target_kind = match candidate.target_type {
        "task" => ReferenceTargetKind::Task,
        "session" => ReferenceTargetKind::Session,
        "memory" => ReferenceTargetKind::Memory,
        "knowledge" => ReferenceTargetKind::Knowledge,
        _ => return Err(E::UnknownTargetType),
    };
    if candidate.id.is_empty() {
        return Err(E::EmptyTargetId);
    }
    if candidate.relation.is_empty() {
        return Err(E::EmptyRelation);
    }
    let project = match candidate.scope {
        None => matches!(source.scope, LogicalScope::Project(_)),
        Some("project") => true,
        Some("user") => false,
        Some(_) => return Err(E::UnknownScope),
    };
    let scope = match (project, candidate.scope_id) {
        (true, Some(id)) => LogicalScope::Project(id.parse().map_err(E::ProjectId)?),
        (false, Some("")) => return Err(E::EmptyUserId),
        (false, Some(id)) => LogicalScope::User(id),
        (chosen, None) if chosen == matches!(source.scope, LogicalScope::Project(_)) => {
            source.scope
        }
        (_, None) => return Err(E::MissingScopeId),
    };
    let context = match (target_kind, candidate.context) {
        (ReferenceTargetKind::Knowledge, Some("")) => return Err(E::EmptyContext),
        (ReferenceTargetKind::Knowledge, Some(context)) => Some(context),
        (ReferenceTargetKind::Knowledge, None) if scope == source.scope => source.context,
        (ReferenceTargetKind::Knowledge, None) => None,
        (_, Some(_)) => return Err(E::ContextNotAllowed),
        (_, None) => None,
    };
    Ok(LogicalReference {
        target_kind,
        scope,
        id: candidate.id,
        context,
        relation: candidate.relation,
        field: candidate.field,
    })
}
