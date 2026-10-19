//! Pure Task transitions over immutable observations, without store or authority.
//!
//! A delta proposes data. The live consumer must independently enforce complete
//! fresh observations, admission, ownership, fencing, CAS and durable history.
//!
//! The public example proposes a priority change using the original context:
//!
//! ```no_run
//! use graphit_core::file_io::SnapshotContext;
//! use graphit_records::task_spec::{
//!     plan_revision, FieldUpdate, RevisionCommand, TaskSpecDelta,
//!     TaskSpecError, TaskSpecification,
//! };
//! fn propose_priority<'s>(
//!     observed: &'s TaskSpecification,
//!     expected_revision: u64,
//!     original: &SnapshotContext,
//! ) -> Result<TaskSpecDelta<'s>, TaskSpecError> {
//!     plan_revision(observed, RevisionCommand {
//!         expected_revision, title: FieldUpdate::Keep,
//!         description: FieldUpdate::Keep, task_type: FieldUpdate::Keep,
//!         schedule: FieldUpdate::Keep, priority: Some(3), add_checks: &[], actor: "observed-actor",
//!         reason: "priority changed", occurred_at: 1,
//!     }, original)
//! }
//! ```

use crate::task_schedule::TaskSchedule;
use graphit_core::file_io::SnapshotContext;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt, io,
    num::NonZeroU128,
};

/// An observed Task identifier; construction does not generate an identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TaskId(NonZeroU128);
impl TaskId {
    pub const fn new(value: NonZeroU128) -> Self {
        Self(value)
    }
    pub const fn get(self) -> NonZeroU128 {
        self.0
    }
}

/// An observed check identifier, distinct from Task identifiers.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CheckId(NonZeroU128);
impl CheckId {
    pub const fn new(value: NonZeroU128) -> Self {
        Self(value)
    }
    pub const fn get(self) -> NonZeroU128 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    Acceptance,
    Test,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pending,
    Passed,
    Failed,
    Superseded,
}

impl Serialize for TaskId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&ulid::Ulid::from(self.0.get()))
    }
}
impl<'de> Deserialize<'de> for TaskId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_identifier(deserializer).map(Self)
    }
}

impl Serialize for CheckId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&ulid::Ulid::from(self.0.get()))
    }
}
impl<'de> Deserialize<'de> for CheckId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_identifier(deserializer).map(Self)
    }
}

pub(crate) fn deserialize_identifier<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<NonZeroU128, D::Error> {
    let value = String::deserialize(deserializer)?;
    let parsed = value
        .parse::<ulid::Ulid>()
        .map_err(serde::de::Error::custom)?;
    if parsed.to_string() != value {
        return Err(serde::de::Error::custom(
            "identifier must be a canonical ULID",
        ));
    }
    NonZeroU128::new(parsed.into())
        .ok_or_else(|| serde::de::Error::custom("identifier must be nonzero"))
}
impl TaskSpecification {
    /// Validate an observed specification without proposing a change or doing I/O.
    pub fn validate(&self, original: &SnapshotContext) -> Result<(), TaskSpecError> {
        validate(self, original).map(|_| ())
    }
}

/// A missing serde field uses Keep through #[serde(default)].
/// A present value, including null for Option<T>, becomes Set(value).
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum FieldUpdate<T> {
    #[default]
    Keep,
    Set(T),
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for FieldUpdate<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::Set)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckVerification {
    pub evidence: String,
    pub verified_by: String,
    pub occurred_at: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckSupersession {
    pub actor: String,
    pub reason: String,
    pub occurred_at: u64,
    pub replacement: Option<CheckId>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRecord {
    pub id: CheckId,
    pub kind: CheckKind,
    pub text: String,
    pub status: CheckStatus,
    pub verification: Option<CheckVerification>,
    pub supersession: Option<CheckSupersession>,
}
/// Observed data. Each planner validates its invariants before proposing output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSpecification {
    pub id: TaskId,
    pub revision: u64,
    pub title: String,
    pub description: String,
    pub task_type: String,
    pub priority: u8,
    pub schedule: Option<TaskSchedule>,
    pub checks: Vec<CheckRecord>,
}

#[derive(Clone, Copy, Debug)]
pub struct NewCheck<'a> {
    pub id: CheckId,
    pub kind: CheckKind,
    pub text: &'a str,
}
#[derive(Clone, Copy, Debug)]
pub struct ReplacementCheck<'a> {
    pub id: CheckId,
    pub text: &'a str,
    pub kind: Option<CheckKind>,
}
#[derive(Clone, Copy, Debug)]
pub struct RevisionCommand<'a> {
    pub expected_revision: u64,
    pub title: FieldUpdate<&'a str>,
    pub description: FieldUpdate<&'a str>,
    pub task_type: FieldUpdate<&'a str>,
    pub priority: Option<u8>,
    pub schedule: FieldUpdate<Option<TaskSchedule>>,
    pub add_checks: &'a [NewCheck<'a>],
    pub actor: &'a str,
    pub reason: &'a str,
    pub occurred_at: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct CheckVerificationCommand<'a> {
    pub expected_revision: u64,
    pub check_id: CheckId,
    pub passed: bool,
    pub evidence: &'a str,
    pub actor: &'a str,
    pub occurred_at: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct CheckSupersessionCommand<'a> {
    pub expected_revision: u64,
    pub check_id: CheckId,
    pub actor: &'a str,
    pub reason: &'a str,
    pub occurred_at: u64,
    pub replacement: Option<ReplacementCheck<'a>>,
}

#[derive(Debug)]
pub enum TaskSpecError {
    InvalidInput(&'static str),
    NoChange,
    ConflictObservation,
    UnknownCheck,
    InactiveCheck,
    DuplicateCheckId,
    DuplicateReplacement,
    MissingRequiredKind,
    RevisionOverflow,
    Request(io::Error),
}
impl fmt::Display for TaskSpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(reason) => write!(f, "invalid Task input: {reason}"),
            Self::Request(error) => write!(f, "original request failed: {error}"),
            other => write!(f, "Task transition rejected: {other:?}"),
        }
    }
}
impl Error for TaskSpecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskSpecAuditKind {
    Revision,
    CheckVerified {
        check_id: CheckId,
    },
    CheckSuperseded {
        check_id: CheckId,
        replacement: Option<CheckId>,
    },
}
#[derive(Debug, Eq, PartialEq)]
pub struct TaskSpecAudit {
    pub kind: TaskSpecAuditKind,
    pub actor: String,
    pub reason: String,
    pub occurred_at: u64,
}
/// Before remains borrowed and immutable; after/audit retain no command borrow.
#[derive(Debug)]
pub struct TaskSpecDelta<'s> {
    before: &'s TaskSpecification,
    after: TaskSpecification,
    audit: TaskSpecAudit,
}
impl<'s> TaskSpecDelta<'s> {
    pub fn before(&self) -> &'s TaskSpecification {
        self.before
    }
    pub fn after(&self) -> &TaskSpecification {
        &self.after
    }
    pub fn audit(&self) -> &TaskSpecAudit {
        &self.audit
    }
    /// Extracts every part for an external consumer to conserve and publish.
    /// This performs no authorization, fencing or storage CAS.
    pub fn into_parts(self) -> (&'s TaskSpecification, TaskSpecification, TaskSpecAudit) {
        (self.before, self.after, self.audit)
    }
}

fn check_request(original: &SnapshotContext) -> Result<(), TaskSpecError> {
    original.check().map_err(TaskSpecError::Request)
}

fn trim<'a>(
    value: &'a str,
    allow_empty: bool,
    original: &SnapshotContext,
) -> Result<&'a str, TaskSpecError> {
    check_request(original)?;
    let value = value.trim();
    check_request(original)?;
    if value.is_empty() && !allow_empty {
        return Err(TaskSpecError::InvalidInput("blank text"));
    }
    Ok(value)
}

fn canonical(value: &str, original: &SnapshotContext) -> Result<(), TaskSpecError> {
    if trim(value, false, original)? != value {
        return Err(TaskSpecError::InvalidInput("noncanonical observation"));
    }
    Ok(())
}

struct CheckIndex<'a> {
    ids: HashMap<CheckId, usize>,
    logical: HashSet<(CheckKind, &'a str)>,
}

fn validate<'a>(
    snapshot: &'a TaskSpecification,
    original: &SnapshotContext,
) -> Result<CheckIndex<'a>, TaskSpecError> {
    check_request(original)?;
    if snapshot.revision == 0 || snapshot.priority > 4 {
        return Err(TaskSpecError::InvalidInput("revision or priority"));
    }
    for value in [&snapshot.title, &snapshot.description, &snapshot.task_type] {
        canonical(value, original)?;
    }
    let mut ids = HashMap::with_capacity(snapshot.checks.len());
    let mut logical = HashSet::with_capacity(snapshot.checks.len());
    let mut acceptance = false;
    let mut test = false;
    for (index, check) in snapshot.checks.iter().enumerate() {
        check_request(original)?;
        canonical(&check.text, original)?;
        if let Some(verification) = &check.verification {
            canonical(&verification.evidence, original)?;
            canonical(&verification.verified_by, original)?;
        }
        if let Some(supersession) = &check.supersession {
            canonical(&supersession.actor, original)?;
            canonical(&supersession.reason, original)?;
        }
        let valid_state = match check.status {
            CheckStatus::Pending => check.verification.is_none() && check.supersession.is_none(),
            CheckStatus::Passed | CheckStatus::Failed => {
                check.verification.is_some() && check.supersession.is_none()
            }
            CheckStatus::Superseded => check.supersession.is_some(),
        };
        if !valid_state {
            return Err(TaskSpecError::InvalidInput("check state"));
        }
        if ids.insert(check.id, index).is_some() {
            return Err(TaskSpecError::DuplicateCheckId);
        }
        if !logical.insert((check.kind, check.text.as_str())) {
            return Err(TaskSpecError::InvalidInput("duplicate logical check"));
        }
        if check.status != CheckStatus::Superseded {
            match check.kind {
                CheckKind::Acceptance => acceptance = true,
                CheckKind::Test => test = true,
            }
        }
    }
    if !acceptance || !test {
        return Err(TaskSpecError::MissingRequiredKind);
    }
    // Each check is visited once. Completed historical chains are reused.
    let mut states = vec![0u8; snapshot.checks.len()];
    let mut path = Vec::new();
    for start in 0..snapshot.checks.len() {
        check_request(original)?;
        if states[start] != 0 {
            continue;
        }
        let mut cursor = start;
        loop {
            check_request(original)?;
            match states[cursor] {
                1 => return Err(TaskSpecError::InvalidInput("cyclic replacement")),
                2 => break,
                _ => {}
            }
            states[cursor] = 1;
            path.push(cursor);
            let Some(next) = snapshot.checks[cursor]
                .supersession
                .as_ref()
                .and_then(|meta| meta.replacement)
            else {
                break;
            };
            if next == snapshot.checks[cursor].id {
                return Err(TaskSpecError::InvalidInput("self replacement"));
            }
            cursor = *ids
                .get(&next)
                .ok_or(TaskSpecError::InvalidInput("orphan replacement"))?;
        }
        for index in path.drain(..) {
            check_request(original)?;
            states[index] = 2;
        }
    }
    check_request(original)?;
    Ok(CheckIndex { ids, logical })
}

fn revision(snapshot: &TaskSpecification, expected: u64) -> Result<u64, TaskSpecError> {
    if snapshot.revision != expected {
        return Err(TaskSpecError::ConflictObservation);
    }
    snapshot
        .revision
        .checked_add(1)
        .ok_or(TaskSpecError::RevisionOverflow)
}

fn active_target(
    snapshot: &TaskSpecification,
    ids: &HashMap<CheckId, usize>,
    id: CheckId,
) -> Result<usize, TaskSpecError> {
    let index = *ids.get(&id).ok_or(TaskSpecError::UnknownCheck)?;
    if snapshot.checks[index].status == CheckStatus::Superseded {
        return Err(TaskSpecError::InactiveCheck);
    }
    Ok(index)
}

fn field<'a>(
    observed: &'a str,
    update: FieldUpdate<&'a str>,
    original: &SnapshotContext,
) -> Result<&'a str, TaskSpecError> {
    match update {
        FieldUpdate::Keep => Ok(observed),
        FieldUpdate::Set(value) => trim(value, false, original),
    }
}

// Retain borrowed command text only during planning; select each addition once.
fn additions<'a>(
    mut index: CheckIndex<'a>,
    added: &'a [NewCheck<'a>],
    original: &SnapshotContext,
) -> Result<Vec<NewCheck<'a>>, TaskSpecError> {
    let mut selected_ids = HashSet::new();
    let mut selected = Vec::new();
    for candidate in added {
        let text = trim(candidate.text, true, original)?;
        if text.is_empty() || !index.logical.insert((candidate.kind, text)) {
            continue;
        }
        if index.ids.contains_key(&candidate.id) || !selected_ids.insert(candidate.id) {
            return Err(TaskSpecError::DuplicateCheckId);
        }
        selected.push(NewCheck { text, ..*candidate });
        check_request(original)?;
    }
    Ok(selected)
}

#[derive(Clone, Copy)]
enum Change<'a> {
    Preserve,
    Reset,
    Verify {
        id: CheckId,
        passed: bool,
        evidence: &'a str,
        actor: &'a str,
        at: u64,
    },
    Supersede {
        id: CheckId,
        actor: &'a str,
        reason: &'a str,
        at: u64,
        replacement: Option<CheckId>,
    },
}

fn own_check(
    old: &CheckRecord,
    change: Change<'_>,
    original: &SnapshotContext,
) -> Result<CheckRecord, TaskSpecError> {
    check_request(original)?;
    let (status, verification, supersession) = match change {
        Change::Reset if old.status != CheckStatus::Superseded => {
            (CheckStatus::Pending, None, None)
        }
        Change::Verify {
            id,
            passed,
            evidence,
            actor,
            at,
        } if old.id == id => (
            if passed {
                CheckStatus::Passed
            } else {
                CheckStatus::Failed
            },
            Some(CheckVerification {
                evidence: evidence.to_owned(),
                verified_by: actor.to_owned(),
                occurred_at: at,
            }),
            None,
        ),
        Change::Supersede {
            id,
            actor,
            reason,
            at,
            replacement,
        } if old.id == id => (
            CheckStatus::Superseded,
            old.verification.clone(),
            Some(CheckSupersession {
                actor: actor.to_owned(),
                reason: reason.to_owned(),
                occurred_at: at,
                replacement,
            }),
        ),
        _ => (
            old.status,
            old.verification.clone(),
            old.supersession.clone(),
        ),
    };
    let check = CheckRecord {
        id: old.id,
        kind: old.kind,
        text: old.text.clone(),
        status,
        verification,
        supersession,
    };
    check_request(original)?;
    Ok(check)
}

fn after_base(
    snapshot: &TaskSpecification,
    next_revision: u64,
    fields: [&str; 3],
    priority: u8,
    change: Change<'_>,
    original: &SnapshotContext,
) -> Result<TaskSpecification, TaskSpecError> {
    let mut checks = Vec::with_capacity(snapshot.checks.len());
    for old in &snapshot.checks {
        checks.push(own_check(old, change, original)?);
    }
    let after = TaskSpecification {
        id: snapshot.id,
        revision: next_revision,
        title: fields[0].to_owned(),
        description: fields[1].to_owned(),
        task_type: fields[2].to_owned(),
        priority,
        schedule: snapshot.schedule,
        checks,
    };
    check_request(original)?;
    Ok(after)
}

fn append_new(
    after: &mut TaskSpecification,
    check: NewCheck<'_>,
    original: &SnapshotContext,
) -> Result<(), TaskSpecError> {
    check_request(original)?;
    after.checks.push(CheckRecord {
        id: check.id,
        kind: check.kind,
        text: check.text.to_owned(),
        status: CheckStatus::Pending,
        verification: None,
        supersession: None,
    });
    check_request(original)
}

fn finish<'s>(
    before: &'s TaskSpecification,
    after: TaskSpecification,
    audit: TaskSpecAudit,
    original: &SnapshotContext,
) -> Result<TaskSpecDelta<'s>, TaskSpecError> {
    check_request(original)?;
    Ok(TaskSpecDelta {
        before,
        after,
        audit,
    })
}

/// Propose a revision. Effective title/description/type changes reset active checks.
pub fn plan_revision<'s>(
    observed: &'s TaskSpecification,
    command: RevisionCommand<'_>,
    original: &SnapshotContext,
) -> Result<TaskSpecDelta<'s>, TaskSpecError> {
    let index = validate(observed, original)?;
    let next_revision = revision(observed, command.expected_revision)?;
    let actor = trim(command.actor, false, original)?;
    let reason = trim(command.reason, false, original)?;
    let title = field(&observed.title, command.title, original)?;
    let description = field(&observed.description, command.description, original)?;
    let task_type = field(&observed.task_type, command.task_type, original)?;
    let reset = title != observed.title
        || description != observed.description
        || task_type != observed.task_type;
    let priority = command.priority.unwrap_or(observed.priority);
    if priority > 4 {
        return Err(TaskSpecError::InvalidInput("priority"));
    }
    let schedule = match command.schedule {
        FieldUpdate::Keep => observed.schedule,
        FieldUpdate::Set(value) => value,
    };
    let selected = additions(index, command.add_checks, original)?;
    if !reset
        && priority == observed.priority
        && schedule == observed.schedule
        && selected.is_empty()
    {
        return Err(TaskSpecError::NoChange);
    }
    let mut after = after_base(
        observed,
        next_revision,
        [title, description, task_type],
        priority,
        if reset {
            Change::Reset
        } else {
            Change::Preserve
        },
        original,
    )?;
    after.schedule = schedule;
    after.checks.reserve(selected.len());
    for candidate in selected {
        append_new(&mut after, candidate, original)?;
    }
    finish(
        observed,
        after,
        TaskSpecAudit {
            kind: TaskSpecAuditKind::Revision,
            actor: actor.to_owned(),
            reason: reason.to_owned(),
            occurred_at: command.occurred_at,
        },
        original,
    )
}

/// Propose a pass/fail event for a known active check; repeated events are allowed.
pub fn plan_check_verification<'s>(
    observed: &'s TaskSpecification,
    command: CheckVerificationCommand<'_>,
    original: &SnapshotContext,
) -> Result<TaskSpecDelta<'s>, TaskSpecError> {
    let index = validate(observed, original)?;
    let next_revision = revision(observed, command.expected_revision)?;
    let actor = trim(command.actor, false, original)?;
    let evidence = trim(command.evidence, false, original)?;
    active_target(observed, &index.ids, command.check_id)?;
    let after = after_base(
        observed,
        next_revision,
        [&observed.title, &observed.description, &observed.task_type],
        observed.priority,
        Change::Verify {
            id: command.check_id,
            passed: command.passed,
            evidence,
            actor,
            at: command.occurred_at,
        },
        original,
    )?;
    finish(
        observed,
        after,
        TaskSpecAudit {
            kind: TaskSpecAuditKind::CheckVerified {
                check_id: command.check_id,
            },
            actor: actor.to_owned(),
            reason: "verify".to_owned(),
            occurred_at: command.occurred_at,
        },
        original,
    )
}

/// Propose supersession, conserving verification and required active check kinds.
pub fn plan_check_supersession<'s>(
    observed: &'s TaskSpecification,
    command: CheckSupersessionCommand<'_>,
    original: &SnapshotContext,
) -> Result<TaskSpecDelta<'s>, TaskSpecError> {
    let index = validate(observed, original)?;
    let next_revision = revision(observed, command.expected_revision)?;
    let actor = trim(command.actor, false, original)?;
    let reason = trim(command.reason, false, original)?;
    let target = active_target(observed, &index.ids, command.check_id)?;
    let replacement = if let Some(replacement) = command.replacement {
        let text = trim(replacement.text, false, original)?;
        let kind = replacement.kind.unwrap_or(observed.checks[target].kind);
        check_request(original)?;
        if index.ids.contains_key(&replacement.id) || index.logical.contains(&(kind, text)) {
            return Err(TaskSpecError::DuplicateReplacement);
        }
        Some(NewCheck {
            id: replacement.id,
            kind,
            text,
        })
    } else {
        None
    };
    let mut acceptance = false;
    let mut test = false;
    for (index, check) in observed.checks.iter().enumerate() {
        check_request(original)?;
        if index != target && check.status != CheckStatus::Superseded {
            match check.kind {
                CheckKind::Acceptance => acceptance = true,
                CheckKind::Test => test = true,
            }
        }
    }
    if let Some(replacement) = replacement {
        match replacement.kind {
            CheckKind::Acceptance => acceptance = true,
            CheckKind::Test => test = true,
        }
    }
    if !acceptance || !test {
        return Err(TaskSpecError::MissingRequiredKind);
    }
    let replacement_id = replacement.map(|check| check.id);
    let mut after = after_base(
        observed,
        next_revision,
        [&observed.title, &observed.description, &observed.task_type],
        observed.priority,
        Change::Supersede {
            id: command.check_id,
            actor,
            reason,
            at: command.occurred_at,
            replacement: replacement_id,
        },
        original,
    )?;
    if let Some(replacement) = replacement {
        append_new(&mut after, replacement, original)?;
    }
    finish(
        observed,
        after,
        TaskSpecAudit {
            kind: TaskSpecAuditKind::CheckSuperseded {
                check_id: command.check_id,
                replacement: replacement_id,
            },
            actor: actor.to_owned(),
            reason: reason.to_owned(),
            occurred_at: command.occurred_at,
        },
        original,
    )
}

#[cfg(test)]
mod tests;
